package devicetelemetry

import (
	"bytes"
	"encoding/json"
	"io"
	"math"
	"net"
	"sort"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
)

type wireCounter struct {
	IP string  `json:"ip"`
	HW string  `json:"hw"`
	RX *uint64 `json:"rx_bytes"`
	TX *uint64 `json:"tx_bytes"`
}
type wireDevice struct {
	HW             string        `json:"hw"`
	Hostname       string        `json:"hostname"`
	Interface      string        `json:"ifname"`
	Associated     *int          `json:"assoc"`
	OnlineSeconds  *uint64       `json:"online_timer"`
	AgeingSeconds  *uint64       `json:"ageing_timer"`
	MLD            *int          `json:"mld"`
	Signal         string        `json:"signal"`
	Noise          string        `json:"noise"`
	NegotiatedRX   string        `json:"nego_rx_rate"`
	NegotiatedTX   string        `json:"nego_tx_rate"`
	Protocol       string        `json:"wifiprotocol"`
	WirelessAgeing *uint64       `json:"wireless_ageing"`
	IPs            []wireCounter `json:"ip_list"`
}

func mac(value string) string {
	address, err := net.ParseMAC(value)
	if err != nil || len(address) != 6 {
		return ""
	}
	return strings.ToUpper(address.String())
}
func validText(value string, limit int) bool {
	if len(value) > limit || !utf8.ValidString(value) {
		return false
	}
	for _, r := range value {
		if unicode.IsControl(r) {
			return false
		}
	}
	return true
}

