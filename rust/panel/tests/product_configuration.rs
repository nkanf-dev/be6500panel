//! Synthetic filesystem/Backend tests. Never connect to a router or execute Go.
use be6500_panel::http::Method;
use be6500_panel::product_configuration::{Configuration, MAX_DOCUMENT_BYTES, MAX_DRAFTS, preview_documents};
use be6500_panel::product_io::{Backend, Error, Output, Program};
use be6500_panel::readiness_tun::Budget;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const MODULES:[&str;6]=["network","wireless","dhcp","firewall","system","dropbear"];
const NETWORK:&str="# preserve factory comments\nconfig interface 'lan'\n option device 'br-lan'\n option proto 'static'\n option ipaddr '192.168.31.1'\n option netmask '255.255.255.0'\nconfig device 'bridge'\n option name 'br-lan'\n option type 'bridge'\n list ports 'eth1.4'\nconfig interface 'wan'\n option proto 'dhcp'\n";
const WIRELESS:&str="config wifi-device 'radio0'\n option channel 'auto'\nconfig wifi-iface 'primary'\n option device 'radio0'\n option network 'lan'\n option ifname 'wlan0'\n option ssid 'native'\n option encryption 'psk2'\n option key 'test-only-fixture'\nconfig wifi-iface 'guest'\n option device 'radio0'\n option network 'lan'\n option ifname 'wlan0-guest'\n option macfilter 'deny'\n list maclist '02:00:00:00:00:01'\n";
const DHCP:&str="config dnsmasq\n option port '0'\n list server '/example.test/192.0.2.53#5353'\nconfig dhcp 'lan'\n option interface 'lan'\n option start '100'\n option limit '150'\n";
const FIREWALL:&str="config defaults\n option input 'ACCEPT'\n option output 'ACCEPT'\n option forward 'REJECT'\nconfig zone 'lan'\n option name 'lan'\n list network 'lan'\n option input 'ACCEPT'\nconfig include 'factory_acl'\n option path '/etc/firewall.user'\n option reload '1'\n";
const SYSTEM:&str="config system\n option hostname 'fixture-router'\n option timezone 'CST-8'\nconfig timeserver 'ntp'\n option enabled '1'\n list server 'time.example.test'\n";
const DROPBEAR:&str="config dropbear 'factory'\n option Port '22'\n option RootLogin '1'\n option PasswordAuth 'on'\nconfig dropbear 'rescue'\n option Port '2222'\n option RootLogin '1'\n option PasswordAuth 'off'\n";

