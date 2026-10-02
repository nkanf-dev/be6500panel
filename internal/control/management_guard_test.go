package control

import "testing"

func TestLANManagementMigrationGuard(t *testing.T) {
	before := "config interface 'lan'\n option proto 'static'\n option device 'br-lan'\n option ipaddr '192.168.31.1'\n option netmask '255.255.255.0'\nconfig device 'bridge'\n option name 'br-lan'\n option type 'bridge'\n list ports 'eth1.4'\n"
	for _, next := range []string{
		"config interface 'lan'\n option proto 'static'\n option device 'br-lan'\n option ipaddr '192.168.32.1'\n",
		"config interface 'wan'\n option proto 'dhcp'\n",
		"config interface 'lan'\n option proto 'none'\n",
		"config interface 'lan'\n option proto 'static'\n option device 'eth1.4'\n option ipaddr '192.168.31.1'\n",
	} {
		if got := validateLANManagement(before, next); len(got) == 0 || got[0].Code != "management_migration_unavailable" {
			t.Fatal("unsafe management migration accepted")
		}
	}
	allowed := before + "config route 'static1'\n option interface 'lan'\n option target '10.10.0.0'\n option netmask '255.255.0.0'\n option gateway '192.168.31.2'\n"
	if got := validateLANManagement(before, allowed); len(got) > 0 {
		t.Fatalf("unrelated static route denied: %v", got)
	}
	changedBridge := "config interface 'lan'\n option proto 'static'\n option device 'br-lan'\n option ipaddr '192.168.31.1'\n option netmask '255.255.255.0'\nconfig device 'bridge'\n option name 'br-lan'\n option type 'bridge'\n list ports 'eth1.3'\n"
	if got := validateLANManagement(before, changedBridge); len(got) == 0 {
		t.Fatal("bridge membership changed without migration owner")
	}
}
