use be6500_panel::endpoint_dns::{EndpointError, resolve};
use be6500_panel::readiness_dns::{
    QuestionType, ReadinessError, dns_query, dns_query_for, dns_response_addresses,
    validate_dns_response,
};
use std::{
    io::{Read, Write},
    net::{IpAddr, TcpListener, UdpSocket},
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};
fn answer(query: &[u8], records: &[(u16, &[u8])]) -> Vec<u8> {
    let mut response = query.to_vec();
    response[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
    response[6..8].copy_from_slice(&(records.len() as u16).to_be_bytes());
    for (kind, bytes) in records {
        response.extend_from_slice(&[0xc0, 0x0c]);
        response.extend_from_slice(&kind.to_be_bytes());
        response.extend_from_slice(&[0, 1, 0, 0, 0, 60]);
        response.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        response.extend_from_slice(bytes);
    }
    response
}
#[test]
fn exposes_only_reachable_answer_addresses_of_exact_question_type() {
    let query = dns_query("endpoint.example", 42).unwrap();
    let ip = [203, 0, 113, 9];
    let v6 = "2001:db8::9"
        .parse::<std::net::Ipv6Addr>()
        .unwrap()
        .octets();
    let response = answer(&query, &[(1, &ip), (28, &v6)]);
    assert_eq!(
        dns_response_addresses(&response, &query).unwrap(),
        vec![IpAddr::from(ip)]
    );
    assert_eq!(validate_dns_response(&response, &query), Ok(()));
    let query6 = dns_query_for("endpoint.example", 43, QuestionType::Aaaa).unwrap();
    let response6 = answer(&query6, &[(1, &ip), (28, &v6)]);
    assert_eq!(
        dns_response_addresses(&response6, &query6).unwrap(),
        vec![IpAddr::from(v6)]
    );
    assert_eq!(
        validate_dns_response(&response6, &query6),
        Err(ReadinessError::InvalidResponse)
    );
    let mut mismatched = response6.clone();
    mismatched[0] ^= 1;
    assert!(dns_response_addresses(&mismatched, &query6).is_err());
    let mut additional = answer(&query, &[]);
    additional[10..12].copy_from_slice(&1u16.to_be_bytes());
    let row = answer(&query, &[(1, &ip)]);
    additional.extend_from_slice(&row[query.len()..]);
    assert_eq!(
        dns_response_addresses(&additional, &query),
        Err(ReadinessError::NoAnswer)
    );
}
#[test]
fn endpoint_lookup_uses_actual_a_aaaa_and_checked_tcp_fallback() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = tcp.local_addr().unwrap();
    let udp = UdpSocket::bind(address).unwrap();
    udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let responder = thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let (n, peer) = udp.recv_from(&mut buffer).unwrap();
        let query = buffer[..n].to_vec();
        assert_eq!(&query[n - 4..], &[0, 1, 0, 1]);
        let mut truncated = answer(&query, &[]);
        truncated[2..4].copy_from_slice(&0x8380u16.to_be_bytes());
        udp.send_to(&truncated, peer).unwrap();
        let mut stream = tcp.accept().unwrap().0;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut size = [0u8; 2];
        stream.read_exact(&mut size).unwrap();
        let mut framed = vec![0u8; u16::from_be_bytes(size) as usize];
        stream.read_exact(&mut framed).unwrap();
        assert_eq!(framed, query);
        let response = answer(&framed, &[(1, &[203, 0, 113, 9])]);
        stream
            .write_all(&(response.len() as u16).to_be_bytes())
            .unwrap();
        stream.write_all(&response).unwrap();
        let (n, peer) = udp.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[n - 4..n], &[0, 28, 0, 1]);
        let v6 = "2001:db8::9"
            .parse::<std::net::Ipv6Addr>()
            .unwrap()
            .octets();
        udp.send_to(&answer(&buffer[..n], &[(28, &v6)]), peer)
            .unwrap();
    });
    let values = resolve(
        "endpoint.example",
        address,
        Instant::now() + Duration::from_secs(2),
        None,
    )
    .unwrap();
    assert_eq!(
        values,
        vec![
            "203.0.113.9".parse::<IpAddr>().unwrap(),
            "2001:db8::9".parse::<IpAddr>().unwrap()
        ]
    );
    responder.join().unwrap();
}
#[test]
fn literal_deadline_cancel_and_invalid_bootstrap_refuse_without_packets() {
    let address = "127.0.0.1:53".parse().unwrap();
    assert_eq!(
        resolve(
            "203.0.113.9",
            address,
            Instant::now() + Duration::from_secs(1),
            None
        )
        .unwrap(),
        vec!["203.0.113.9".parse::<IpAddr>().unwrap()]
    );
    assert_eq!(
        resolve("endpoint.example", address, Instant::now(), None),
        Err(EndpointError::Deadline)
    );
    let canceled = AtomicBool::new(true);
    assert_eq!(
        resolve(
            "endpoint.example",
            address,
            Instant::now() + Duration::from_secs(1),
            Some(&canceled)
        ),
        Err(EndpointError::Canceled)
    );
    assert_eq!(
        resolve(
            "endpoint.example",
            "0.0.0.0:53".parse().unwrap(),
            Instant::now() + Duration::from_secs(1),
            None
        ),
        Err(EndpointError::Invalid)
    );
}

#[test]
fn endpoint_terminal_cname_must_be_in_answer_chain_and_no_conflicting_address() {
    let query = dns_query_for("endpoint.example", 77, QuestionType::Aaaa).unwrap();
    let cname = [
        5, b'a', b'l', b'i', b'a', b's', 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0,
    ];
    let address = "2001:db8::7"
        .parse::<std::net::Ipv6Addr>()
        .unwrap()
        .octets();
    let mut response = answer(&query, &[(5, &cname)]);
    response[6..8].copy_from_slice(&2u16.to_be_bytes());
    response.extend_from_slice(&cname);
    response.extend_from_slice(&[0, 28, 0, 1, 0, 0, 0, 60, 0, 16]);
    response.extend_from_slice(&address);
    assert_eq!(
        dns_response_addresses(&response, &query).unwrap(),
        vec![IpAddr::from(address)]
    );
    let wrong = answer(&query, &[(5, &cname), (28, &address)]);
    assert_eq!(
        dns_response_addresses(&wrong, &query),
        Err(ReadinessError::InvalidResponse)
    );
}
#[test]
fn unanswered_native_query_honors_one_deadline() {
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    let started = Instant::now();
    assert_eq!(
        resolve(
            "endpoint.example",
            peer.local_addr().unwrap(),
            started + Duration::from_millis(90),
            None
        ),
        Err(EndpointError::Deadline)
    );
    assert!(started.elapsed() < Duration::from_secs(1));
}
