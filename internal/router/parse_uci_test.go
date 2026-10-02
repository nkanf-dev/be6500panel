package router

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestWirelessSafeFieldsAndQuotes(t *testing.T) {
	data := []byte("config wifi-device 'wifi0'\n option hwmode '11beg'\n option channel 'auto'\n option htmode 'HT40'\n option bw '0'\nconfig wifi-device 'wifi1'\n option hwmode '11bea'\n option channel '44'\n option bw '160'\nconfig wifi-iface\n option device 'wifi0'\n option ifname 'wl1'\n option ssid 'Example '\\''Lab'\n option encryption 'psk2'\n option key 'DO-NOT-RETURN-PSK'\n option password 'DO-NOT-RETURN-PASSWORD'\n option disabled '0'\nconfig wifi-iface 'guest'\n option device 'wifi1'\n option ssid \"Example 5 GHz\" # comment\n option encryption 'none'\n option disabled '1'\n")
	sections, bad := parseUCI(data, wirelessFields)
	rows, invalid := wirelessFromUCI(sections)
	if bad || invalid || len(rows) != 2 {
		t.Fatalf("bad=%v invalid=%v rows=%+v", bad, invalid, rows)
	}
	if rows[0].SSID != "Example 'Lab" || rows[0].Band != "2.4GHz" || rows[0].Bandwidth != "HT40" || rows[0].Name != "wl1" {
		t.Fatalf("row=%+v", rows[0])
	}
	if rows[1].Band != "5GHz" || rows[1].Bandwidth != "160" || rows[1].Channel != 44 || !rows[1].Disabled {
		t.Fatal(rows[1])
	}
	encoded, _ := json.Marshal(sections)
	if strings.Contains(string(encoded), "DO-NOT-RETURN") {
		t.Fatal("parser retained secret")
	}
	encoded, _ = json.Marshal(rows)
	if strings.Contains(string(encoded), "DO-NOT-RETURN") {
		t.Fatal("wireless disclosed secret")
	}
}

func TestUCIMalformedAndVersion(t *testing.T) {
	sections, bad := parseUCI([]byte("config core 'version'\n option HARDWARE 'RN02'\n option ROM '9.8.7'\n option broken 'unterminated\n"), versionFields)
	if !bad || len(sections) != 1 || sections[0].options["HARDWARE"] != "RN02" || sections[0].options["ROM"] != "9.8.7" {
		t.Fatalf("bad=%v sections=%+v", bad, sections)
	}
	for _, input := range []string{"option ssid hi", "config wifi-device a\noption channel 99999\nconfig wifi-iface b\noption device a", "config wifi-iface x\noption device unknown"} {
		s, b := parseUCI([]byte(input), wirelessFields)
		_, invalid := wirelessFromUCI(s)
		if !b && !invalid {
			t.Fatalf("accepted %q", input)
		}
	}
}
