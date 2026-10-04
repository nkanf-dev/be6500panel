use be6500_panel::readiness_dns::{
    ListenerTarget, Network, ReadinessError, dns_query, native_readiness_targets, probe_once,
    validate_dns_response, wait_readiness,
};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

type Reply = fn(&[u8]) -> Vec<u8>;

fn answer(query: &[u8]) -> Vec<u8> {
    let mut response = query.to_vec();
    response[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
    response[6..8].copy_from_slice(&1u16.to_be_bytes());
    response.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1]);
    response
}

#[test]
fn packet_answer_and_query_bounds() {
    let query = dns_query("BOOTSTRAP.test.", 0x1234).unwrap();
    assert_eq!(&query[..6], &[0x12, 0x34, 1, 0, 0, 1]);
    validate_dns_response(&answer(&query), &query).unwrap();
    for domain in [
        "",
        "192.0.2.1",
        "bad..test",
        "-bad.test",
        "bad-.test",
        "a b.test",
        "é.test",
    ] {
        assert!(dns_query(domain, 1).is_err());
    }
    assert!(dns_query(&format!("{}.test", "x".repeat(64)), 1).is_err());
    assert!(dns_query(&"x.".repeat(128), 1).is_err());
    let longest = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    assert!(dns_query(&longest, 1).is_ok());
}

#[test]
fn malformed_packets_never_pass() {
    let query = dns_query("bootstrap.test", 0x1234).unwrap();
    let valid = answer(&query);
    let mut bad = Vec::new();
    bad.push(valid[..11].to_vec());
    for (offset, value) in [
        (1, 0x35),
        (2, 1),
        (2, 0x89),
        (2, 0x83),
        (3, 0x82),
        (3, 0xc0),
        (5, 0),
        (5, 2),
        (13, b'z'),
        (query.len() - 3, 28),
        (query.len() - 1, 3),
        (query.len() + 3, 28),
        (query.len() + 5, 3),
        (query.len() + 1, 22),
        (query.len() + 1, query.len() as u8),
    ] {
        let mut packet = valid.clone();
        packet[offset] = value;
        bad.push(packet);
    }
    bad.push(valid[..valid.len() - 1].to_vec());
    let mut trailing = valid.clone();
    trailing.push(0);
    bad.push(trailing);
    let mut additional = valid.clone();
    additional[7] = 0;
    additional[11] = 1;
    bad.push(additional);
    let mut a_length = valid.clone();
    a_length[query.len() + 11] = 3;
    a_length.pop();
    bad.push(a_length);
    let mut no_answer = query.clone();
    no_answer[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
    bad.push(no_answer);
    bad.push(vec![0; 4097]);
    for packet in bad {
        assert!(
            validate_dns_response(&packet, &query).is_err(),
            "invalid packet passed"
        );
    }
}

#[test]
fn selects_go_scalar_and_array_fixtures_without_retired_tproxy() {
    let cases = [
        (
            r#"{"inbounds":[{"type":"mixed","listen_port":2080}]}"#,
            Network::Tcp,
            "127.0.0.1:2080",
            None,
        ),
        (
            r#"{"inbounds":[{"type":"direct","tag":"resolver","listen":"::","listen_port":1053,"network":"udp"}],"route":{"rules":[{"inbound":"resolver","action":"hijack-dns"}]}}"#,
            Network::Udp,
            "[::1]:1053",
            Some("dns.alidns.com"),
        ),
        (
            r#"{"inbounds":[{"type":"direct","listen_port":53,"network":["tcp"]}],"route":{"rules":[{"port":53,"action":"hijack-dns"}]},"dns":{"rules":[{"domain":"Node.TEST.","server":"dns-direct"}]}}"#,
            Network::Tcp,
            "127.0.0.1:53",
            Some("node.test"),
        ),
    ];
    for (raw, network, address, domain) in cases {
        let targets = native_readiness_targets(raw.as_bytes()).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].network, network);
        assert_eq!(targets[0].address, address.parse::<SocketAddr>().unwrap());
        assert_eq!(targets[0].domain.as_deref(), domain);
    }
    for raw in [
        r#"{"inbounds":[{"type":"mixed","listen":"private-host.test","listen_port":2080}]}"#,
        r#"{"inbounds":[{"type":"mixed","listen_port":0}]}"#,
        r#"{"inbounds":[{"type":"direct","tag":"dns-in","listen_port":1053}]}"#,
        r#"{"inbounds":[{"type":"direct","listen_port":53}]}"#,
        r#"{"inbounds":[{"type":"mixed","listen_port":2080,"network":"sctp"}]}"#,
        r#"{"inbounds":[{"type":"tproxy","listen_port":1053}]}"#,
        r#"{"inbounds":[{"type":"tun","tag":"tun-in"}]}"#,
        "{",
    ] {
        assert!(native_readiness_targets(raw.as_bytes()).is_err());
    }
}

