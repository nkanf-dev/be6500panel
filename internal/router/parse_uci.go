package router

import (
	"strconv"
	"strings"
)

type uciSection struct {
	kind, name string
	options    map[string]string
	lists      map[string][]string
}

var wirelessFields = map[string]bool{"device": true, "ifname": true, "ssid": true, "band": true, "hwmode": true, "channel": true, "htmode": true, "bw": true, "disabled": true, "encryption": true}
var versionFields = map[string]bool{"HARDWARE": true, "ROM": true, "LINUX": true}

// UCI is parsed as data, not as shell code. Only allowed fields are retained.
func parseUCI(data []byte, allowed map[string]bool) ([]uciSection, bool) {
	sections := []uciSection{}
	bad := false
	counts := map[string]int{}
	for _, line := range strings.Split(string(data), "\n") {
		words, ok := uciWords(line)
		if !ok {
			bad = true
			continue
		}
		if len(words) == 0 {
			continue
		}
		switch words[0] {
		case "config":
			if len(words) < 2 || len(words) > 3 || !safeString(words[1], 64) {
				bad = true
				continue
			}
			name := "@" + words[1] + "[" + strconv.Itoa(counts[words[1]]) + "]"
			counts[words[1]]++
			if len(words) == 3 {
				if !safeString(words[2], 128) {
					bad = true
					continue
				}
				name = words[2]
			}
			sections = append(sections, uciSection{kind: words[1], name: name, options: map[string]string{}, lists: map[string][]string{}})
		case "option", "list":
			if len(words) != 3 || len(sections) == 0 {
				bad = true
				continue
			}
			if allowed[words[1]] {
				if !safeString(words[2], 256) && words[2] != "" {
					bad = true
					continue
				}
				section := &sections[len(sections)-1]
				if words[0] == "option" {
					section.options[words[1]] = words[2]
				} else {
					section.lists[words[1]] = append(section.lists[words[1]], words[2])
				}
			}
		default:
			bad = true
		}
	}
	if len(sections) == 0 {
		bad = true
	}
	return sections, bad
}

// Handles UCI single/double quotes, escaped quotes, and comments. No expansions.
func uciWords(line string) ([]string, bool) {
	words := []string{}
	var b strings.Builder
	quote := byte(0)
	token := false
	for i := 0; i < len(line); i++ {
		c := line[i]
		if quote != 0 {
			if c == quote {
				quote = 0
				continue
			}
			if c == '\\' && quote == '"' && i+1 < len(line) {
				i++
				b.WriteByte(line[i])
				continue
			}
			b.WriteByte(c)
			continue
		}
		if c == '#' {
			break
		}
		if c == '\'' || c == '"' {
			quote = c
			token = true
			continue
		}
		if c == '\\' {
			if i+1 >= len(line) {
				return nil, false
			}
			i++
			b.WriteByte(line[i])
			token = true
			continue
		}
		if c == ' ' || c == '\t' || c == '\r' {
			if token {
				words = append(words, b.String())
				b.Reset()
				token = false
			}
			continue
		}
		b.WriteByte(c)
		token = true
	}
	if quote != 0 {
		return nil, false
	}
	if token {
		words = append(words, b.String())
	}
	return words, true
}

func safeString(s string, max int) bool {
	if len(s) > max {
		return false
	}
	for _, c := range s {
		if c < 32 || c == 127 {
			return false
		}
	}
	return true
}

func wirelessFromUCI(sections []uciSection) ([]WiFi, bool) {
	out := []WiFi{}
	radios := map[string]uciSection{}
	bad := false
	for _, s := range sections {
		if s.kind == "wifi-device" {
			radios[s.name] = s
		}
	}
	for _, s := range sections {
		if s.kind != "wifi-iface" {
			continue
		}
		radio, ok := radios[s.options["device"]]
		if !ok {
			bad = true
		}
		name := s.options["ifname"]
		if name == "" {
			name = s.name
		}
		w := WiFi{Name: name, SSID: s.options["ssid"], Encryption: s.options["encryption"]}
		w.Band = wirelessBand(radio.options)
		channel := radio.options["channel"]
		if channel != "" && channel != "auto" {
			value, err := strconv.Atoi(channel)
			if err != nil || value < 0 || value > 233 {
				bad = true
			} else {
				w.Channel = value
			}
		}
		w.Bandwidth = radio.options["bw"]
		if w.Bandwidth == "" || w.Bandwidth == "0" {
			w.Bandwidth = radio.options["htmode"]
		}
		for _, disabled := range []string{s.options["disabled"], radio.options["disabled"]} {
			switch disabled {
			case "1":
				w.Disabled = true
			case "", "0":
			default:
				bad = true
			}
		}
		out = append(out, w)
	}
	return out, bad
}

func wirelessBand(o map[string]string) string {
	switch strings.ToLower(o["band"]) {
	case "2g", "2.4g", "2.4ghz":
		return "2.4GHz"
	case "5g", "5ghz":
		return "5GHz"
	case "6g", "6ghz":
		return "6GHz"
	}
	mode := strings.ToLower(o["hwmode"])
	if strings.HasSuffix(mode, "g") || mode == "11b" {
		return "2.4GHz"
	}
	if strings.HasSuffix(mode, "a") {
		return "5GHz"
	}
	return ""
}

func parseAssignments(data []byte, allowed map[string]bool) (map[string]string, bool) {
	values := map[string]string{}
	bad := false
	for _, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		key, value, ok := strings.Cut(line, "=")
		if !ok {
			bad = true
			continue
		}
		key = strings.TrimSpace(key)
		if !allowed[key] {
			continue
		}
		words, ok := uciWords(strings.TrimSpace(value))
		if !ok || len(words) != 1 || !safeString(words[0], 256) {
			bad = true
			continue
		}
		values[key] = words[0]
	}
	return values, bad
}
