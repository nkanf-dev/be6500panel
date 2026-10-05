use be6500_panel::{
    http::Method,
    product_configuration::Configuration,
    product_gateway::{Product, Response},
    product_io::{Backend, Error, Output, Program},
    readiness_tun::Budget,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fake {
    native: PathBuf,
    now: u64,
    calls: Vec<(Program, Vec<String>)>,
}
impl Backend for Fake {
    fn read(&mut self, path: &Path, limit: usize, b: &Budget<'_>) -> Result<Vec<u8>, Error> {
        b.check().map_err(|_| Error::Deadline)?;
        let fixed=match path.to_str().unwrap_or(""){"/proc/sys/kernel/hostname"=>Some(b"native-fixture\n".to_vec()),"/proc/sys/kernel/ostype"=>Some(b"Linux\n".to_vec()),"/proc/stat"=>Some(b"cpu 10 0 0 10 0 0 0 0 0 0\ncpu0 5 0 0 5 0 0 0 0 0 0\ncpu1 5 0 0 5 0 0 0 0 0 0\n".to_vec()),"/proc/sys/kernel/osrelease"=>Some(b"synthetic-linux\n".to_vec()),"/proc/uptime"=>Some(b"120.25 400\n".to_vec()),"/proc/meminfo"=>Some(b"MemTotal: 393300 kB\nMemAvailable: 48000 kB\n".to_vec()),"/proc/loadavg"=>Some(b"0.1 0.2 0.3 1/20 123\n".to_vec()),"/proc/cpuinfo"=>Some(b"processor : 0\nprocessor : 1\n".to_vec()),"/etc/os-release"=>Some(b"PRETTY_NAME=\"Synthetic OpenWrt\"\n".to_vec()),"/etc/openwrt_release"=>Some(b"DISTRIB_DESCRIPTION='Synthetic OpenWrt'\nDISTRIB_RELEASE='1.0.64'\n".to_vec()),"/proc/version"=>Some(b"Linux fixture\n".to_vec()),"/tmp/sysinfo/model"=>Some(b"Xiaomi BE6500\n".to_vec()),"/tmp/sysinfo/board_name"=>Some(b"xiaomi,rn02\n".to_vec()),"/proc/net/route"=>Some(b"Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0 00000000 010200C0 0003 0 0 0 00000000 0 0 0\n".to_vec()),"/proc/net/dev"=>Some(b"Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n eth0: 1000 1 0 0 0 0 0 0 2000 1 0 0 0 0 0 0\n".to_vec()),"/proc/net/arp"=>Some(b"IP address HW type Flags HW address Mask Device\n".to_vec()),"/tmp/dhcp.leases"=>Some(Vec::new()),"/proc/net/ip_tables_names"=>Some(b"filter\nnat\nmangle\n".to_vec()),"/proc/net/ip6_tables_names"=>Some(b"filter\n".to_vec()),"/etc/resolv.conf"=>Some(b"nameserver 127.0.0.1\n".to_vec()),_=>None};
        let raw = if let Some(raw) = fixed {
            raw
        } else if path.starts_with("/etc/config") {
            fs::read(self.native.join(path.file_name().unwrap())).map_err(|_| Error::Unavailable)?
        } else {
            fs::read(path).map_err(|_| Error::Unavailable)?
        };
        if raw.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(raw)
        }
    }
    fn run(
        &mut self,
        p: Program,
        args: &[String],
        _: Option<&[u8]>,
        _: usize,
        b: &Budget<'_>,
    ) -> Result<Output, Error> {
        b.check().map_err(|_| Error::Deadline)?;
        self.calls.push((p, args.to_vec()));
        let stdout = match p {
            Program::Uci => {
                if args
                    .last()
                    .is_some_and(|arg| arg == "xiaoqiang.common.INITTED")
                {
                    b"YES\n".to_vec()
                } else {
                    Vec::new()
                }
            }
            Program::Ubus => {
                if args.get(1).is_some_and(|arg| arg == "system") {
                    serde_json::to_vec(&json!({"model":"Xiaomi BE6500","release":{"version":"1.0.64","description":"Synthetic OpenWrt"},"kernel":"synthetic-linux"})).unwrap()
                } else {
                    b"{}".to_vec()
                }
            }
            Program::Ip => {
                if args.iter().any(|arg| arg == "addr" || arg == "address") {
                    b"1: lo inet 127.0.0.1/8 scope host lo\n2: br-lan inet 192.168.31.1/24 scope global br-lan\n".to_vec()
                } else {
                    Vec::new()
                }
            }
            Program::Iptables | Program::Ip6tables => {
                b"-P INPUT ACCEPT\n-P OUTPUT ACCEPT\n-P FORWARD ACCEPT\n".to_vec()
            }
            Program::Service => Vec::new(),
            _ => return Err(Error::Unavailable),
        };
        Ok(Output {
            code: 0,
            stdout,
            stderr: Vec::new(),
        })
    }
    fn now_unix(&self) -> u64 {
        self.now
    }
    fn list(&mut self, path: &Path, _: usize, _: &Budget<'_>) -> Result<Vec<String>, Error> {
        if path == Path::new("/etc/init.d") {
            Ok(vec![])
        } else {
            Err(Error::Unavailable)
        }
    }
}
fn budget(flag: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(5),
        cancel: flag,
    }
}
struct Fixture {
    root: PathBuf,
    data: PathBuf,
    native: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "native-product-gateway-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let data = root.join("data");
        let native = root.join("native");
        for p in [&data, &native] {
            fs::DirBuilder::new().mode(0o700).create(p).unwrap();
        }
        for (name, raw) in [
            (
                "network",
                "config interface 'lan'\n option device 'br-lan'\n option proto 'static'\n option ipaddr '192.168.31.1'\n option netmask '255.255.255.0'\n",
            ),
            (
                "wireless",
                "config wifi-device 'radio0'\n option channel 'auto'\n",
            ),
            ("dhcp", "config dnsmasq\n option port '0'\n"),
            (
                "firewall",
                "config defaults\n option input 'ACCEPT'\n option forward 'REJECT'\n option output 'ACCEPT'\n",
            ),
            ("system", "config system\n option hostname 'fixture'\n"),
            (
                "dropbear",
                "config dropbear 'factory'\n option Port '22'\nconfig dropbear 'rescue'\n option Port '2222'\n",
            ),
        ] {
            fs::write(native.join(name), raw).unwrap();
            fs::set_permissions(native.join(name), fs::Permissions::from_mode(0o600)).unwrap();
        }
        Self { root, data, native }
    }
    fn product(&self) -> Product<Fake> {
        let io = Fake {
            native: self.native.clone(),
            now: 1800000000,
            calls: Vec::new(),
        };
        let mut product = Product::with_backend(&self.data, io);
        product.replace_configuration(
            Configuration::open_with_native_dir(&self.data, &self.native).unwrap(),
        );
        product
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn value(response: Response) -> Value {
    match response {
        Response::Json { status, value } => {
            assert_eq!(status, 200);
            value
        }
        Response::Backup(_) => panic!("expected JSON"),
    }
}
#[test]
fn complete_gateway_native_configuration_draft_backup_preview_staging_keep_wire() {
    let f = Fixture::new();
    let mut product = f.product();
    let flag = AtomicBool::new(false);
    let b = budget(&flag);
    let documents = value(
        product
            .handle(
                "/api/configuration",
                Method::Get,
                "",
                &[],
                None,
                None,
                &[],
                &b,
            )
            .unwrap(),
    );
    assert_eq!(documents["documents"].as_array().unwrap().len(), 6);
    let generation = documents["generation"].as_u64().unwrap();
    let raw = json!({"module":"dhcp","content":"config dnsmasq\n option port '0'\n option cachesize '400'\n","generation":generation});
    let staged = value(
        product
            .handle(
                "/api/configuration/stage",
                Method::Post,
                "",
                raw.to_string().as_bytes(),
                None,
                None,
                &[],
                &b,
            )
            .unwrap(),
    );
    assert_eq!(staged["valid"], true);
    assert!(!staged["id"].as_str().unwrap().is_empty());
    let before = fs::read(f.native.join("dhcp")).unwrap();
    let backup = product
        .handle(
            "/api/maintenance/backup",
            Method::Post,
            "",
            br#"{"scopes":["network","wireless","dhcp","firewall","system","dropbear"]}"#,
            None,
            None,
            &[],
            &b,
        )
        .unwrap();
    let bytes = match backup {
        Response::Backup(bytes) => bytes,
        _ => panic!("backupattachment"),
    };
    let decoded: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded["documents"].as_array().unwrap().len(), 6);
    assert_eq!(fs::read(f.native.join("dhcp")).unwrap(), before);
    let preview = value(
        product
            .handle(
                "/api/maintenance/import/preview",
                Method::Post,
                "",
                &bytes,
                None,
                None,
                &[],
                &b,
            )
            .unwrap(),
    );
    assert!(!preview["id"].as_str().unwrap().is_empty());
}
#[test]
fn complete_gateway_read_plans_log_methods_and_invalid_inputs_do_not_mutate_native() {
    let f = Fixture::new();
    let mut product = f.product();
    let flag = AtomicBool::new(false);
    let b = budget(&flag);
    let before = fs::read(f.native.join("network")).unwrap();
    let system = value(
        product
            .handle("/api/system", Method::Get, "", &[], None, None, &[], &b)
            .unwrap(),
    );
    assert_eq!(system["mode"], "host");
    let plan=value(product.handle("/api/proxy/plan",Method::Post,"",br#"{"mode":"direct","dnsStrategy":"direct","ipv6Policy":"direct","failurePolicy":"direct","nodeCount":0}"#,None,None,&[],&b).unwrap());
    assert_eq!(plan["canApply"], false);
    assert!(
        product
            .handle(
                "/api/configuration/stage",
                Method::Post,
                "",
                b"[]",
                None,
                None,
                &[],
                &b
            )
            .is_err()
    );
    let logs = value(
        product
            .handle(
                "/api/logs",
                Method::Get,
                "limit=10",
                &[],
                None,
                None,
                &[],
                &b,
            )
            .unwrap(),
    );
    assert_eq!(logs["capacity"], 500);
    assert_eq!(fs::read(f.native.join("network")).unwrap(), before);
    assert!(product.close());
}

#[test]
fn product_owner_loads_legacy_configuration_child_not_application_root() {
    let f = Fixture::new();
    let configuration_dir = f.data.join("configuration");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&configuration_dir)
        .unwrap();
    fs::write(
        configuration_dir.join("state.json"),
        br#"{"generation":8,"fingerprint":"","drafts":[]}"#,
    )
    .unwrap();
    fs::set_permissions(
        configuration_dir.join("state.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let mut product = Product::with_backend_native_dir(
        &f.data,
        Fake {
            native: f.native.clone(),
            now: 1800000000,
            calls: Vec::new(),
        },
        &f.native,
    );
    let flag = AtomicBool::new(false);
    let b = budget(&flag);
    let status = value(
        product
            .handle(
                "/api/configuration/status",
                Method::Get,
                "",
                &[],
                None,
                None,
                &[],
                &b,
            )
            .unwrap(),
    );
    assert_eq!(status["generation"], 8);
    assert!(!f.data.join("state.json").exists());
    assert!(product.close());
}
