use be6500_panel::{
    policy::{Policy, policy_revision},
    rule_apply::selected_node_id,
    subscription::parse_clash_yaml,
};
use sha2::{Digest, Sha256};
use std::{env, fs};
fn main() {
    let args = env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 4);
    let raw = fs::read(&args[1]).unwrap();
    let config = fs::read(&args[2]).unwrap();
    let draft = fs::read(&args[3]).unwrap();
    let source = parse_clash_yaml(&raw).unwrap();
    let node = selected_node_id(&config, &source.nodes).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&draft).unwrap();
    let policy: Policy =
        serde_json::from_value(value.get("policy").cloned().unwrap_or(value)).unwrap();
    println!(
        "{}",
        serde_json::json!({"nodeId":node,"sourceSHA256":format!("{:x}",Sha256::digest(&raw)),"nativeSHA256":format!("{:x}",Sha256::digest(&config)),"draftRevision":policy_revision(&policy).unwrap()})
    );
}