fn config(address: SocketAddr, network: &str) -> Vec<u8> {
    format!(r#"{{"inbounds":[{{"type":"direct","tag":"dns-in","listen":"{}","listen_port":{},"network":"{}"}}],"route":{{"rules":[{{"inbound":"dns-in","action":"hijack-dns"}}]}},"dns":{{"rules":[{{"server":"dns-direct","domain":"bootstrap.test"}}]}}}}"#, address.ip(), address.port(), network).into_bytes()
}

// Every fake owns one finite thread. Socket deadlines and explicit stop channels
// ensure test cleanup cannot leave a blocked fake behind.
fn udp_fake(reply: fn(&[u8]) -> Vec<u8>) -> (SocketAddr, thread::JoinHandle<()>) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let address = socket.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let (size, peer) = socket.recv_from(&mut buffer).unwrap();
        let response = reply(&buffer[..size]);
        socket.send_to(&response, peer).unwrap();
    });
    (address, handle)
}

fn tcp_fake(reply: Option<Reply>) -> (SocketAddr, mpsc::Sender<()>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = mpsc::channel();
    let handle = thread::spawn(move || {
        let Some(mut socket) = accept_finite(&listener) else {
            return;
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut prefix = [0; 2];
        if socket.read_exact(&mut prefix).is_err() {
            return;
        }
        let mut query = vec![0; usize::from(u16::from_be_bytes(prefix))];
        socket.read_exact(&mut query).unwrap();
        if let Some(reply) = reply {
            let response = reply(&query);
            let prefix = (response.len() as u16).to_be_bytes();
            for chunk in [&prefix[..1], &prefix[1..], &response[..1], &response[1..]] {
                if socket.write_all(chunk).is_err() {
                    break;
                }
            }
        } else {
            let _ = stopped.recv_timeout(Duration::from_secs(2));
        }
    });
    (address, stop, handle)
}

#[test]
fn actual_queries_succeed_over_udp_and_fragmented_tcp() {
    let (address, handle) = udp_fake(answer);
    wait_readiness(
        &config(address, "udp"),
        Instant::now() + Duration::from_secs(1),
        None,
    )
    .unwrap();
    handle.join().unwrap();
    let (address, _stop, handle) = tcp_fake(Some(answer));
    wait_readiness(
        &config(address, "tcp"),
        Instant::now() + Duration::from_secs(1),
        None,
    )
    .unwrap();
    handle.join().unwrap();
}

#[test]
fn tcp_accept_without_dns_times_out_and_cancellation_interrupts_io() {
    let (address, stop, handle) = tcp_fake(None);
    let start = Instant::now();
    let error = wait_readiness(
        &config(address, "tcp"),
        start + Duration::from_millis(120),
        None,
    )
    .unwrap_err();
    assert_eq!(error, ReadinessError::Deadline);
    assert!(start.elapsed() < Duration::from_millis(500));
    let _ = stop.send(());
    handle.join().unwrap();

    for network in [Network::Udp, Network::Tcp] {
        let canceled = Arc::new(AtomicBool::new(false));
        let notify = Arc::clone(&canceled);
        let (address, stop, handle) = if network == Network::Tcp {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (stop, stopped) = mpsc::channel();
            let handle = thread::spawn(move || {
                let Some(mut socket) = accept_finite(&listener) else {
                    return;
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0; 512];
                assert!(socket.read(&mut request).unwrap() > 0);
                notify.store(true, Ordering::Release);
                let _ = stopped.recv_timeout(Duration::from_secs(2));
            });
            (address, stop, handle)
        } else {
            let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let address = socket.local_addr().unwrap();
            let (stop, stopped) = mpsc::channel();
            let handle = thread::spawn(move || {
                let mut request = [0; 512];
                socket.recv_from(&mut request).unwrap();
                notify.store(true, Ordering::Release);
                let _ = stopped.recv_timeout(Duration::from_secs(2));
            });
            (address, stop, handle)
        };
        let start = Instant::now();
        let target = ListenerTarget {
            network,
            address,
            domain: Some("bootstrap.test".to_owned()),
        };
        assert_eq!(
            probe_once(&target, start + Duration::from_secs(10), Some(&canceled)),
            Err(ReadinessError::Canceled)
        );
        assert!(start.elapsed() < Duration::from_millis(400));
        let _ = stop.send(());
        handle.join().unwrap();
    }
}

fn wire_name(domain: &str) -> Vec<u8> {
    let query = dns_query(domain, 1).unwrap();
    query[12..query.len() - 4].to_vec()
}
fn record(owner: &[u8], typ: u16, class: u16, data: &[u8]) -> Vec<u8> {
    let mut result = owner.to_vec();
    result.extend_from_slice(&typ.to_be_bytes());
    result.extend_from_slice(&class.to_be_bytes());
    result.extend_from_slice(&[0, 0, 0, 60]);
    result.extend_from_slice(&(data.len() as u16).to_be_bytes());
    result.extend_from_slice(data);
    result
}
fn records(query: &[u8], sections: &[Vec<Vec<u8>>; 3]) -> Vec<u8> {
    let mut response = query.to_vec();
    response[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
    for (section, records) in sections.iter().enumerate() {
        response[6 + section * 2..8 + section * 2]
            .copy_from_slice(&(records.len() as u16).to_be_bytes());
        for record in records {
            response.extend_from_slice(record);
        }
    }
    response
}

#[test]
fn cname_chain_requires_reachable_answer_and_every_section_is_validated() {
    let query = dns_query("bootstrap.test", 7).unwrap();
    let cname = record(&[0xc0, 12], 5, 1, &wire_name("alias.test"));
    let alias_cname = record(&wire_name("alias.test"), 5, 1, &wire_name("final.test"));
    let a = record(&wire_name("final.test"), 1, 1, &[192, 0, 2, 1]);
    validate_dns_response(
        &records(
            &query,
            &[
                vec![cname.clone(), alias_cname.clone(), a.clone()],
                vec![],
                vec![],
            ],
        ),
        &query,
    )
    .unwrap();
    let cases = [
        [vec![cname.clone()], vec![], vec![]],
        [
            vec![cname.clone(), alias_cname.clone()],
            vec![],
            vec![a.clone()],
        ],
        [
            vec![cname.clone(), alias_cname.clone()],
            vec![a.clone()],
            vec![],
        ],
        [
            vec![
                cname.clone(),
                record(&wire_name("alias.test"), 5, 1, &[0xc0, 12]),
            ],
            vec![],
            vec![],
        ],
        [vec![record(&[0xc0, 12], 5, 1, &[0xc0, 12])], vec![], vec![]],
        [vec![a.clone()], vec![], vec![]],
        [
            vec![
                cname.clone(),
                record(&[0xc0, 12], 1, 1, &[192, 0, 2, 1]),
                a.clone(),
            ],
            vec![],
            vec![],
        ],
        [
            vec![
                cname.clone(),
                record(&[0xc0, 12], 5, 1, &wire_name("conflict.test")),
                a.clone(),
            ],
            vec![],
            vec![],
        ],
        [
            vec![cname.clone(), alias_cname.clone(), a.clone()],
            vec![record(&[0xc0, 12], 1, 3, &[1, 2, 3])],
            vec![],
        ],
        [
            vec![cname.clone(), alias_cname.clone(), a.clone()],
            vec![],
            vec![record(&[0xc0, 12], 5, 3, &[1, b'z', 0, 0])],
        ],
        [
            vec![cname, alias_cname, a],
            vec![],
            vec![record(&[0xc0, 12], 5, 1, &[0xc0, 0])],
        ],
    ];
    for sections in cases {
        assert!(validate_dns_response(&records(&query, &sections), &query).is_err());
    }
}

#[test]
fn labels_pointers_opt_and_record_counts_are_bounded() {
    let query = dns_query("bootstrap.test", 1).unwrap();
    let a = record(&[0xc0, 12], 1, 1, &[192, 0, 2, 1]);
    let opt = |extended| {
        let mut rr = record(&[0], 41, 4096, &[]);
        rr[5] = extended;
        rr
    };
    validate_dns_response(
        &records(&query, &[vec![a.clone()], vec![], vec![opt(0)]]),
        &query,
    )
    .unwrap();
    for extra in [
        vec![opt(1)],
        vec![opt(0), opt(0)],
        vec![record(&[0], 41, 4096, &[0, 1, 0, 5, 0])],
        vec![record(&[0xc0, 12], 41, 4096, &[])],
        vec![record(&[0x40, 0], 2, 1, &[])],
        vec![record(&[1, b'.', 0], 2, 1, &[])],
        vec![record(&[1, 0x20, 0], 2, 1, &[])],
        vec![record(&[1, 0x7f, 0], 2, 1, &[])],
        vec![record(&[0xff, 0xff], 2, 1, &[])],
        vec![record(&[0xc0, 0], 2, 1, &[])],
    ] {
        assert!(
            validate_dns_response(&records(&query, &[vec![a.clone()], vec![], extra]), &query)
                .is_err()
        );
    }
    let mut too_long = Vec::new();
    for _ in 0..4 {
        too_long.push(63);
        too_long.extend_from_slice(&[b'x'; 63]);
    }
    too_long.push(0);
    assert!(
        validate_dns_response(
            &records(
                &query,
                &[vec![a], vec![], vec![record(&too_long, 2, 1, &[])]]
            ),
            &query
        )
        .is_err()
    );
    let mut absurd_count = answer(&query);
    absurd_count[6..8].copy_from_slice(&u16::MAX.to_be_bytes());
    assert!(validate_dns_response(&absurd_count, &query).is_err());
    // Pure deterministic garbage samples exercise parser bounds without I/O.
    let mut seed = 0x12345678u32;
    for size in 0..=4097 {
        let mut bytes = Vec::with_capacity(size);
        for _ in 0..size {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            bytes.push(seed as u8);
        }
        let _ = validate_dns_response(&bytes, &query);
    }
}

#[test]
fn bootstrap_selection_ports_and_limits_match_go_fixtures() {
    let cases = [
        (
            r#"{"rules":[{"domain":["proxy.test"],"server":"dns-proxy"},{"domain":["node.test"],"server":"dns-direct"}]}"#,
            "node.test",
        ),
        (
            r#"{"servers":[{"tag":"dns-direct","server":"223.5.5.5","tls":{"server_name":"dns.alidns.com"}}],"rules":[{"domain":["a-node.test","dns.alidns.com"],"server":"dns-direct"}]}"#,
            "dns.alidns.com",
        ),
        (
            r#"{"servers":[{"tag":"dns-direct","server":"223.5.5.5","tls":{"server_name":"resolver.test"}}]}"#,
            "resolver.test",
        ),
        (
            r#"{"servers":[{"tag":"dns-direct","server":"resolver.test"}]}"#,
            "resolver.test",
        ),
        (
            r#"{"rules":[{"domain":["192.0.2.1","-invalid.test","good.test"],"server":"dns-direct"}]}"#,
            "good.test",
        ),
        ("{}", "dns.alidns.com"),
    ];
    for (dns, expected) in cases {
        let raw = format!(
            r#"{{"inbounds":[{{"type":"mixed","listen_port":2080}},{{"type":"direct","tag":"dns-in","listen":"0.0.0.0","listen_port":1053}}],"route":{{"rules":[{{"inbound":["dns-in"],"port":[1053],"action":"hijack-dns"}}]}},"dns":{dns}}}"#
        );
        let targets = native_readiness_targets(raw.as_bytes()).unwrap();
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].domain, None);
        assert_eq!(targets[1].network, Network::Udp);
        assert_eq!(targets[2].network, Network::Tcp);
        assert_eq!(targets[1].domain.as_deref(), Some(expected));
        assert_eq!(targets[2].domain.as_deref(), Some(expected));
        let debug = format!("{targets:?}");
        assert!(!debug.contains(expected));
        assert!(!debug.contains("127.0.0.1"));
    }
    let raw = r#"{"inbounds":[{"type":"tun","tag":"tun-in"},{"type":"mixed","listen_port":2080}]}"#;
    assert_eq!(native_readiness_targets(raw.as_bytes()).unwrap().len(), 1);
    let raw = r#"{"inbounds":[{"type":"direct","tag":"dns-in","listen_port":1053}],"route":{"rules":[{"inbound":["other"],"action":"hijack-dns"}]}}"#;
    assert_eq!(
        native_readiness_targets(raw.as_bytes()).unwrap_err(),
        ReadinessError::MissingDnsRoute
    );
    assert_eq!(
        native_readiness_targets(&vec![b' '; (4 << 20) + 1]).unwrap_err(),
        ReadinessError::Config
    );
    let inbound = r#"{"type":"mixed","listen_port":2080}"#;
    for (count, valid) in [(64, true), (65, false)] {
        let raw = format!(r#"{{"inbounds":[{}]}}"#, vec![inbound; count].join(","));
        assert_eq!(native_readiness_targets(raw.as_bytes()).is_ok(), valid);
    }
    for (count, valid) in [(256, true), (257, false)] {
        let raw = format!(
            r#"{{"inbounds":[{{"type":"mixed","listen_port":2080,"network":[{}]}}]}}"#,
            vec!["\"tcp\""; count].join(",")
        );
        assert_eq!(native_readiness_targets(raw.as_bytes()).is_ok(), valid);
    }
    let private = "secret-config-value".repeat(100);
    let raw =
        format!(r#"{{"inbounds":[{{"type":"mixed","listen":"{private}","listen_port":2080}}]}}"#);
    let error = native_readiness_targets(raw.as_bytes()).unwrap_err();
    assert!(!format!("{error:?} {error}").contains("secret"));
}

fn accept_finite(listener: &TcpListener) -> Option<std::net::TcpStream> {
    listener.set_nonblocking(true).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        match listener.accept() {
            Ok((socket, _)) => {
                socket.set_nonblocking(false).unwrap();
                return Some(socket);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= end {
                    return None;
                }
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("fake accept failed: {error}"),
        }
    }
}

#[test]
fn dual_transport_selection_requires_both_actual_answers() {
    let (tcp_address, _stop, tcp_handle) = tcp_fake(Some(answer));
    let udp = UdpSocket::bind(tcp_address).unwrap();
    udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let udp_handle = thread::spawn(move || {
        let mut buffer = [0; 512];
        let (size, peer) = udp.recv_from(&mut buffer).unwrap();
        udp.send_to(&answer(&buffer[..size]), peer).unwrap();
    });
    wait_readiness(
        &config(tcp_address, ""),
        Instant::now() + Duration::from_secs(1),
        None,
    )
    .unwrap();
    tcp_handle.join().unwrap();
    udp_handle.join().unwrap();

    let (address, handle) = udp_fake(answer);
    assert_eq!(
        wait_readiness(
            &config(address, ""),
            Instant::now() + Duration::from_millis(100),
            None
        ),
        Err(ReadinessError::Deadline)
    );
    handle.join().unwrap();

    let (address, stop, handle) = tcp_fake(Some(answer));
    assert_eq!(
        wait_readiness(
            &config(address, ""),
            Instant::now() + Duration::from_millis(100),
            None
        ),
        Err(ReadinessError::Deadline)
    );
    let _ = stop.send(());
    handle.join().unwrap();
}

#[test]
fn mixed_listener_smoke_check_is_not_dns_success() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let raw = format!(
        r#"{{"inbounds":[{{"type":"mixed","listen":"{}","listen_port":{}}}]}}"#,
        address.ip(),
        address.port()
    );
    wait_readiness(
        raw.as_bytes(),
        Instant::now() + Duration::from_secs(1),
        None,
    )
    .unwrap();
    drop(listener);
    let canceled = AtomicBool::new(true);
    assert_eq!(
        wait_readiness(
            raw.as_bytes(),
            Instant::now() + Duration::from_secs(1),
            Some(&canceled)
        ),
        Err(ReadinessError::Canceled)
    );
    assert_eq!(
        wait_readiness(raw.as_bytes(), Instant::now(), None),
        Err(ReadinessError::Deadline)
    );
}

fn wrong_nonce(query: &[u8]) -> Vec<u8> {
    let mut reply = answer(query);
    reply[1] ^= 1;
    reply
}
fn oversized(_: &[u8]) -> Vec<u8> {
    vec![0; 4097]
}
fn short_response(_: &[u8]) -> Vec<u8> {
    vec![0; 11]
}
#[test]
fn fake_dns_wrong_nonce_and_size_fail_over_both_transports() {
    for reply in [
        wrong_nonce as fn(&[u8]) -> Vec<u8>,
        oversized,
        short_response,
    ] {
        let (address, handle) = udp_fake(reply);
        let target = ListenerTarget {
            network: Network::Udp,
            address,
            domain: Some("bootstrap.test".to_owned()),
        };
        assert_eq!(
            probe_once(&target, Instant::now() + Duration::from_secs(1), None),
            Err(ReadinessError::InvalidResponse)
        );
        handle.join().unwrap();
        let (address, _stop, handle) = tcp_fake(Some(reply));
        let target = ListenerTarget {
            network: Network::Tcp,
            address,
            domain: Some("bootstrap.test".to_owned()),
        };
        assert_eq!(
            probe_once(&target, Instant::now() + Duration::from_secs(1), None),
            Err(ReadinessError::InvalidResponse)
        );
        handle.join().unwrap();
    }
}

#[test]
fn fragmented_tcp_uses_one_absolute_deadline_and_exact_frame_length() {
    for fragment in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = mpsc::channel();
        let handle = thread::spawn(move || {
            let Some(mut stream) = accept_finite(&listener) else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut prefix = [0; 2];
            stream.read_exact(&mut prefix).unwrap();
            let mut query = vec![0; usize::from(u16::from_be_bytes(prefix))];
            stream.read_exact(&mut query).unwrap();
            let response = answer(&query);
            let prefix = (response.len() as u16).to_be_bytes();
            if fragment {
                stream.write_all(&prefix[..1]).unwrap();
                let _ = stopped.recv_timeout(Duration::from_millis(65));
                if stream.write_all(&prefix[1..]).is_err() {
                    return;
                }
                let _ = stopped.recv_timeout(Duration::from_millis(65));
                let _ = stream.write_all(&response);
            } else {
                stream.write_all(&prefix).unwrap();
                stream.write_all(&response[..response.len() - 1]).unwrap();
                // EOF cannot satisfy the announced exact frame length.
            }
        });
        let start = Instant::now();
        let target = ListenerTarget {
            network: Network::Tcp,
            address,
            domain: Some("bootstrap.test".to_owned()),
        };
        let result = probe_once(&target, start + Duration::from_millis(100), None);
        assert_eq!(
            result,
            Err(if fragment {
                ReadinessError::Deadline
            } else {
                ReadinessError::Io
            })
        );
        assert!(start.elapsed() < Duration::from_millis(400));
        let _ = stop.send(());
        handle.join().unwrap();
    }
}

