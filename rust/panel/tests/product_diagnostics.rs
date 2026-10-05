#![cfg(unix)]
use be6500_panel::{
    product_diagnostics::{Diagnostics,ApiError},
    product_io::{Backend,Error,Output,Program},
    readiness_tun::Budget, http::Method, native::Node,
};
use serde_json::{Value,json};
use std::{path::Path,sync::atomic::AtomicBool,time::{Duration,Instant}};
struct Fake {calls:usize}
impl Backend for Fake {
    fn read(&mut self,_:&Path,_:usize,_:&Budget<'_>)->Result<Vec<u8>,Error> {self.calls+=1;Err(Error::Unavailable)}
    fn run(&mut self,_:Program,_:&[String],_:Option<&[u8]>,_:usize,_:&Budget<'_>)->Result<Output,Error> {panic!("no command may run in readonly diagnostic fixtures")}
    fn now_unix(&self)->u64 {1_700_000_000}
}
fn call(d:&mut Diagnostics,path:&str,method:Method,query:&str,body:&[u8],nodes:&[Node],io:&mut Fake)->Result<Value,ApiError> {
    let cancel=AtomicBool::new(false);
    d.handle(path,method,query,body,None,nodes,io,&Budget{deadline:Instant::now()+Duration::from_secs(1),cancel:&cancel})
}
#[test]
fn legacy_get_contract_is_bounded_and_never_starts_network_or_processes() {
    let mut d=Diagnostics::new();let mut io=Fake{calls:0};
    let probes=call(&mut d,"/api/proxy/node-probes",Method::Get,"",&[],&[],&mut io).unwrap();
    for key in ["revision","available","target","running","results","limits"] {assert!(probes.get(key).is_some(),"missing {key}");}
    assert_eq!(probes["revision"],"");assert_eq!(probes["available"],false);assert_eq!(probes["running"],false);assert!(probes["job"].is_null());assert_eq!(probes["results"],json!([]));
    assert_eq!(probes["limits"],json!({"maxNodes":256,"concurrency":1,"timeoutMs":3000}));
    assert_eq!(probes["target"],"https://www.gstatic.com/generate_204");
    let traces=call(&mut d,"/api/proxy/request-traces",Method::Get,"",&[],&[],&mut io).unwrap();
    assert_eq!(traces["traces"],json!([]));assert_eq!(traces["running"],false);
    assert_eq!(traces["limits"],json!({"timeoutMs":10000,"bodyBytes":65536,"concurrency":1,"capacity":64}));
    assert_eq!(traces["targets"][0]["id"],"google204");assert_eq!(traces["targets"][1]["id"],"cloudflare");
    assert_eq!(io.calls,0);
}
#[test]
fn reads_reject_queries_bodies_and_wrong_methods_without_io() {
    let mut d=Diagnostics::new();let mut io=Fake{calls:0};
    for path in ["/api/proxy/node-probes","/api/proxy/request-traces"] {
        assert_eq!(call(&mut d,path,Method::Get,"url=https://private",&[],&[],&mut io).unwrap_err().status,400);
        assert_eq!(call(&mut d,path,Method::Get,"",b"{}",&[],&mut io).unwrap_err().status,400);
    }
    assert_eq!(call(&mut d,"/api/proxy/request-traces",Method::Delete,"",&[],&[],&mut io).unwrap_err().status,405);
    assert_eq!(call(&mut d,"/api/proxy/private-diagnostic-command",Method::Post,"",b"{}",&[],&mut io).unwrap_err().status,404);
    assert_eq!(io.calls,0);
}
#[test]
fn trace_presets_and_routes_do_not_admit_client_network_parameters() {
    let mut d=Diagnostics::new();let mut io=Fake{calls:0};
    for body in [br#"{"targetId":"arbitrary","route":"direct"}"#.as_slice(),
        br#"{"targetId":"google204","route":"automatic"}"#,
        br#"{"targetId":"google204","route":"direct","url":"https://secret.example/"}"#,
        br#"{"targetId":"google204","route":"proxy","pid":12}"#,
        br#"{"targetId":"google204","route":"proxy","socket":"127.0.0.1:1080"}"#,
        br#"{"targetId":"google204","route":"direct","argv":["curl"]}"#,
        br#"{"targetId":"google204","route":"direct","ca":"/tmp/ca.pem"}"#,
        br#"["google204","direct"]"#] {
        assert_eq!(call(&mut d,"/api/proxy/request-traces",Method::Post,"",body,&[],&mut io).unwrap_err().status,400);
    }
    let valid=br#"{"targetId":"google204","route":"proxy"}"#;
    assert_eq!(call(&mut d,"/api/proxy/request-traces",Method::Post,"",valid,&[],&mut io).unwrap_err().code,"request_trace_unavailable");
    assert_eq!(io.calls,0);
}
#[test]
fn node_source_revision_is_authoritative_and_old_inputs_never_alias_new_source() {
    let mut d=Diagnostics::new();let mut io=Fake{calls:0};let nodes=vec![Node{id:"current-node".into(),..Node::default()}];
    d.bind_nodes("rev-current",&nodes).unwrap();
    let view=call(&mut d,"/api/proxy/node-probes",Method::Get,"",&[],&nodes,&mut io).unwrap();assert_eq!(view["revision"],"rev-current");
    assert_eq!(call(&mut d,"/api/proxy/node-probes",Method::Post,"",br#"{"all":true,"nodeIds":[],"revision":"rev-old"}"#,&nodes,&mut io).unwrap_err().code,"revision_mismatch");
    assert_eq!(call(&mut d,"/api/proxy/node-probes",Method::Post,"",br#"{"nodeIds":["not-current"],"revision":"rev-current"}"#,&nodes,&mut io).unwrap_err().code,"invalid_nodes");
    d.revoke_nodes();let view=call(&mut d,"/api/proxy/node-probes",Method::Get,"",&[],&nodes,&mut io).unwrap();assert_eq!(view["results"],json!([]));assert!(view["job"].is_null());
    assert_eq!(io.calls,0);
}
#[test]
fn private_configuration_and_error_messages_cannot_leak_into_public_status() {
    let mut d=Diagnostics::new();let mut io=Fake{calls:0};
    assert_eq!(format!("{d:?}"),"Diagnostics([private])");
    let body=br#"{"targetId":"google204","route":"proxy","password":"sensitive-token"}"#;
    let e=call(&mut d,"/api/proxy/request-traces",Method::Post,"",body,&[],&mut io).unwrap_err();
    assert!(!format!("{e:?} {e}").contains("sensitive-token"));
    let cancelled=call(&mut d,"/api/proxy/node-probes",Method::Delete,"",&[],&[],&mut io).unwrap();assert_eq!(cancelled["running"],false);
    assert!(d.close(&mut io).unwrap());assert_eq!(io.calls,0);
}