struct Fake {
    now:u64, calls:Vec<(Program,Vec<String>)>, reloads:usize,
    reject_uci:bool, fail_reload:Option<usize>, fail_service:Option<String>,
    alter_on_reload:Option<(PathBuf,String)>, init:bool, edit_on_show:Option<(PathBuf,String)>,
}
impl Fake {fn new()->Self{Self{now:1_800_000_000,calls:Vec::new(),reloads:0,reject_uci:false,fail_reload:None,fail_service:None,alter_on_reload:None,init:true,edit_on_show:None}}}
impl Backend for Fake {
    fn read(&mut self,path:&Path,limit:usize,budget:&Budget<'_>)->Result<Vec<u8>,Error>{
        budget.check().map_err(|_|Error::Deadline)?;
        let bytes=fs::read(path).map_err(|_|Error::Failed)?;
        if bytes.len()>limit{return Err(Error::Limit);}Ok(bytes)
    }
    fn run(&mut self,program:Program,args:&[String],stdin:Option<&[u8]>,limit:usize,budget:&Budget<'_>)->Result<Output,Error>{
        budget.check().map_err(|_|Error::Deadline)?;assert!(stdin.is_none());assert!(limit<=256<<10);
        self.calls.push((program,args.to_vec()));
        if program==Program::Uci&&args.last().is_some_and(|s|s=="xiaoqiang.common.INITTED"){
            return Ok(Output{code:0,stdout:if self.init{b"YES\n".to_vec()}else{b"NO\n".to_vec()},stderr:Vec::new()});
        }
        if program==Program::Uci {
            assert_eq!(args[0],"-s");assert_eq!(args[1],"-c");assert_eq!(args[3],"-P");assert_eq!(args[2],args[4]);assert_eq!(args[5],"show");
            assert!(Path::new(&args[2]).join(&args[6]).is_file());
            assert_eq!(fs::metadata(&args[2]).unwrap().mode()&0o777,0o700);
            if let Some((path,text))=self.edit_on_show.take(){fs::write(path,text).unwrap();}
            return Ok(Output{code:if self.reject_uci{1}else{0},stdout:Vec::new(),stderr:b"private synthetic output must never escape".to_vec()});
        }
        assert_eq!(program,Program::Service);assert_eq!(args.len(),2);assert_eq!(args[1],"reload");
        self.reloads+=1;
        if let Some((path,text))=self.alter_on_reload.take(){fs::write(path,text).unwrap();}
        let fail=self.fail_reload==Some(self.reloads)||self.fail_service.as_ref()==args.first();
        Ok(Output{code:if fail{1}else{0},stdout:Vec::new(),stderr:Vec::new()})
    }
    fn now_unix(&self)->u64{self.now}
}
struct Fixture{root:PathBuf,data:PathBuf,native:PathBuf,io:Fake}
impl Fixture{
    fn new()->Self{
        let mut random=[0u8;16];getrandom::fill(&mut random).unwrap();
        let root=std::env::temp_dir().join(format!("be6500-configuration-{}",random.iter().map(|b|format!("{b:02x}")).collect::<String>()));
        let native=root.join("etc/config");let data=root.join("configuration");fs::create_dir_all(&native).unwrap();fs::create_dir(&data).unwrap();
        for(module,text)in MODULES.into_iter().zip([NETWORK,WIRELESS,DHCP,FIREWALL,SYSTEM,DROPBEAR]){
            fs::write(native.join(module),text).unwrap();fs::set_permissions(native.join(module),fs::Permissions::from_mode(0o644)).unwrap();
        }
        Self{root,data,native,io:Fake::new()}
    }
    fn open(&self)->Configuration{Configuration::open_with_native_dir(&self.data,&self.native).unwrap()}
    fn text(&self,module:&str)->String{fs::read_to_string(self.native.join(module)).unwrap()}
}
impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}
fn budget<'a>(cancel:&'a AtomicBool)->Budget<'a>{Budget{deadline:Instant::now()+Duration::from_secs(120),cancel}}
fn call(c:&mut Configuration,f:&mut Fixture,path:&str,method:Method,body:Value)->Result<Value,be6500_panel::product_configuration::ApiError>{
    let cancel=AtomicBool::new(false);c.handle(path,method,"",&serde_json::to_vec(&body).unwrap(),&mut f.io,&budget(&cancel))
}
fn read(c:&mut Configuration,f:&mut Fixture)->Value{call(c,f,"/api/configuration",Method::Get,Value::Null).unwrap()}
fn stage(c:&mut Configuration,f:&mut Fixture,module:&str,text:&str)->Value{
    let generation=read(c,f)["generation"].as_u64().unwrap();
    call(c,f,"/api/configuration/stage",Method::Post,json!({"module":module,"content":text,"generation":generation})).unwrap()
}
fn commit(c:&mut Configuration,f:&mut Fixture,drafts:&[Value],ack:bool)->Result<Value,be6500_panel::product_configuration::ApiError>{
    call(c,f,"/api/configuration/commit",Method::Post,json!({"draftIds":drafts.iter().map(|d|d["id"].clone()).collect::<Vec<_>>(),"generation":drafts[0]["generation"],"acknowledgeRisks":ack}))
}
fn status(c:&mut Configuration,f:&mut Fixture)->Value{call(c,f,"/api/configuration/status",Method::Get,Value::Null).unwrap()}