#[test]
fn unrelated_cname_cycles_and_non_name_compression_targets_are_rejected() {
    let query = dns_query("bootstrap.test", 1).unwrap();
    let a = record(&[0xc0, 12], 1, 1, &[192, 0, 2, 1]);
    let invalid_pointer = record(&[0xc0, (query.len() - 4) as u8], 1, 1, &[192, 0, 2, 1]);
    assert!(
        validate_dns_response(
            &records(&query, &[vec![a.clone(), invalid_pointer], vec![], vec![]]),
            &query
        )
        .is_err()
    );
    let malformed_ns = record(&[0xc0, 12], 2, 1, &[0xc0]);
    assert!(
        validate_dns_response(
            &records(&query, &[vec![a.clone()], vec![malformed_ns], vec![]]),
            &query
        )
        .is_err()
    );
    let first = record(&wire_name("loop-a.test"), 5, 1, &wire_name("terminal.test"));
    let second = record(&wire_name("loop-a.test"), 5, 1, &wire_name("loop-b.test"));
    let third = record(&wire_name("loop-b.test"), 5, 1, &wire_name("loop-a.test"));
    assert!(
        validate_dns_response(
            &records(
                &query,
                &[vec![a.clone()], vec![first, second, third], vec![]]
            ),
            &query
        )
        .is_err()
    );
    let loop_a = record(&wire_name("loop-a.test"), 5, 1, &wire_name("loop-b.test"));
    let loop_b = record(&wire_name("loop-b.test"), 5, 1, &wire_name("loop-a.test"));
    assert!(
        validate_dns_response(
            &records(&query, &[vec![a, loop_a, loop_b], vec![], vec![]]),
            &query
        )
        .is_err()
    );
}

