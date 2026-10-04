//! Pure synthetic Observer fixtures. No native reads, sockets or commands.
use be6500_panel::capture_lan::{
    LanError, MAX_DEVICES, MAX_MANAGEMENT_IPS, MAX_SOURCE_BYTES, MAX_SOURCE_ROWS, observe,
};
use be6500_panel::readiness_tun::{
    Budget, FileIdentity, Interface, InterfaceAddress, Ipv4Prefix, Observer, TunError,
};
use std::{
    collections::BTreeMap,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const HEADER: &str =
    "IP address       HW type     Flags       HW address            Mask     Device\n";
const MAC: &str = "02:00:00:00:00:01";

fn interface(name: &str, addresses: &[(&str, u8)]) -> Interface {
    Interface {
        name: name.into(),
        up: true,
        mtu: 1500,
        addresses: addresses
            .iter()
            .map(|(ip, bits)| InterfaceAddress {
                address: ip.parse().unwrap(),
                bits: *bits,
            })
            .collect(),
    }
}
struct Fake {
    interfaces: Vec<Interface>,
    files: BTreeMap<&'static str, Vec<u8>>,
    reads: Vec<String>,
    failures: BTreeMap<&'static str, TunError>,
    cancel_on_read: bool,
}
impl Fake {
    fn new(lease: &str, arp: &str) -> Self {
        Self {
            interfaces: vec![
                interface("br-lan", &[("192.168.31.1", 24), ("fd00::1", 64)]),
                interface("wan", &[("192.0.2.1", 24)]),
                interface("lo", &[("127.0.0.1", 8)]),
            ],
            files: BTreeMap::from([
                ("/tmp/dhcp.leases", lease.as_bytes().to_vec()),
                ("/proc/net/arp", format!("{HEADER}{arp}").into_bytes()),
            ]),
            reads: Vec::new(),
            failures: BTreeMap::new(),
            cancel_on_read: false,
        }
    }
}
impl Observer for Fake {
    fn read_file(
        &mut self,
        path: &Path,
        limit: usize,
        b: &Budget<'_>,
    ) -> Result<Vec<u8>, TunError> {
        b.check()?;
        assert_eq!(limit, MAX_SOURCE_BYTES);
        let path = path.to_str().unwrap();
        self.reads.push(path.to_owned());
        if self.cancel_on_read {
            b.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(error) = self.failures.get(path) {
            return Err(*error);
        }
        self.files.get(path).cloned().ok_or(TunError::Unavailable)
    }
    fn interfaces(&mut self, b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
        b.check()?;
        Ok(self.interfaces.clone())
    }
    fn read_link(&mut self, _: &Path, _: &Budget<'_>) -> Result<PathBuf, TunError> {
        panic!("LAN observer must not read links")
    }
    fn metadata(&mut self, _: &Path, _: bool, _: &Budget<'_>) -> Result<FileIdentity, TunError> {
        panic!("LAN observer must not inspect unrelated metadata")
    }
    fn list_dir(&mut self, _: &Path, _: usize, _: &Budget<'_>) -> Result<Vec<String>, TunError> {
        panic!("LAN observer must not scan directories")
    }
    fn ipv4_routes(&mut self, _: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
        panic!("LAN observer must not invent scope from route/cache data")
    }
}
fn budget(cancel: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(10),
        cancel,
    }
}
fn lease(expiry: u64, mac: &str, ip: &str) -> String {
    format!("{expiry} {mac} {ip} synthetic-host *\n")
}
fn arp(mac: &str, ip: &str, interface: &str) -> String {
    format!("{ip} 0x1 0x2 {mac} * {interface}\n")
}

#[test]
fn fresh_scope_uses_masks_and_retains_all_actual_interfaces_without_device_reads() {
    let cancel = AtomicBool::new(false);
    let mut fake = Fake::new("", "");
    fake.files.clear();
    fake.interfaces
        .push(interface("guest", &[("192.168.50.1", 24)]));
    fake.interfaces.last_mut().unwrap().up = false;
    let snapshot = observe(&mut fake, false, 100, &budget(&cancel)).unwrap();
    assert_eq!(snapshot.lan_ipv4_prefixes, ["192.168.31.0/24"]);
    assert!(snapshot.lan_addresses.contains(&"192.168.31.1".into()));
    assert!(snapshot.lan_addresses.contains(&"fd00::1".into()));
    assert!(snapshot.management_ips.contains(&"192.168.50.1".into()));
    assert_eq!(snapshot.interface_addresses.len(), 4);
    assert!(!snapshot.interface_addresses[3].up);
    assert_eq!(snapshot.interface_addresses[0].addresses[0].bits, 24);
    assert!(snapshot.devices.is_empty());
    assert!(fake.reads.is_empty());
    assert!(!format!("{snapshot:?}").contains("192.168"));
}

#[test]
fn br_lan_missing_down_or_only_host_prefix_cannot_authorize_scope() {
    let cancel = AtomicBool::new(false);
    for kind in [
        "missing",
        "down",
        "host",
        "default",
        "bad-mask",
        "duplicate",
    ] {
        let mut fake = Fake::new("", "");
        match kind {
            "missing" => {
                fake.interfaces.remove(0);
            }
            "down" => fake.interfaces[0].up = false,
            "host" => fake.interfaces[0].addresses[0].bits = 32,
            "default" => fake.interfaces[0].addresses[0].bits = 0,
            "bad-mask" => fake.interfaces[0].addresses[0].bits = 33,
            "duplicate" => fake.interfaces.push(fake.interfaces[0].clone()),
            _ => unreachable!(),
        }
        assert!(
            observe(&mut fake, false, 100, &budget(&cancel)).is_err(),
            "{kind}"
        );
        assert!(fake.reads.is_empty());
    }
}

#[test]
fn current_lease_or_complete_lan_arp_authorizes_but_expired_lease_does_not() {
    let cancel = AtomicBool::new(false);
    for expiry in [0, 101] {
        let mut fake = Fake::new(&lease(expiry, MAC, "192.168.31.20"), "");
        let snapshot = observe(&mut fake, true, 100, &budget(&cancel)).unwrap();
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].mac, MAC);
        assert_eq!(snapshot.devices[0].ip, "192.168.31.20");
        assert_eq!(fake.reads, ["/tmp/dhcp.leases", "/proc/net/arp"]);
        assert_eq!(format!("{:?}", snapshot.devices[0]), "Device([private])");
    }
    for expiry in [1, 100] {
        let mut fake = Fake::new(&lease(expiry, MAC, "192.168.31.20"), "");
        assert!(
            observe(&mut fake, true, 100, &budget(&cancel))
                .unwrap()
                .devices
                .is_empty()
        );
    }
    let mut fake = Fake::new(
        &lease(99, MAC, "192.168.31.20"),
        &arp(MAC, "192.168.31.21", "br-lan"),
    );
    let snapshot = observe(&mut fake, true, 100, &budget(&cancel)).unwrap();
    assert_eq!(snapshot.devices[0].ip, "192.168.31.21");
    let mut fake = Fake::new("", "192.168.31.20 0x1 0x0 00:00:00:00:00:00 * br-lan\n");
    assert!(
        observe(&mut fake, true, 100, &budget(&cancel))
            .unwrap()
            .devices
            .is_empty()
    );
}

