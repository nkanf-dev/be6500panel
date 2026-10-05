#![cfg(unix)]
use be6500_panel::{
    capture_input::{InputError, build_from_observation, observe_and_build},
    capture_lan::{Device, Snapshot},
    capture_state::{Desired, DeviceSelection},
    readiness_tun::{Budget, Interface, InterfaceAddress},
};
use serde_json::Value;
use std::{
    net::IpAddr,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn accepted() -> Value {
    let fixtures: Value = serde_json::from_str(include_str!("fixtures/native.json")).unwrap();
    let mut config: Value =
        serde_json::from_str(fixtures["cases"][0]["config"].as_str().unwrap()).unwrap();
    config["inbounds"][2]["listen"] = "192.168.31.1".into();
    config
}
fn observed() -> Snapshot {
    Snapshot {
        lan_ipv4_prefixes: vec!["192.168.31.0/24".into()],
        lan_addresses: vec!["192.168.31.1".into()],
        management_ips: vec![
            "127.0.0.1".into(),
            "192.168.31.1".into(),
            "203.0.113.2".into(),
        ],
        interface_addresses: vec![Interface {
            name: "br-lan".into(),
            up: true,
            mtu: 1500,
            addresses: vec![InterfaceAddress {
                address: "192.168.31.1".parse().unwrap(),
                bits: 24,
            }],
        }],
        devices: vec![Device {
            mac: "02:aa:bb:cc:dd:01".into(),
            ip: "192.168.31.50".into(),
        }],
    }
}
fn gateway() -> Desired {
    Desired {
        scope: "gateway".into(),
        desired: true,
        lan_ipv4_prefixes: vec!["192.168.31.50/32".into()],
        ..Desired::default()
    }
}
#[test]
fn declared_scope_and_accepted_settings_preserved_from_fresh_inputs() {
    let cancel = AtomicBool::new(false);
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(2),
        cancel: &cancel,
    };
    let input = build_from_observation(
        &gateway(),
        &serde_json::to_vec(&accepted()).unwrap(),
        &observed(),
        &budget,
        |host, bootstrap, _| {
            assert_eq!(host, "node.example");
            assert_eq!(bootstrap, "127.0.0.1:53".parse().unwrap());
            Ok(vec![
                "203.0.113.9".parse().unwrap(),
                "2001:db8::9".parse().unwrap(),
            ])
        },
    )
    .unwrap();
    assert_eq!(input.lan_ipv4_prefixes, ["192.168.31.50/32"]);
    assert_eq!(input.router_dns_addresses, ["192.168.31.1"]);
    assert_eq!(input.tun_address, "172.31.255.253/30");
    assert_eq!(input.ports.mixed, 2080);
    assert_eq!(input.ports.dns, 1053);
    assert!(input.endpoint_ips.contains(&"203.0.113.9".into()));
    assert!(input.endpoint_ips.contains(&"2001:db8::9".into()));
    assert!(!input.fake_ip);
}
#[test]
fn stale_scope_identity_and_unsupported_mode_are_refused() {
    let cancel = AtomicBool::new(false);
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(2),
        cancel: &cancel,
    };
    let raw = serde_json::to_vec(&accepted()).unwrap();
    let resolve = |_: &str, _, _: &Budget<'_>| Ok(vec!["203.0.113.9".parse::<IpAddr>().unwrap()]);
    let mut scope = gateway();
    scope.lan_ipv4_prefixes = vec!["192.168.0.0/16".into()];
    assert_eq!(
        build_from_observation(&scope, &raw, &observed(), &budget, resolve).err(),
        Some(InputError::Lan)
    );
    let mut device = Desired {
        scope: "devices".into(),
        desired: true,
        devices: vec![DeviceSelection {
            mac: "02:aa:bb:cc:dd:01".into(),
        }],
        ..Desired::default()
    };
    let input = build_from_observation(&device, &raw, &observed(), &budget, resolve).unwrap();
    assert_eq!(input.client_ipv4, "192.168.31.50");
    assert_eq!(
        input.client_macs.unwrap()["192.168.31.50"],
        "02:aa:bb:cc:dd:01"
    );
    let mut missing = observed();
    missing.devices.clear();
    assert_eq!(
        build_from_observation(&device, &raw, &missing, &budget, resolve).err(),
        Some(InputError::Identity)
    );
    device.devices.clear();
    device.client_ipv4 = "192.168.31.50".into();
    assert_eq!(
        build_from_observation(&device, &raw, &observed(), &budget, resolve).err(),
        Some(InputError::Intent)
    );
    let mut scope = gateway();
    scope.ipv6 = "block".into();
    assert_eq!(
        build_from_observation(&scope, &raw, &observed(), &budget, resolve).err(),
        Some(InputError::Intent)
    );
}
#[test]
fn exact_positive_tun_self_address_is_only_collision_exception() {
    let cancel = AtomicBool::new(false);
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(2),
        cancel: &cancel,
    };
    let raw = serde_json::to_vec(&accepted()).unwrap();
    let resolve = |_: &str, _, _: &Budget<'_>| Ok(vec!["203.0.113.9".parse::<IpAddr>().unwrap()]);
    let mut sample = observed();
    sample.management_ips.push("172.31.255.253".into());
    assert_eq!(
        build_from_observation(&gateway(), &raw, &sample, &budget, resolve).err(),
        Some(InputError::Lan)
    );
    sample.interface_addresses.push(Interface {
        name: "b6p-tun".into(),
        up: true,
        mtu: 1500,
        addresses: vec![InterfaceAddress {
            address: "172.31.255.253".parse().unwrap(),
            bits: 30,
        }],
    });
    let input = build_from_observation(&gateway(), &raw, &sample, &budget, resolve).unwrap();
    assert!(!input.management_ips.contains(&"172.31.255.253".into()));
    sample.interface_addresses[1].name = "foreign-tun".into();
    assert_eq!(
        build_from_observation(&gateway(), &raw, &sample, &budget, resolve).err(),
        Some(InputError::Lan)
    );
    assert_eq!(
        build_from_observation(&gateway(), &raw, &observed(), &budget, |_, _, _| Ok(vec![
            "172.31.255.254".parse().unwrap()
        ]))
        .err(),
        Some(InputError::Endpoint)
    );
}
#[test]
fn accepted_dns_bind_bootstrap_transport_and_cancel_are_authoritative() {
    let cancel = AtomicBool::new(false);
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(2),
        cancel: &cancel,
    };
    let resolve = |_: &str, _, _: &Budget<'_>| Ok(vec!["203.0.113.9".parse::<IpAddr>().unwrap()]);
    let mut config = accepted();
    config["inbounds"][2]["listen"] = "127.0.0.1".into();
    assert_eq!(
        build_from_observation(
            &gateway(),
            &serde_json::to_vec(&config).unwrap(),
            &observed(),
            &budget,
            resolve
        )
        .err(),
        Some(InputError::Config)
    );
    let mut config = accepted();
    config["dns"]["servers"][2]["type"] = "tls".into();
    assert_eq!(
        build_from_observation(
            &gateway(),
            &serde_json::to_vec(&config).unwrap(),
            &observed(),
            &budget,
            resolve
        )
        .err(),
        Some(InputError::Config)
    );
    let mut config = accepted();
    config["dns"]["servers"][2]["server"] = "127.0.0.1".into();
    config["dns"]["servers"][2]["server_port"] = 5353.into();
    build_from_observation(
        &gateway(),
        &serde_json::to_vec(&config).unwrap(),
        &observed(),
        &budget,
        |_, address, _| {
            assert_eq!(address, "127.0.0.1:5353".parse().unwrap());
            Ok(vec!["203.0.113.9".parse().unwrap()])
        },
    )
    .unwrap();
    let cancel = AtomicBool::new(true);
    let canceled = Budget {
        deadline: budget.deadline,
        cancel: &cancel,
    };
    assert_eq!(
        build_from_observation(&gateway(), &[], &observed(), &canceled, |_, _, _| panic!(
            "DNS on canceled call"
        ))
        .err(),
        Some(InputError::Canceled)
    );
    let mut off = gateway();
    off.desired = false;
    assert_eq!(
        build_from_observation(&off, &[], &observed(), &budget, |_, _, _| panic!(
            "DNS on off call"
        ))
        .err(),
        Some(InputError::Off)
    );
}
#[test]
fn off_native_builder_does_not_observe_or_query() {
    let mut observer =
        be6500_panel::readiness_tun::NativeObserver::with_proc_root("/missing-fixture".into());
    let cancel = AtomicBool::new(false);
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(1),
        cancel: &cancel,
    };
    assert_eq!(
        observe_and_build(&mut observer, &Desired::default(), &[], &budget).err(),
        Some(InputError::Off)
    );
}

#[test]
fn non_direct_dns_literal_cannot_be_stolen_by_connected_tun_prefix() {
    let cancel = AtomicBool::new(false);
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(2),
        cancel: &cancel,
    };
    let mut config = accepted();
    config["dns"]["servers"][1]["server"] = "172.31.255.254".into();
    assert_eq!(
        build_from_observation(
            &gateway(),
            &serde_json::to_vec(&config).unwrap(),
            &observed(),
            &budget,
            |_, _, _| panic!("DNS should not run for a known collision")
        )
        .err(),
        Some(InputError::Config)
    );
}