#[test]
fn compiled_configuration_over_256_rules_retains_dns_readiness_selection() {
    use be6500_panel::native::{CompileInput, compile_native};
    use be6500_panel::policy::{Rule, RuleKind, Target};
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native-go.json")).unwrap();
    let mut input: CompileInput =
        serde_json::from_value(fixtures["cases"][0]["input"].clone()).unwrap();
    input.rules = (0..512)
        .map(|index| Rule {
            kind: RuleKind::Domain,
            value: format!("direct-{index}.example.test"),
            target: Target::Direct,
            no_resolve: false,
            index,
        })
        .collect();
    let compiled = compile_native(&input).unwrap();
    let config: serde_json::Value = serde_json::from_slice(&compiled.config).unwrap();
    assert!(config["route"]["rules"].as_array().unwrap().len() > 256);
    assert!(config["dns"]["rules"].as_array().unwrap().len() > 256);
    let targets = native_readiness_targets(&compiled.config).unwrap();
    assert_eq!(targets.len(), 3);
    assert_eq!(
        targets
            .iter()
            .filter(|target| target.domain.is_some())
            .count(),
        2
    );
    assert!(
        targets
            .iter()
            .filter_map(|target| target.domain.as_deref())
            .all(|domain| domain == "dns.alidns.com")
    );
}