#[test]
fn guest_foreign_multiple_ips_and_multiple_mac_claims_never_authorize() {
    let cancel = AtomicBool::new(false);
    let other = "02:00:00:00:00:02";
    let cases = [
        (
            lease(0, MAC, "192.168.31.20"),
            arp(MAC, "192.168.31.20", "guest"),
        ),
        (String::new(), arp(MAC, "192.168.31.20", "guest")),
        (
            String::new(),
            arp(MAC, "192.168.31.20", "br-lan") + &arp(MAC, "192.168.31.20", "guest"),
        ),
        (
            String::new(),
            arp(MAC, "192.168.31.20", "br-lan") + &arp(MAC, "192.168.31.21", "br-lan"),
        ),
        (
            lease(0, MAC, "192.168.31.20") + &lease(0, MAC, "192.168.31.21"),
            String::new(),
        ),
        (
            lease(0, MAC, "192.168.31.20"),
            arp(other, "192.168.31.20", "br-lan"),
        ),
        (lease(0, MAC, "192.168.50.20"), String::new()),
    ];
    for (leases, arp) in cases {
        let mut fake = Fake::new(&leases, &arp);
        assert!(
            observe(&mut fake, true, 100, &budget(&cancel))
                .unwrap()
                .devices
                .is_empty()
        );
    }
}

