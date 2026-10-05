#![cfg(unix)]
use be6500_panel::readiness_dns::{self, ReadinessError};
use be6500_panel::readiness_tun::{
    self, Budget, FileIdentity, Interface, InterfaceAddress, Ipv4Prefix, NativeObserver, Observer,
    OwnedIdentity, OwnedObservation, OwnedStatus, TunError,
};
use be6500_panel::runtime_process::ServiceId;
use std::{
    fs,
    net::{Ipv4Addr, UdpSocket},
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};
const PID: u32 = 4567;
struct Fixture {
    root: PathBuf,
    artifact: PathBuf,
    native: NativeObserver,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("be6500-readiness-joint-{}", std::process::id()));
        assert!(!root.exists());
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("artifacts"))
            .unwrap();
        let artifact = root.join("artifacts/.artifact-fixture");
        fs::write(&artifact, b"private fixture").unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        let proc = root.join("proc");
        for sub in ["4567/fd", "4567/fdinfo", "net", "sys/net/ipv4/conf/b6p-tun"] {
            fs::create_dir_all(proc.join(sub)).unwrap();
        }
        fs::write(
            proc.join("4567/stat"),
            format!(
                "{PID} (fixture ) worker) S {} 123456\n",
                ["0"; 18].join(" ")
            ),
        )
        .unwrap();
        fs::write(proc.join("4567/fdinfo/7"), b"iff:\tb6p-tun\n").unwrap();
        symlink("/dev/net/tun", proc.join("4567/fd/7")).unwrap();
        symlink("socket:[111]", proc.join("4567/fd/8")).unwrap();
        symlink(&artifact, proc.join("4567/exe")).unwrap();
        fs::write(proc.join("sys/net/ipv4/conf/b6p-tun/rp_filter"), b"2\n").unwrap();
        let ip = u32::from_ne_bytes(Ipv4Addr::new(172, 31, 255, 253).octets());
        fs::write(proc.join("net/tcp"),format!("sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n0: {ip:08X}:88B9 00000000:0000 0A 0:0 00:0 0 0 0 111 1\n")).unwrap();
        Self {
            root,
            artifact,
            native: NativeObserver::with_proc_root(proc),
        }
    }
    fn owner() -> OwnedStatus {
        OwnedStatus {
            service: ServiceId::SingBox,
            pid: PID,
            generation: 3,
            running: true,
        }
    }
    fn bind(&mut self) -> OwnedIdentity {
        OwnedIdentity::bind(
            Self::owner(),
            self.artifact.parent().unwrap().to_path_buf(),
            self.artifact.clone(),
            FileIdentity::from_metadata(&fs::metadata(&self.artifact).unwrap()),
            FileIdentity::from_metadata(&fs::metadata(self.artifact.parent().unwrap()).unwrap()),
            self,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
impl Observer for Fixture {
    fn read_file(
        &mut self,
        path: &Path,
        limit: usize,
        b: &Budget<'_>,
    ) -> Result<Vec<u8>, TunError> {
        self.native.read_file(path, limit, b)
    }
    fn read_link(&mut self, path: &Path, b: &Budget<'_>) -> Result<PathBuf, TunError> {
        self.native.read_link(path, b)
    }
    fn metadata(
        &mut self,
        path: &Path,
        follow: bool,
        b: &Budget<'_>,
    ) -> Result<FileIdentity, TunError> {
        self.native.metadata(path, follow, b)
    }
    fn list_dir(
        &mut self,
        path: &Path,
        limit: usize,
        b: &Budget<'_>,
    ) -> Result<Vec<String>, TunError> {
        self.native.list_dir(path, limit, b)
    }
    fn interfaces(&mut self, b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
        b.check()?;
        Ok(vec![Interface {
            name: "b6p-tun".into(),
            up: true,
            mtu: 1500,
            addresses: vec![InterfaceAddress {
                address: "172.31.255.253".parse().unwrap(),
                bits: 30,
            }],
        }])
    }
    fn ipv4_routes(&mut self, b: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
        b.check()?;
        Ok(vec![])
    }
}
fn config(port: u16) -> Vec<u8> {
    format!(r#"{{"inbounds":[{{"type":"direct","tag":"dns-in","listen":"127.0.0.1","listen_port":{port},"network":"udp"}},{{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"mtu":1500,"stack":"system","dns_mode":"disabled","auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024}}],"route":{{"rules":[{{"inbound":"dns-in","action":"hijack-dns"}},{{"ip_version":6,"outbound":"direct"}}]}},"dns":{{"rules":[{{"server":"dns-direct","domain":"bootstrap.test"}}]}}}}"#).into_bytes()
}
fn dns_hook(raw: &[u8], budget: &Budget<'_>) -> Result<(), TunError> {
    readiness_dns::wait_readiness(raw, budget.deadline, Some(budget.cancel)).map_err(|error| {
        match error {
            ReadinessError::Deadline => TunError::Deadline,
            ReadinessError::Canceled => TunError::Cancelled,
            _ => TunError::Observation,
        }
    })
}
#[test]
fn owned_tun_startup_requires_actual_dns_not_just_owned_interface() {
    let cancel = AtomicBool::new(false);
    let mut fixture = Fixture::new();
    let identity = fixture.bind();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut query = [0; 4096];
        let (n, peer) = socket.recv_from(&mut query).unwrap();
        let mut answer = query[..n].to_vec();
        answer[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
        answer[6..8].copy_from_slice(&1u16.to_be_bytes());
        answer.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1]);
        socket.send_to(&answer, peer).unwrap();
    });
    let result = readiness_tun::check_startup_once(
        &config(port),
        &identity,
        &mut fixture,
        Instant::now() + Duration::from_secs(1),
        &cancel,
        &mut || Ok(Fixture::owner()),
        Some(&mut dns_hook),
    );
    server.join().unwrap();
    assert_eq!(result, Ok(OwnedObservation::TunOwned));
    let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
    silent
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let port = silent.local_addr().unwrap().port();
    let (stop, stopped) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut query = [0; 4096];
        let _ = silent.recv_from(&mut query);
        let _ = stopped.recv_timeout(Duration::from_secs(1));
    });
    let result = readiness_tun::check_startup_once(
        &config(port),
        &identity,
        &mut fixture,
        Instant::now() + Duration::from_millis(150),
        &cancel,
        &mut || Ok(Fixture::owner()),
        Some(&mut dns_hook),
    );
    let _ = stop.send(());
    server.join().unwrap();
    assert_eq!(result, Err(TunError::Deadline));
}
