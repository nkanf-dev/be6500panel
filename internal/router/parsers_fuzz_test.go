package router

import (
	"encoding/json"
	"testing"
	"time"
)

func FuzzObservationParsers(f *testing.F) {
	for _, seed := range []string{"", "invalid\x00source", "config wifi-iface 'test'\noption ssid 'Example'\noption key 'SYNTHETIC-SECRET'\n", netDevFixture("18446744073709551615", "1")} {
		f.Add([]byte(seed))
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		if len(data) > fileLimit {
			return
		}
		wireless, bad := parseUCI(data, wirelessFields)
		rows, invalid := wirelessFromUCI(wireless)
		_ = bad
		_ = invalid
		_, _ = json.Marshal(rows)
		_, _ = parseAssignments(data, versionFields)
		_, _ = parseNetDev(data)
		_, _ = parseIPv4Routes(data)
		_, _ = parseIPv6Routes(data)
		_, _ = parseLeases(data, time.Unix(2000000000, 0))
		_, _ = parseARP(data)
		_, _ = parseResolvers(data)
		_, _ = parseFirewall(data)
	})
}