#[test]
fn stale_arp_is_suppressed_but_its_ip_owner_conflict_is_not_erased() {
    let cancel = AtomicBool::new(false);
    let other = "02:00:00:00:00:02";
    let mut fake = Fake::new(
        &lease(0, MAC, "192.168.31.20"),
        &arp(MAC, "192.168.31.21", "br-lan"),
    );
    let snapshot = observe(&mut fake, true, 100, &budget(&cancel)).unwrap();
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(snapshot.devices[0].ip, "192.168.31.20");
    let leases = lease(0, MAC, "192.168.31.20") + &lease(0, other, "192.168.31.21");
    let mut fake = Fake::new(&leases, &arp(MAC, "192.168.31.21", "br-lan"));
    let snapshot = observe(&mut fake, true, 100, &budget(&cancel)).unwrap();
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(snapshot.devices[0].mac, MAC);
}

#[test]
fn network_broadcast_management_and_foreign_hosts_are_excluded_but_slash31_hosts_work() {
    let cancel = AtomicBool::new(false);
    for ip in [
        "192.168.31.0",
        "192.168.31.255",
        "192.168.31.1",
        "192.0.2.1",
        "127.0.0.2",
        "169.254.1.2",
    ] {
        let mut fake = Fake::new(&lease(0, MAC, ip), "");
        assert!(
            observe(&mut fake, true, 100, &budget(&cancel))
                .unwrap()
                .devices
                .is_empty(),
            "{ip}"
        );
    }
    let mut fake = Fake::new(&lease(0, MAC, "192.168.31.0"), "");
    fake.interfaces[0].addresses[0].bits = 31;
    let snapshot = observe(&mut fake, true, 100, &budget(&cancel)).unwrap();
    assert_eq!(snapshot.lan_ipv4_prefixes, ["192.168.31.0/31"]);
    assert_eq!(snapshot.devices[0].ip, "192.168.31.0");
}

#[test]
fn malformed_or_unavailable_identity_source_refuses_devices_not_gateway() {
    let cancel = AtomicBool::new(false);
    for line in [
        "bad row\n".into(),
        lease(0, "01:00:00:00:00:01", "192.168.31.20"),
        lease(0, "00:00:00:00:00:00", "192.168.31.20"),
        lease(0, MAC, "0.0.0.0"),
        lease(0, MAC, "224.0.0.1"),
        lease(253_402_300_800, MAC, "192.168.31.20"),
        lease(0, MAC, "192.168.31.20") + &lease(0, MAC, "192.168.31.20"),
    ] {
        let mut fake = Fake::new(&line, "");
        assert_eq!(
            observe(&mut fake, true, 100, &budget(&cancel)).err(),
            Some(LanError::Sources)
        );
        assert!(observe(&mut fake, false, 100, &budget(&cancel)).is_ok());
    }
    for raw in ["", "not an ARP table", "IP address HW address\nbad row\n"] {
        let mut fake = Fake::new(&lease(0, MAC, "192.168.31.20"), "");
        fake.files.insert("/proc/net/arp", raw.as_bytes().to_vec());
        assert_eq!(
            observe(&mut fake, true, 100, &budget(&cancel)).err(),
            Some(LanError::Sources)
        );
    }
    for path in ["/tmp/dhcp.leases", "/proc/net/arp"] {
        let mut fake = Fake::new(&lease(0, MAC, "192.168.31.20"), "");
        fake.files.remove(path);
        assert_eq!(
            observe(&mut fake, true, 100, &budget(&cancel)).err(),
            Some(LanError::Sources)
        );
    }
}