// ParseTrafficd accepts the real MAC-keyed trafficd hw detail response. Raw
// rx_rate/tx_rate are deliberately ignored: their vendor units and direction
// were not verified. Rates are derived later from timed byte counter deltas.
// Identical duplicate address rows are kept once; conflicting duplicates and
// malformed devices are skipped, never counted twice or converted to zero.
func ParseTrafficd(data []byte) ([]Observation, bool, error) {
	if len(data) > MaxSourceBytes {
		return nil, false, ErrSource
	}
	dec := json.NewDecoder(bytes.NewReader(data))
	first, err := dec.Token()
	if err != nil || first != json.Delim('{') {
		return nil, false, ErrSource
	}
	devices := make([]Observation, 0)
	byMAC := map[string]int{}
	keys := map[string]bool{}
	rejected := map[string]bool{}
	malformed := false
	rows := 0
	for dec.More() {
		key, err := dec.Token()
		if err != nil {
			return nil, false, ErrSource
		}
		rows++
		if rows > MaxSourceRows {
			return nil, false, ErrSource
		}
		var raw json.RawMessage
		if err = dec.Decode(&raw); err != nil {
			return nil, false, ErrSource
		}
		rowKey := key.(string)
		if keys[rowKey] {
			return nil, false, ErrSource
		}
		keys[rowKey] = true
		var w wireDevice
		id := ""
		if json.Unmarshal(raw, &w) == nil {
			id = mac(w.HW)
		}
		// Wireless MLO detail returns MAC-interface keys sharing identical counters.
		if id == "" || (strings.ToUpper(rowKey) != id && strings.ToUpper(rowKey) != id+"-"+strings.ToUpper(w.Interface)) || !validText(w.Hostname, 128) || !validText(w.Interface, 32) || (w.Associated == nil || (*w.Associated != 0 && *w.Associated != 1)) || len(w.IPs) > MaxAddresses || !validText(w.Protocol, 64) || !validText(w.NegotiatedRX, 64) || !validText(w.NegotiatedTX, 64) {
			if id != "" {
				rejected[id] = true
			}
			malformed = true
			continue
		}
		o := Observation{ID: id, Name: w.Hostname, Interface: w.Interface, Associated: *w.Associated == 1, OnlineSeconds: w.OnlineSeconds, AgeingSeconds: w.AgeingSeconds, Links: []WirelessLink{}, Counters: []Counter{}}
		if w.Protocol != "" || w.MLD != nil || w.Signal != "" || w.Noise != "" {
			link := WirelessLink{Interface: w.Interface, Protocol: w.Protocol, NegotiatedRX: w.NegotiatedRX, NegotiatedTX: w.NegotiatedTX, AgeingSeconds: w.WirelessAgeing}
			if w.MLD != nil && (*w.MLD == 0 || *w.MLD == 1) {
				mld := *w.MLD == 1
				link.MLD = &mld
			}
			for name, value := range map[string]string{"signal": w.Signal, "noise": w.Noise} {
				if value == "" {
					continue
				}
				n, err := strconv.Atoi(value)
				if err != nil || n > 0 || n < -150 {
					malformed = true
					continue
				}
				if name == "signal" {
					link.SignalDBM = &n
				} else {
					link.NoiseDBM = &n
				}
			}
			o.Links = append(o.Links, link)
		}
		addresses := map[string]Counter{}
		bad := false
		for _, ip := range w.IPs {
			parsed := net.ParseIP(ip.IP)
			if parsed == nil || (ip.HW != "" && mac(ip.HW) != id) || ip.RX == nil || ip.TX == nil {
				bad = true
				break
			}
			c := Counter{Address: parsed.String(), RX: *ip.RX, TX: *ip.TX}
			if prev, ok := addresses[c.Address]; ok {
				if c != prev {
					bad = true
					break
				}
				continue
			}
			addresses[c.Address] = c
		}
		if bad {
			rejected[id] = true
			malformed = true
			continue
		}
		var rx, tx uint64
		for _, c := range addresses {
			if math.MaxUint64-rx < c.RX || math.MaxUint64-tx < c.TX {
				bad = true
				break
			}
			rx += c.RX
			tx += c.TX
			o.Counters = append(o.Counters, c)
		}
		if bad {
			rejected[id] = true
			malformed = true
			continue
		}
		sort.Slice(o.Counters, func(i, j int) bool { return o.Counters[i].Address < o.Counters[j].Address })
		if index, ok := byMAC[id]; ok {
			existing := &devices[index]
			// Counters are whole-device counters duplicated per wireless link, not
			// per-link traffic. Any disagreement drops counter coverage for this MAC.
			if len(existing.Counters) != len(o.Counters) {
				rejected[id] = true
			}
			if !rejected[id] {
				for i, c := range o.Counters {
					if existing.Counters[i] != c {
						rejected[id] = true
						break
					}
				}
			}
			existing.Associated = existing.Associated || o.Associated
			if existing.Name == "" {
				existing.Name = o.Name
			}
			for _, link := range o.Links {
				duplicate := false
				for _, old := range existing.Links {
					if old.Interface == link.Interface {
						duplicate = true
						break
					}
				}
				if !duplicate {
					existing.Links = append(existing.Links, link)
				}
			}
			if existing.Interface != o.Interface && o.Interface != "" {
				interfaces := strings.Split(existing.Interface, " + ")
				found := false
				for _, name := range interfaces {
					if name == o.Interface {
						found = true
					}
				}
				if !found {
					interfaces = append(interfaces, o.Interface)
					sort.Strings(interfaces)
					existing.Interface = strings.Join(interfaces, " + ")
				}
			}
		} else {
			byMAC[id] = len(devices)
			devices = append(devices, o)
		}
	}
	for i := range devices {
		if rejected[devices[i].ID] {
			devices[i].Counters = []Counter{}
			malformed = true
		}
		sort.Slice(devices[i].Links, func(a, b int) bool { return devices[i].Links[a].Interface < devices[i].Links[b].Interface })
	}
	if _, err = dec.Token(); err != nil {
		return nil, false, ErrSource
	}
	if _, err = dec.Token(); err != io.EOF {
		return nil, false, ErrSource
	}
	sort.Slice(devices, func(i, j int) bool {
		if devices[i].Associated != devices[j].Associated {
			return devices[i].Associated
		}
		return devices[i].ID < devices[j].ID
	})
	truncated := len(devices) > MaxDevices
	if truncated {
		devices = devices[:MaxDevices]
	}
	if malformed {
		return devices, truncated, ErrSource
	}
	return devices, truncated, nil
}