#[test]
fn all_six_documents_preserve_raw_vendor_comments_quotes_lists_and_anonymous_sections(){
    let mut f=Fixture::new();let mut c=f.open();
    let before=read(&mut c,&mut f);assert_eq!(before["documents"].as_array().unwrap().len(),6);
    for module in MODULES{
        let old=f.text(module);
        let text=format!("{old}config vendor 'native_extra'\n option odd 'a'\"b\"'c' # quote concatenation\n list values 'first'\n list values 'second'\n option port 'automatic'\n# final comment without newline");
        let d=stage(&mut c,&mut f,module,&text);
        assert_eq!(d["valid"],true,"{module}: {d}");assert!(d["diff"].as_str().unwrap().contains("# final comment without newline"));
        assert_eq!(f.text(module),old);assert_eq!(f.io.reloads,0);
        let op=commit(&mut c,&mut f,&[d],true).unwrap();assert_eq!(f.text(module),text);
        if op["state"]=="pending_confirmation"{call(&mut c,&mut f,"/api/configuration/confirm",Method::Post,json!({"id":op["id"]})).unwrap();}
        f.io.reloads=0;
    }
    assert!(f.text("dropbear").contains("2222"));assert!(f.text("firewall").contains("factory_acl"));
}
#[test]
fn invalid_known_fields_stay_private_drafts_and_never_reload(){
    let mut f=Fixture::new();let mut c=f.open();
    for(module,tail)in[("network","config interface 'bad'\n option mtu '12'\n"),("wireless","config wifi-iface 'bad'\n option maxassoc '1.5'\n"),("dhcp","config dhcp 'bad'\n option start 'not-a-number'\n"),("firewall","config rule 'bad'\n option dest_port '65536'\n"),("system","config led 'bad'\n option default 'maybe'\n"),("dropbear","config dropbear 'bad'\n option Port 'zero'\n")]{
        let old=f.text(module);let d=stage(&mut c,&mut f,module,&format!("{old}{tail}"));assert_eq!(d["valid"],false);assert_eq!(d["errors"][0]["code"],"invalid_field");
        assert_eq!(commit(&mut c,&mut f,&[d],true).unwrap_err().code,"validation_failed");assert_eq!(f.text(module),old);
    }
    let d=stage(&mut c,&mut f,"dhcp","config dnsmasq\n option local 'unterminated\n");assert_eq!(d["valid"],false);assert_eq!(d["errors"][0]["code"],"uci_syntax");assert_eq!(f.io.reloads,0);
}
#[test]
fn full_schema_preview_matches_exact_native_contexts(){
    let cases=[("network","interface","dns","192.0.2.53 2001:db8::53",true),("network","interface","netmask","255.0.255.0",false),
        ("network","interface","metric","18446744073709551615",true),("network","interface","metric","18446744073709551616",false),
        ("network","device","ports","lan1:u* lan2:t",true),("wireless","wifi-device","channel","234",false),
        ("dhcp","host","ip","ignore",true),("dhcp","host","dns","192.0.2.1",false),("dhcp","dnsmasq","port","0",true),
        ("dhcp","host","mac","02:*:00:00:00:01",true),("firewall","rule","src_port","!80 443 1000-2000",true),
        ("firewall","rule","limit","10/second",true),("system","system","hostname","router.example.test",true),
        ("dropbear","dropbear","passwordauth","vendor-policy",true),("dropbear","vendor","Port","automatic",true)];
    for(module,kind,key,value,valid)in cases{
        let text=format!("config {kind} 'fixture'\n option {key} '{value}'\n");
        let result=preview_documents(&json!([]),&json!([{"module":module,"content":text}])).unwrap();
        assert_eq!(result[0]["valid"],valid,"{module}/{kind}/{key}");
    }
}
#[test]
fn native_uci_rejection_and_factory_guards_do_not_mutate(){
    let mut f=Fixture::new();let mut c=f.open();f.io.reject_uci=true;
    let d=stage(&mut c,&mut f,"dhcp",&(DHCP.to_owned()+"# update\n"));assert_eq!(d["valid"],false);assert_eq!(d["errors"][0]["code"],"uci_validation_failed");assert!(!d.to_string().contains("private synthetic output"));
    f.io.reject_uci=false;
    for(module,text,code)in[("network",NETWORK.replace("192.168.31.1","192.168.32.1"),"management_migration_unavailable"),
        ("firewall",FIREWALL.replace("/etc/firewall.user","/tmp/evil"),"execution_hook_not_allowed"),
        ("dropbear",DROPBEAR.replace("2222","2200"),"rescue_access_required")]{
        let old=f.text(module);let d=stage(&mut c,&mut f,module,&text);assert_eq!(d["valid"],false);assert_eq!(d["errors"][0]["code"],code);assert_eq!(f.text(module),old);
    }
    assert_eq!(f.io.reloads,0);
}
#[test]
fn selected_dependency_bundle_validates_before_first_live_write(){
    let mut f=Fixture::new();let mut c=f.open();
    let net=stage(&mut c,&mut f,"network",&(NETWORK.to_owned()+"config interface 'guest'\n option proto 'static'\n option ipaddr '192.0.2.1/24'\n"));
    let dhcp=stage(&mut c,&mut f,"dhcp",&(DHCP.to_owned()+"config dhcp 'guest'\n option interface 'guest'\n option start '100'\n"));
    assert_eq!(dhcp["valid"],true);assert_eq!(dhcp["dependencies"][0]["code"],"invalid_reference");
    assert_eq!(commit(&mut c,&mut f,&[dhcp.clone()],false).unwrap_err().code,"invalid_reference");assert_eq!(f.text("dhcp"),DHCP);assert!(!f.data.join("journal.json").exists());
    let op=commit(&mut c,&mut f,&[net,dhcp],true).unwrap();assert_eq!(op["state"],"pending_confirmation");assert_eq!(op["changedModules"],json!(["network","dhcp"]));
    call(&mut c,&mut f,"/api/configuration/rollback",Method::Post,json!({"id":op["id"]})).unwrap();assert_eq!(f.text("network"),NETWORK);assert_eq!(f.text("dhcp"),DHCP);
}
#[test]
fn partial_reload_failure_restores_all_bytes_modes_and_retains_drafts(){
    let mut f=Fixture::new();let mut c=f.open();
    let d1=stage(&mut c,&mut f,"dhcp",&(DHCP.to_owned()+"# changed\n"));let d2=stage(&mut c,&mut f,"system",&(SYSTEM.to_owned()+"# changed\n"));
    f.io.fail_reload=Some(2);
    assert_eq!(commit(&mut c,&mut f,&[d1,d2],true).unwrap_err().code,"reload_failed");
    assert_eq!(f.text("dhcp"),DHCP);assert_eq!(f.text("system"),SYSTEM);assert_eq!(fs::metadata(f.native.join("dhcp")).unwrap().mode()&0o777,0o644);
    assert_eq!(status(&mut c,&mut f)["operation"]["phase"],"rolled_back");
    let drafts=call(&mut c,&mut f,"/api/configuration/drafts",Method::Get,Value::Null).unwrap();assert_eq!(drafts["drafts"].as_array().unwrap().len(),2);
    assert_eq!(f.io.reloads,4);
}
#[test]
fn pending_confirmation_confirm_rollback_and_caller_tick_deadline(){
    let mut f=Fixture::new();let mut c=f.open();
    let candidate=NETWORK.replace("option proto 'dhcp'","option proto 'static'\n option ipaddr '192.0.2.2/24'");let d=stage(&mut c,&mut f,"network",&candidate);
    assert_eq!(commit(&mut c,&mut f,&[d.clone()],false).unwrap_err().code,"risk_ack_required");
    let op=commit(&mut c,&mut f,&[d],true).unwrap();let deadline=op["deadline"].as_str().unwrap();assert!(deadline.ends_with('Z'));
    assert_eq!(status(&mut c,&mut f)["operation"]["canConfirm"],true);
    let confirmed=call(&mut c,&mut f,"/api/configuration/confirm",Method::Post,json!({"id":op["id"]})).unwrap();assert_eq!(confirmed["state"],"committed");assert!(confirmed.get("deadline").is_none());
    call(&mut c,&mut f,"/api/configuration/rollback",Method::Post,json!({"id":op["id"]})).unwrap();assert_eq!(f.text("network"),NETWORK);
    let d=stage(&mut c,&mut f,"network",&candidate);let op=commit(&mut c,&mut f,&[d],true).unwrap();
    f.io.now+=119;let cancel=AtomicBool::new(false);c.tick(&mut f.io,&budget(&cancel)).unwrap();assert_eq!(f.text("network"),candidate);
    f.io.now+=1;c.tick(&mut f.io,&budget(&cancel)).unwrap();assert_eq!(f.text("network"),NETWORK);assert_eq!(status(&mut c,&mut f)["operation"]["state"],"rolled_back");
    assert_eq!(call(&mut c,&mut f,"/api/configuration/confirm",Method::Post,json!({"id":op["id"]})).unwrap_err().code,"not_pending");
}
#[test]
fn legacy_reopen_retains_state_journal_and_unknown_backup_files(){
    let mut f=Fixture::new();let id="0123456789abcdef0123456789abcdef";
    fs::write(f.data.join("state.json"),serde_json::to_vec(&json!({"generation":8,"fingerprint":"","drafts":[{"id":id,"module":"dhcp","generation":8,"diff":"fixture diff","risks":[],"valid":true,"errors":[],"createdAt":"2026-10-05T00:00:00.123456789Z","content":DHCP}]})).unwrap()).unwrap();
    fs::write(f.data.join("retained-backup.json"),b"do not clear old backups").unwrap();
    fs::write(f.data.join("journal.json"),serde_json::to_vec(&json!({"operation":{"id":id,"state":"pending_confirmation","generation":9,"deadline":"2027-01-01T00:00:00Z","changedModules":["dhcp"]},"phase":"pending","before":{"dhcp":{"exists":true,"content":DHCP,"mode":420}},"baseGeneration":8})).unwrap()).unwrap();
    fs::write(f.native.join("dhcp"),DHCP.to_owned()+"# interrupted\n").unwrap();
    let mut c=f.open();let cancel=AtomicBool::new(false);c.tick(&mut f.io,&budget(&cancel)).unwrap();
    assert_eq!(f.text("dhcp"),DHCP);assert_eq!(status(&mut c,&mut f)["operation"]["phase"],"rolled_back");
    assert_eq!(fs::read(f.data.join("retained-backup.json")).unwrap(),b"do not clear old backups");
    assert_eq!(call(&mut c,&mut f,"/api/configuration/drafts",Method::Get,Value::Null).unwrap()["drafts"][0]["id"],id);
    drop(c);let mut reopened=f.open();assert_eq!(status(&mut reopened,&mut f)["operation"]["phase"],"rolled_back");
}
#[test]
fn failed_rollback_remains_retryable_and_tick_backoff_is_bounded(){
    let mut f=Fixture::new();let mut c=f.open();
    let d=stage(&mut c,&mut f,"network",&(NETWORK.to_owned()+"# changed\n"));let op=commit(&mut c,&mut f,&[d],true).unwrap();f.io.fail_service=Some("network".into());f.io.now+=120;
    let cancel=AtomicBool::new(false);assert_eq!(c.tick(&mut f.io,&budget(&cancel)).unwrap_err().code,"rollback_failed");
    let s=status(&mut c,&mut f);assert_eq!(s["enabled"],false);assert!(s.get("pendingCommit").is_none());assert_eq!(s["operation"]["phase"],"rolling_back");assert_eq!(s["operation"]["canRollback"],true);
    let reloads=f.io.reloads;c.tick(&mut f.io,&budget(&cancel)).unwrap();assert_eq!(f.io.reloads,reloads);
    f.io.fail_service=None;
    let result=call(&mut c,&mut f,"/api/configuration/rollback",Method::Post,json!({"id":op["id"]})).unwrap();assert_eq!(result["state"],"rolled_back");assert_eq!(f.text("network"),NETWORK);
}
#[test]
fn native_readback_drift_never_accepts_candidate_and_recovery_keeps_previous_bytes(){
    let mut f=Fixture::new();let mut c=f.open();let d=stage(&mut c,&mut f,"dhcp",&(DHCP.to_owned()+"# changed\n"));
    f.io.alter_on_reload=Some((f.native.join("dhcp"),DHCP.to_owned()+"# reload unexpectedly changed\n"));
    assert_eq!(commit(&mut c,&mut f,&[d],false).unwrap_err().code,"verification_failed");assert_eq!(f.text("dhcp"),DHCP);
    assert_eq!(status(&mut c,&mut f)["operation"]["phase"],"rolled_back");
}
#[test]
fn generation_cas_system_guard_and_callback_admission_are_local(){
    let mut f=Fixture::new();let mut c=f.open();let d=stage(&mut c,&mut f,"system",&(SYSTEM.to_owned()+"# changed\n"));f.io.init=false;
    assert_eq!(commit(&mut c,&mut f,&[d.clone()],false).unwrap_err().code,"system_reload_unsafe");assert_eq!(f.text("system"),SYSTEM);f.io.init=true;
    let mut hooks=Vec::new();let cancel=AtomicBool::new(false);
    let body=serde_json::to_vec(&json!({"draftIds":[d["id"]],"generation":d["generation"],"acknowledgeRisks":false})).unwrap();
    let op=c.handle_with_before_mutation("/configuration/commit",Method::Post,"",&body,&mut f.io,&budget(&cancel),&mut |_:&mut Fake,network|{hooks.push(network);Ok(())}).unwrap();assert_eq!(hooks,vec![false]);assert_eq!(op["state"],"committed");
    let d=stage(&mut c,&mut f,"dhcp",&(DHCP.to_owned()+"# planned\n"));
    f.io.edit_on_show=Some((f.native.join("dhcp"),DHCP.to_owned()+"# external edit\n"));
    assert_eq!(commit(&mut c,&mut f,&[d],false).unwrap_err().code,"generation_conflict");assert_eq!(f.text("dhcp"),DHCP.to_owned()+"# external edit\n");
}
#[test]
fn bounds_invalid_requests_and_duplicate_drafts_do_not_disturb_native_state(){
    let mut f=Fixture::new();let mut c=f.open();let cancel=AtomicBool::new(false);
    assert_eq!(c.handle("/configuration/stage",Method::Post,"",b"{",&mut f.io,&budget(&cancel)).unwrap_err().code,"invalid_request");
    let too_big="x".repeat(MAX_DOCUMENT_BYTES+1);let generation=read(&mut c,&mut f)["generation"].clone();
    assert_eq!(call(&mut c,&mut f,"/configuration/stage",Method::Post,json!({"module":"dhcp","content":too_big,"generation":generation})).unwrap_err().code,"document_too_large");
    let mut drafts=Vec::new();for i in 0..MAX_DRAFTS{drafts.push(stage(&mut c,&mut f,"dhcp",&format!("{DHCP}# {i}\n")));}
    assert_eq!(call(&mut c,&mut f,"/configuration/stage",Method::Post,json!({"module":"dhcp","content":DHCP,"generation":generation})).unwrap_err().code,"draft_limit");
    assert_eq!(commit(&mut c,&mut f,&drafts[..2],false).unwrap_err().code,"invalid_commit");assert_eq!(f.text("dhcp"),DHCP);assert_eq!(f.io.reloads,0);
    c.discard_import_draft(drafts[0]["id"].as_str().unwrap()).unwrap();assert_eq!(status(&mut c,&mut f)["enabled"],true);
    assert!(Configuration::open_with_native_dir(&f.data,&f.native).is_err());
}
#[test]
fn reverse_reference_preview_checks_only_new_breaks_and_guest_acl_stays_available(){
    let current=json!([{"module":"network","content":NETWORK.to_owned()+"config interface 'guest'\n option proto 'none'\n"},{"module":"dhcp","content":DHCP.to_owned()+"config dhcp 'guest'\n option interface 'guest'\nconfig dhcp 'vendor'\n option interface 'vendor_missing'\n"}]);
    let preview=preview_documents(&current,&json!([{"module":"network","content":NETWORK}])).unwrap();assert_eq!(preview[0]["valid"],true);assert_eq!(preview[0]["dependencies"][0]["code"],"invalid_reference");
    let mut f=Fixture::new();let mut c=f.open();let next=WIRELESS.replace("02:00:00:00:00:01","02:00:00:00:00:02");
    let d=stage(&mut c,&mut f,"wireless",&next);assert_eq!(d["valid"],true);assert_eq!(d["risks"],json!([]));let op=commit(&mut c,&mut f,&[d],false).unwrap();assert_eq!(op["state"],"committed");assert_eq!(f.text("wireless"),next);
}