#[test]
fn fixed_lease_fallback_normalizes_mac_and_never_masks_invalid_source() {
    let cancel = AtomicBool::new(false);
    for spelling in ["02-00-00-00-00-AB", "0200.0000.00ab"] {
        let mut fake = Fake::new("", "");
        fake.files.remove("/tmp/dhcp.leases");
        fake.files.insert(
            "/tmp/dnsmasq.leases",
            lease(0, spelling, "192.168.31.20").into_bytes(),
        );
        let snapshot = observe(&mut fake, true, 100, &budget(&cancel)).unwrap();
        assert_eq!(snapshot.devices[0].mac, "02:00:00:00:00:ab");
        assert_eq!(
            fake.reads,
            ["/tmp/dhcp.leases", "/tmp/dnsmasq.leases", "/proc/net/arp"]
        );
    }
    let mut fake = Fake::new("bad row\n", "");
    fake.files.insert("/tmp/dnsmasq.leases", Vec::new());
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Sources)
    );
    assert_eq!(fake.reads, ["/tmp/dhcp.leases"]);
    fake.failures
        .insert("/tmp/dhcp.leases", TunError::Observation);
    fake.reads.clear();
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Sources)
    );
    assert_eq!(fake.reads, ["/tmp/dhcp.leases"]);
}

#[test]
fn device_source_row_and_interface_caps_fail_without_truncation() {
    let cancel = AtomicBool::new(false);
    for count in [MAX_DEVICES, MAX_DEVICES + 1] {
        let rows: String = (0..count)
            .map(|i| {
                lease(
                    0,
                    &format!("02:00:00:00:00:{:02x}", i + 1),
                    &format!("192.168.31.{}", i + 2),
                )
            })
            .collect();
        let mut fake = Fake::new(&rows, "");
        let result = observe(&mut fake, true, 100, &budget(&cancel));
        if count == MAX_DEVICES {
            assert_eq!(result.unwrap().devices.len(), count);
        } else {
            assert_eq!(result.err(), Some(LanError::Limit));
        }
    }
    let mut fake = Fake::new("", "");
    fake.files
        .insert("/tmp/dhcp.leases", vec![b' '; MAX_SOURCE_BYTES + 1]);
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Limit)
    );
    let mut fake = Fake::new(&"\n".repeat(MAX_SOURCE_ROWS + 1), "");
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Limit)
    );
    let mut fake = Fake::new("", "");
    fake.interfaces[1].addresses = (0..MAX_MANAGEMENT_IPS)
        .map(|i| InterfaceAddress {
            address: IpAddr::V4(std::net::Ipv4Addr::new(198, 51, 100, i as u8)),
            bits: 24,
        })
        .collect();
    assert_eq!(
        observe(&mut fake, false, 100, &budget(&cancel)).err(),
        Some(LanError::Limit)
    );
}

#[test]
fn absolute_budget_and_source_errors_are_fixed_and_private() {
    let cancel = AtomicBool::new(false);
    let mut fake = Fake::new(&lease(0, MAC, "192.168.31.20"), "");
    let expired = Budget {
        deadline: Instant::now(),
        cancel: &cancel,
    };
    assert_eq!(
        observe(&mut fake, true, 100, &expired).err(),
        Some(LanError::Deadline)
    );
    assert!(fake.reads.is_empty());
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        observe(&mut fake, false, 100, &budget(&cancel)).err(),
        Some(LanError::Cancelled)
    );
    cancel.store(false, Ordering::Relaxed);
    fake.cancel_on_read = true;
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Cancelled)
    );
    assert_eq!(fake.reads, ["/tmp/dhcp.leases"]);
    for error in [
        LanError::Sources,
        LanError::Scope,
        LanError::Limit,
        LanError::Deadline,
        LanError::Cancelled,
    ] {
        let text = format!("{error:?} {error}");
        for private in [MAC, "192.168.31.20", "synthetic-host", "/tmp/"] {
            assert!(!text.contains(private));
        }
    }
}