#[test]
fn large_readiness_lists_remain_bounded_and_field_order_independent() {
    let many = std::iter::repeat_n("{}", be6500_panel::policy::MAX_RULES * 2 + 129)
        .collect::<Vec<_>>()
        .join(",");
    let over_route = format!(
        r#"{{"inbounds":[{{"type":"mixed","listen_port":2080}}],"route":{{"rules":[{many}]}}}}"#
    );
    assert_eq!(
        native_readiness_targets(over_route.as_bytes()).unwrap_err(),
        ReadinessError::Config
    );
    let over_dns = format!(
        r#"{{"inbounds":[{{"type":"mixed","listen_port":2080}}],"dns":{{"rules":[{many}]}}}}"#
    );
    assert_eq!(
        native_readiness_targets(over_dns.as_bytes()).unwrap_err(),
        ReadinessError::Config
    );
    let dns = r#"{"rules":[{"domain":"first.test","server":"dns-direct"},{"domain":"resolver.test","server":"dns-direct"}],"servers":[{"tag":"dns-direct","tls":{"server_name":"resolver.test"}}]}"#;
    let config = format!(
        r#"{{"inbounds":[{{"type":"direct","tag":"dns-in","listen_port":1053}}],"dns":{dns},"route":{{"rules":[{{"action":"hijack-dns","inbound":"dns-in"}}]}}}}"#
    );
    let targets = native_readiness_targets(config.as_bytes()).unwrap();
    assert!(
        targets
            .iter()
            .all(|target| target.domain.as_deref() == Some("resolver.test"))
    );
    let duplicate = config.replace(
        r#""action":"hijack-dns""#,
        r#""action":"hijack-dns","action":"route""#,
    );
    assert_eq!(
        native_readiness_targets(duplicate.as_bytes()).unwrap_err(),
        ReadinessError::Config
    );
}
