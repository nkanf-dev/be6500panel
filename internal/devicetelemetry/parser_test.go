package devicetelemetry

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strings"
	"testing"
)

func readFixture(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return data
}
func TestObservedShapeSyntheticFixtures(t *testing.T) {
	for _, name := range []string{"trafficd-detail-synthetic.json", "trafficd-wireless-synthetic.json"} {
		rows, truncated, err := ParseTrafficd(readFixture(t, name))
		if err != nil || truncated || len(rows) != 4 {
			t.Fatalf("%s rows=%+v truncated=%v err=%v", name, rows, truncated, err)
		}
		for _, row := range rows {
			if len(row.Counters) != 1 || !row.Associated || row.OnlineSeconds == nil || row.AgeingSeconds == nil {
				t.Fatal(row)
			}
		}
		if name == "trafficd-wireless-synthetic.json" {
			d := rows[0]
			if d.ID != "02:00:00:00:00:01" || len(d.Links) != 2 || d.Interface != "wl0 + wl1" || *d.Links[0].SignalDBM != -62 || d.Links[0].NegotiatedRX != "2161+154Mbps" || !*d.Links[0].MLD {
				t.Fatal(d)
			}
			if d.Counters[0].RX != 36945533 || d.Counters[0].TX != 6555760 {
				t.Fatal("MLO counters counted twice", d.Counters)
			}
		}
	}
}
func encodedDevice(t *testing.T, ips string) []byte {
	t.Helper()
	return []byte(`{"02:00:00:00:00:01":{"hw":"02:00:00:00:00:01","assoc":1,"hostname":"synthetic","online_timer":100,"ip_list":` + ips + `}}`)
}
func TestMultiAddressExactDuplicatesAndInvalidCounters(t *testing.T) {
	row := `{"ip":"192.0.2.1","rx_bytes":10,"tx_bytes":20}`
	data := encodedDevice(t, `[`+row+`,`+row+`,{"ip":"2001:db8::1","rx_bytes":30,"tx_bytes":40}]`)
	devices, _, err := ParseTrafficd(data)
	if err != nil || len(devices) != 1 || len(devices[0].Counters) != 2 {
		t.Fatal(devices, err)
	}
	rx, tx, valid := counterDelta([]Counter{{Address: "192.0.2.1", RX: 0, TX: 0}, {Address: "2001:db8::1", RX: 0, TX: 0}}, devices[0].Counters)
	if !valid || rx != 40 || tx != 60 {
		t.Fatal(rx, tx, valid)
	}
	for _, ips := range []string{
		`[` + row + `,{"ip":"192.0.2.1","rx_bytes":11,"tx_bytes":20}]`,
		`[{"ip":"192.0.2.1","tx_bytes":20}]`,
		`[{"ip":"192.0.2.1","rx_bytes":-1,"tx_bytes":20}]`,
		`[{"ip":"192.0.2.1","rx_bytes":1.5,"tx_bytes":20}]`,
		`[{"ip":"bad","rx_bytes":1,"tx_bytes":2}]`,
		`[{"ip":"192.0.2.1","rx_bytes":18446744073709551615,"tx_bytes":20},{"ip":"192.0.2.2","rx_bytes":1,"tx_bytes":2}]`,
	} {
		devices, _, err = ParseTrafficd(encodedDevice(t, ips))
		if !errors.Is(err, ErrSource) || len(devices) != 0 {
			t.Fatal(ips, devices, err)
		}
	}
}
func TestConflictingMLODoesNotInventCoverage(t *testing.T) {
	var fixture map[string]map[string]any
	if err := json.Unmarshal(readFixture(t, "trafficd-wireless-synthetic.json"), &fixture); err != nil {
		t.Fatal(err)
	}
	ips := fixture["02:00:00:00:00:01-wl1"]["ip_list"].([]any)
	ips[0].(map[string]any)["rx_bytes"] = float64(1)
	data, _ := json.Marshal(fixture)
	rows, _, err := ParseTrafficd(data)
	if !errors.Is(err, ErrSource) || len(rows) != 4 || len(rows[0].Counters) != 0 || len(rows[0].Links) != 2 {
		t.Fatal(rows, err)
	}
}
func TestParserBoundsAndStrictFraming(t *testing.T) {
	for _, data := range [][]byte{[]byte(`[]`), []byte(`null`), []byte(`{"bad":{}}`), []byte(`{} {}`), []byte(strings.Repeat("x", MaxSourceBytes+1)), []byte(`{"02:00:00:00:00:01":{},"02:00:00:00:00:01":{}}`)} {
		if _, _, err := ParseTrafficd(data); err == nil {
			t.Fatal("accepted invalid frame", string(data[:min(len(data), 80)]))
		}
	}
	rows := map[string]any{}
	for i := 0; i < 129; i++ {
		id := fmt.Sprintf("02:00:00:00:%02X:%02X", i/256, i%256)
		rows[id] = map[string]any{"hw": id, "assoc": 1, "ip_list": []any{}}
	}
	data, _ := json.Marshal(rows)
	parsed, truncated, err := ParseTrafficd(data)
	if err != nil || !truncated || len(parsed) != MaxDevices {
		t.Fatal(len(parsed), truncated, err)
	}
	for i := 129; i <= MaxSourceRows; i++ {
		id := fmt.Sprintf("02:00:00:00:%02X:%02X", i/256, i%256)
		rows[id] = map[string]any{"hw": id, "assoc": 1, "ip_list": []any{}}
	}
	data, _ = json.Marshal(rows)
	if _, _, err := ParseTrafficd(data); err == nil {
		t.Fatal("accepted excess source rows")
	}
}

func TestMalformedMLOSiblingDropsCounterCoverage(t *testing.T) {
	var fixture map[string]map[string]any
	if err := json.Unmarshal(readFixture(t, "trafficd-wireless-synthetic.json"), &fixture); err != nil {
		t.Fatal(err)
	}
	fixture["02:00:00:00:00:01-wl1"]["ip_list"] = []any{map[string]any{"ip": "192.0.2.11", "tx_bytes": 1}}
	data, _ := json.Marshal(fixture)
	rows, _, err := ParseTrafficd(data)
	if err == nil || len(rows) != 4 || len(rows[0].Counters) != 0 {
		t.Fatal(rows, err)
	}
}
