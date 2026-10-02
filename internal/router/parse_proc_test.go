package router

import (
	"testing"
)

func TestIPv4RoutesByteOrderAndInvalid(t *testing.T) {
	rows, bad := parseIPv4Routes([]byte("Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0.2 000200C0 010200C0 0003 0 0 12 00FFFFFF 0 0 0\neth0 00000000 010200C0 0003 0 0 7 00000000 0 0 0\nbroken\n"))
	if !bad || len(rows) != 2 {
		t.Fatalf("rows=%+v bad=%v", rows, bad)
	}
	if rows[0].Destination != "192.0.2.0/24" || rows[0].Gateway != "192.0.2.1" || rows[0].Metric != 12 {
		t.Fatalf("route=%+v", rows[0])
	}
	if rows[1].Destination != "0.0.0.0/0" {
		t.Fatal(rows[1])
	}
	if _, bad := parseIPv4Routes([]byte("not a route header\n")); !bad {
		t.Fatal("accepted unsupported source")
	}
	rows, bad = parseIPv4Routes([]byte("Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0 00000000 010200C0 0003 0 0 1 00FF00FF 0 0 0\n"))
	if !bad || len(rows) != 0 {
		t.Fatalf("accepted noncontiguous mask: %+v", rows)
	}
}

func TestIPv6RoutesNetworkOrder(t *testing.T) {
	rows, bad := parseIPv6Routes([]byte("20010db8000100000000000000000000 40 00000000000000000000000000000000 00 fe800000000000000000000000000001 0000000a 00000000 00000000 00000003 eth0.2\n00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000000 00000000 00000000 00200200 lo\nbad\n"))
	if !bad || len(rows) != 1 {
		t.Fatalf("rows=%+v bad=%v", rows, bad)
	}
	if rows[0].Destination != "2001:db8:1::/64" || rows[0].Gateway != "fe80::1" || rows[0].Metric != 10 {
		t.Fatal(rows[0])
	}
	if rows, bad := parseIPv6Routes(nil); bad || len(rows) != 0 {
		t.Fatalf("empty IPv6 source: %v %v", rows, bad)
	}
}

func TestNetDevCounters(t *testing.T) {
	rows, bad := parseNetDev([]byte("Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n eth0.2: 18446744073709551615 0 0 0 0 0 0 0 600 0 0 0 0 0 0 0\n broken: nope\n"))
	if !bad || len(rows) != 1 || rows[0].RXBytes != ^uint64(0) || rows[0].TXBytes != 600 {
		t.Fatalf("rows=%+v bad=%v", rows, bad)
	}
	if _, bad := parseNetDev([]byte("unknown format\n")); !bad {
		t.Fatal("accepted unsupported counter format")
	}
}
