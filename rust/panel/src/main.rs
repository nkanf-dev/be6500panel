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
Runtime Apply/select are unavailable in diagnostic mode. Set BE6500PANEL_PASSWORD at startup.\n\
Empty password disables session authentication on this loopback-only listener.\n\
Explicit native mode: --native-runtime --data-dir PATH --run-dir PATH --command-manifest PATH\n\
Native mode requires authentication and trusted private command bindings; it is not full module parity.\n";

struct Options {
    listen: SocketAddr,
    proc_root: PathBuf,
    web_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    native: bool,
    run_dir: Option<PathBuf>,
    command_manifest: Option<PathBuf>,
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
        native: false,
        run_dir: None,
        command_manifest: None,
    };
    let mut seen = 0_u16;
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
            Some("--native-runtime") => 16,
            Some("--run-dir") => 32,
            Some("--command-manifest") => 64,
            _ => return Err("unknown argument"),
        };
        if seen & bit != 0 {
            return Err("duplicate argument");
        }
        seen |= bit;
        if bit == 16 {
            options.native = true;
            continue;
        }
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
            }
            2 => options.proc_root = PathBuf::from(value),
            4 => options.web_dir = Some(PathBuf::from(value)),
            8 => options.data_dir = Some(PathBuf::from(value)),
            32 => options.run_dir = Some(PathBuf::from(value)),
            64 => options.command_manifest = Some(PathBuf::from(value)),
            _ => unreachable!(),
        }
    }
    if options.native {
        if options.data_dir.is_none()
            || options.run_dir.is_none()
            || options.command_manifest.is_none()
        {
            return Err("native mode requires data, runtime and command manifest roots");
        }
        if options.proc_root.as_path() != std::path::Path::new("/proc") {
            return Err("native mode requires the real proc adapter");
        }
    } else {
        if options.run_dir.is_some() || options.command_manifest.is_some() {
            return Err("native options require explicit native mode");
        }
        if !options.listen.ip().is_loopback() {
            return Err("listen address must be loopback-only");
        }
    }
    Ok(Invocation::Run(options))
}

fn run(options: Options) -> Result<(), &'static str> {
    let password = match std::env::var("BE6500PANEL_PASSWORD") {
        Ok(password) => password,
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(_) => return Err("invalid password environment"),
    };
    if options.native && password.is_empty() {
        return Err("native mode requires authentication");
    }
    let auth = Auth::new(&password);
    drop(password);
    let mut service = Service::new(options.proc_root).with_auth(auth);
    if let Some(data) = &options.data_dir {
        service = service.with_data_dir(data);
    }
    if let Some(web) = options.web_dir {
        service = service
            .with_static_files(StaticFiles::new(&web).map_err(|_| "static root unavailable")?);
    }
    let bindings = if options.native {
        Some(
            be6500_panel::runtime_bindings::Bindings::load(
                options
                    .command_manifest
                    .as_ref()
                    .expect("validated native option"),
            )
            .map_err(|_| "native command bindings unavailable")?,
        )
    } else {
        None
    };
    // Busy/invalid listener never performs startup cleanup or saved-on restore.
    let listener = TcpListener::bind(options.listen).map_err(|_| "listener unavailable")?;
    let _signals = be6500_panel::shutdown::SignalGuard::install()
        .map_err(|_| "shutdown signal setup unavailable")?;
    if let Some(bindings) = bindings {
        let mut owner = be6500_panel::native_owner::NativeOwner::open_with_artifacts(
            be6500_panel::native_owner::Options {
                data_dir: options.data_dir.expect("validated native option"),
                run_dir: options.run_dir.expect("validated native option"),
            },
            bindings.binaries,
            bindings.names,
            bindings.source,
            bindings.artifacts,
            std::rc::Rc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .map_err(|_| "native owner unavailable")?;
        if owner.initialize().is_err() {
            eprintln!("be6500-panel: native startup recovery pending");
        }
        let served = be6500_panel::server_loop::serve(
            &listener,
            &service,
            Some(owner.runtime_mut()),
            be6500_panel::shutdown::flag(),
        );
        // Three bounded initial attempts. A failure MUST keep the creator
        // process, lock and owned handles alive; returning would kill a Linux
        // PDEATHSIG core while interception could still be installed.
        let mut close_result = owner.close();
        for delay in [100, 200] {
            if close_result.is_ok() {
                break;
            }
            be6500_panel::shutdown::wait_cleanup_retry(delay);
            close_result = owner.close();
        }
        if close_result.is_err() {
            eprintln!("be6500-panel: native shutdown cleanup pending; owner retained");
        }
        while close_result.is_err() {
            let sequence = be6500_panel::shutdown::sequence();
            let retry = be6500_panel::server_loop::serve_cleanup(
                &listener,
                &service,
                owner.runtime_mut(),
                sequence,
            );
            if retry.is_err() {
                // If the listener itself is unavailable, preserve the owner
                // and await a new signal after local repair. No busy retries.
                while be6500_panel::shutdown::sequence() == sequence {
                    be6500_panel::shutdown::wait_cleanup_retry(100);
                }
            }
            close_result = owner.close();
        }
        // A prior cleanup refusal is resolved, not a current failure. A real
        // terminal listener/admission error still reports failure.
        match served {
            Ok(()) | Err(be6500_panel::server_loop::LoopError::Shutdown(_)) => Ok(()),
            Err(_) => Err("listener failed"),
        }
    } else {
        be6500_panel::server_loop::serve(&listener, &service, None, be6500_panel::shutdown::flag())
            .map_err(|_| "listener failed")
    }
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
