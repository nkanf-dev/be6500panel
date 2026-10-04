use be6500_panel::auth::Auth;
use be6500_panel::server::Service;
use be6500_panel::static_files::StaticFiles;
use std::ffi::OsString;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "be6500-panel: loopback-only diagnostic and rule-draft slice\n\
Usage: be6500-panel [--listen 127.0.0.1:8790] [--proc-root /proc] [--web-dir PATH] [--data-dir PATH]\n\
GET/HEAD health and memory, session login/logout, and optional rule drafts.\n\
--data-dir loads private subscription/draft only; save and preview never Apply.\n\
Runtime Apply/select are unavailable. Set BE6500PANEL_PASSWORD at startup.\n\
Empty password disables session authentication on this loopback-only listener.\n";

struct Options {
    listen: SocketAddr,
    proc_root: PathBuf,
    web_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
}

enum Invocation {
    Help,
    Run(Options),
}

fn parse_options() -> Result<Invocation, &'static str> {
    let mut args = std::env::args_os().skip(1);
    let mut options = Options {
        listen: "127.0.0.1:8790".parse().expect("constant address"),
        proc_root: PathBuf::from("/proc"),
        web_dir: None,
        data_dir: None,
    };
    let mut seen = 0_u8;
    while let Some(flag) = args.next() {
        if flag == "--help" {
            if seen == 0 && args.next().is_none() {
                return Ok(Invocation::Help);
            }
            return Err("help must be used alone");
        }
        let bit = match flag.to_str() {
            Some("--listen") => 1,
            Some("--proc-root") => 2,
            Some("--web-dir") => 4,
            Some("--data-dir") => 8,
            _ => return Err("unknown argument"),
        };
        if seen & bit != 0 {
            return Err("duplicate argument");
        }
        seen |= bit;
        let value: OsString = args.next().ok_or("missing argument value")?;
        if value.is_empty() || value.to_str().is_some_and(|v| v.starts_with("--")) {
            return Err("invalid argument value");
        }
        match bit {
            1 => {
                options.listen = value
                    .to_str()
                    .ok_or("invalid listen address")?
                    .parse()
                    .map_err(|_| "invalid listen address")?;
                if !options.listen.ip().is_loopback() {
                    return Err("listen address must be loopback-only");
                }
            }
            2 => options.proc_root = PathBuf::from(value),
            4 => options.web_dir = Some(PathBuf::from(value)),
            8 => options.data_dir = Some(PathBuf::from(value)),
            _ => unreachable!(),
        }
    }
    Ok(Invocation::Run(options))
}

fn run(options: Options) -> Result<(), &'static str> {
    // Read once at startup. No flags, files, or request logs carry the password.
    let password = match std::env::var("BE6500PANEL_PASSWORD") {
        Ok(password) => password,
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(_) => return Err("invalid password environment"),
    };
    let auth = Auth::new(&password);
    drop(password);
    let mut service = Service::new(options.proc_root).with_auth(auth);
    if let Some(data_dir) = options.data_dir {
        service = service.with_data_dir(data_dir);
    }
    if let Some(root) = options.web_dir {
        let files = StaticFiles::new(&root).map_err(|_| "static root unavailable")?;
        service = service.with_static_files(files);
    }
    let listener =
        TcpListener::bind(options.listen).map_err(|_| "loopback listener unavailable")?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    be6500_panel::server_loop::serve(&listener, &service, None, &cancel)
        .map_err(|_| "listener failed")
}

fn main() -> ExitCode {
    let result = match parse_options() {
        Ok(Invocation::Help) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Run(options)) => run(options),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("be6500-panel: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
