package router

import (
	"testing"
	"time"
)

func TestLeasesAndARP(t *testing.T) {
	now := time.Unix(2000000000, 0)
	leases, bad := parseLeases([]byte("2000000060 02:00:00:00:00:01 192.0.2.10 example-pc *\n0 02:00:00:00:00:02 192.0.2.11 * *\n1999999990 02:00:00:00:00:03 192.0.2.12 expired *\nnot-a-lease\n"), now)
	if !bad || len(leases) != 2 || leases[0].Hostname != "example-pc" || leases[1].ExpiresAt != nil {
		t.Fatalf("%+v %v", leases, bad)
	}
	arp, bad := parseARP([]byte("IP address HW type Flags HW address Mask Device\n192.0.2.10 0x1 0x2 02:00:00:00:00:01 * br-lan\n192.0.2.20 0x1 0x2 02:00:00:00:00:20 * br-lan\n192.0.2.30 0x1 0x0 00:00:00:00:00:00 * br-lan\ninvalid\n"))
	rows := mergeDevices(leases, arp)
	if !bad || len(rows) != 3 || !rows[0].Online || rows[1].Online || !rows[2].Online || rows[2].Hostname != "" {
		t.Fatalf("%+v %v", rows, bad)
	}
	if _, bad := parseARP([]byte("unsupported header\n")); !bad {
		t.Fatal("accepted unsupported ARP source")
	}
}

func TestResolversAndFirewall(t *testing.T) {
	resolvers, bad := parseResolvers([]byte("# comment\nsearch example.invalid\nnameserver 192.0.2.53\nnameserver 2001:db8::53 # upstream\nnameserver 192.0.2.53\nnameserver not-an-ip\n"))
	if !bad || len(resolvers) != 2 || resolvers[1] != "2001:db8::53" {
		t.Fatalf("%+v %v", resolvers, bad)
	}
	fw, bad := parseFirewall([]byte("# synthetic\n*nat\n:PREROUTING ACCEPT [0:0]\n-A PREROUTING -j ACCEPT\nCOMMIT\n*filter\n:INPUT DROP [0:0]\n:FORWARD DROP [0:0]\n:OUTPUT ACCEPT [0:0]\n:owned - [0:0]\n-A INPUT -i lo -j ACCEPT\n-A FORWARD -m comment --comment DO-NOT-RETURN -j owned\nCOMMIT\n"))
	if bad || fw.Input != "DROP" || fw.Forward != "DROP" || fw.Output != "ACCEPT" || fw.Rules != 3 {
		t.Fatalf("%+v bad=%v", fw, bad)
	}
	if _, bad := parseFirewall([]byte("*filter\n:INPUT ACCEPT [0:0]\n")); !bad {
		t.Fatal("accepted truncated firewall output")
	}
	if _, bad := parseFirewall([]byte("not iptables save\n")); !bad {
		t.Fatal("accepted unsupported firewall")
	}
}
