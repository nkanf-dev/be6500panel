# Shared native product IO contract

Root owns `rust/panel/src/product_io.rs` and library/server exports. Source workers may refer to these exact types without editing root files.

```rust
use std::path::Path;
use crate::readiness_tun::Budget;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Program { Uci, Ubus, Ip, Iptables, Ip6tables, Service, Curl }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error { Unavailable, Invalid, Limit, Deadline, Cancelled, Failed }
pub struct Output { pub code:i32, pub stdout:Vec<u8>, pub stderr:Vec<u8> }
pub trait Backend {
 fn read(&mut self,path:&Path,limit:usize,budget:&Budget<'_>)->Result<Vec<u8>,Error>;
 fn run(&mut self,program:Program,args:&[String],stdin:Option<&[u8]>,limit:usize,budget:&Budget<'_>)->Result<Output,Error>;
 fn now_unix(&self)->u64;
 fn metadata(&mut self,path:&Path,follow:bool,budget:&Budget<'_>)->Result<Metadata,Error> { Err(Error::Unavailable) }
 fn list(&mut self,path:&Path,limit:usize,budget:&Budget<'_>)->Result<Vec<String>,Error> { Err(Error::Unavailable) }
 fn read_link(&mut self,path:&Path,budget:&Budget<'_>)->Result<std::path::PathBuf,Error> { Err(Error::Unavailable) }
}
// Shared exact UTC timestamp helper, no new chrono dependency:
// pub fn timestamp(unix_seconds:u64)->String;
```

Internal callers construct fixed service/UCI/native argv, not client argv endpoints. Native Backend is root-owned, bounded nonblocking pipes/deadline/reap, single-thread. Backend::read with native `/` root; tests fake paths. Service arguments are fixed init script name then admitted action (no shellstring). Curl is reserved only for fixed qualified legacy native providers; ordinary new HTTPS uses existingSourcePolicy where needed. No client CA/PID/path/argv execution. Worker responses return `serde_json::Value` as public DTO only; raw private config bytes stay separate and never Debug/log.

Module error envelope type owns fixed code/message and HTTPstatus (fields `pub status:u16,pub code:&'static str,pub message:&'static str`), implementDisplay fixed. Root maps errors to authenticatedJSON once. Module APIs listed in assignments use this Backend/Budget.

`pub struct Metadata { pub regular:bool, pub directory:bool, pub symlink:bool, pub mode:u32, pub uid:u32, pub size:u64, pub dev:u64, pub ino:u64 }` native fullfileeligibility seam; no HTTPpathinput.