#[test]
fn rollback_restores_absence_and_reload_drift_keeps_original_bytes_retryable(){
    let mut f=Fixture::new();fs::remove_file(f.native.join("dhcp")).unwrap();let mut c=f.open();
    let d=stage(&mut c,&mut f,"dhcp",DHCP);let op=commit(&mut c,&mut f,&[d],false).unwrap();assert!(f.native.join("dhcp").is_file());
    call(&mut c,&mut f,"/configuration/rollback",Method::Post,json!({"id":op["id"]})).unwrap();assert!(!f.native.join("dhcp").exists());
    let d=stage(&mut c,&mut f,"network",&(NETWORK.to_owned()+"# changed\n"));let op=commit(&mut c,&mut f,&[d],true).unwrap();
    f.io.alter_on_reload=Some((f.native.join("network"),NETWORK.to_owned()+"# unexpected recovery rewrite\n"));
    assert_eq!(call(&mut c,&mut f,"/configuration/rollback",Method::Post,json!({"id":op["id"]})).unwrap_err().code,"rollback_failed");
    assert_eq!(f.text("network"),NETWORK);assert_eq!(status(&mut c,&mut f)["operation"]["phase"],"rolling_back");
    call(&mut c,&mut f,"/configuration/rollback",Method::Post,json!({"id":op["id"]})).unwrap();assert_eq!(status(&mut c,&mut f)["enabled"],true);
}
#[test]
fn corrupt_legacy_store_is_retained_and_native_subtree_constructor_has_no_mode_side_effect(){
    let f=Fixture::new();let bytes=b"{ private malformed legacy state";fs::write(f.data.join("state.json"),bytes).unwrap();
    assert!(Configuration::open_with_native_dir(&f.data,&f.native).is_err());assert_eq!(fs::read(f.data.join("state.json")).unwrap(),bytes);
    let mode=fs::metadata(&f.native).unwrap().mode()&0o777;
    assert!(Configuration::open_with_native_dir(&f.native,&f.native).is_err());assert_eq!(fs::metadata(&f.native).unwrap().mode()&0o777,mode);
}