#[test]
fn prefix_boundaries_duplicate_arp_and_invalid_actual_addresses_are_checked() {
    let cancel = AtomicBool::new(false);
    for count in [8, 9] {
        let mut fake = Fake::new("", "");
        fake.interfaces[0].addresses = (0..count)
            .map(|i| InterfaceAddress {
                address: format!("192.168.{i}.1").parse().unwrap(),
                bits: 24,
            })
            .collect();
        let result = observe(&mut fake, false, 100, &budget(&cancel));
        if count == 8 {
            assert_eq!(result.unwrap().lan_ipv4_prefixes.len(), 8);
        } else {
            assert_eq!(result.err(), Some(LanError::Limit));
        }
    }
    let same = arp(MAC, "192.168.31.20", "br-lan");
    let mut fake = Fake::new("", &(same.clone() + &same));
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel))
            .unwrap()
            .devices
            .len(),
        1
    );
    for (ip, bits) in [("::ffff:192.0.2.1", 128), ("fd00::1", 129)] {
        let mut fake = Fake::new("", "");
        fake.interfaces.push(interface("other", &[(ip, bits)]));
        assert_eq!(
            observe(&mut fake, false, 100, &budget(&cancel)).err(),
            Some(LanError::Sources)
        );
    }
}

#[test]
fn expired_source_rows_still_count_and_arp_source_limit_is_independent() {
    let cancel = AtomicBool::new(false);
    let expired = lease(1, MAC, "192.168.31.20");
    let mut fake = Fake::new(&expired.repeat(MAX_SOURCE_ROWS), "");
    assert!(
        observe(&mut fake, true, 100, &budget(&cancel))
            .unwrap()
            .devices
            .is_empty()
    );
    let mut fake = Fake::new(&expired.repeat(MAX_SOURCE_ROWS + 1), "");
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Limit)
    );
    let incomplete = "192.168.31.20 0x1 0x0 00:00:00:00:00:00 * br-lan\n";
    let mut fake = Fake::new("", &incomplete.repeat(MAX_SOURCE_ROWS + 1));
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Limit)
    );
    let mut fake = Fake::new("", "");
    fake.files
        .insert("/proc/net/arp", vec![b' '; MAX_SOURCE_BYTES + 1]);
    assert_eq!(
        observe(&mut fake, true, 100, &budget(&cancel)).err(),
        Some(LanError::Limit)
    );
}

#[test]
fn output_is_numeric_ip_order_and_management_exact_cap_is_supported() {
    let cancel = AtomicBool::new(false);
    let leases = lease(0, "02:00:00:00:00:01", "192.168.31.100")
        + &lease(0, "02:00:00:00:00:02", "192.168.31.9");
    let mut fake = Fake::new(&leases, "");
    let snapshot = observe(&mut fake, true, 100, &budget(&cancel)).unwrap();
    let ips: Vec<_> = snapshot.devices.iter().map(|d| d.ip.as_str()).collect();
    assert_eq!(ips, ["192.168.31.9", "192.168.31.100"]);
    let mut fake = Fake::new("", "");
    fake.interfaces[1].addresses = (0..MAX_MANAGEMENT_IPS - 3)
        .map(|i| InterfaceAddress {
            address: IpAddr::V4(std::net::Ipv4Addr::new(198, 51, 100, i as u8)),
            bits: 24,
        })
        .collect();
    let snapshot = observe(&mut fake, false, 100, &budget(&cancel)).unwrap();
    assert_eq!(snapshot.management_ips.len(), MAX_MANAGEMENT_IPS);
}
