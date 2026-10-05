#![cfg(unix)]
use be6500_panel::capture_kernel::{self, KernelError, TableNames};
use be6500_panel::capture_plan::{OwnedRulesPlan, RulesPlanInput, plan_owned_rules};
use be6500_panel::capture_state::{CommandError, CommandResult};
use be6500_panel::native::Ports;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
fn input() -> RulesPlanInput {
    RulesPlanInput {
        scope: "gateway".into(),
        lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
        datapath: "routed-tun".into(),
        tun_interface: "b6p-test".into(),
        tun_address: "172.30.0.1/30".into(),
        lan_interface: "br-lan".into(),
        ports: Ports {
            mixed: 2080,
            tproxy: 7893,
            dns: 1053,
        },
        ipv6: "direct".into(),
        failure: "direct".into(),
        management_ips: vec!["192.168.50.1".into()],
        ..RulesPlanInput::default()
    }
}
fn command(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn reply(success: bool, text: impl AsRef<str>) -> CommandResult {
    CommandResult {
        success,
        output: text.as_ref().as_bytes().to_vec(),
    }
}
fn names() -> TableNames {
    capture_kernel::table_names(b"255 local\n254 main\n253 default\n16500 owned\n").unwrap()
}
fn preflight_replies(plan: &OwnedRulesPlan) -> BTreeMap<Vec<String>, CommandResult> {
    let mut replies = BTreeMap::new();
    replies.insert(command(&["ip","-4","route","show","table","all"]),reply(true,"default via 192.0.2.1 dev wan\n172.30.0.0/30 dev b6p-test proto kernel scope link src 172.30.0.1\nlocal 172.30.0.1 dev b6p-test table local proto kernel scope host src 172.30.0.1\n"));
    replies.insert(
        command(&["ip", "-4", "route", "show", "table", "16500"]),
        reply(false, "Error: ipv4: FIB table does not exist.\n"),
    );
    replies.insert(
        command(&["ip", "-4", "rule", "show"]),
        reply(
            true,
            "0: from all lookup local\n32766: from all lookup main\n",
        ),
    );
    replies.insert(
        command(&["iptables", "-w", "5", "-t", "mangle", "-S"]),
        reply(
            true,
            "-P PREROUTING ACCEPT\n-A PREROUTING -j MARK --set-xmark 0x1/0xf\n",
        ),
    );
    for chain in &plan.ownership.chains {
        replies.insert(
            command(&["iptables", "-w", "5", "-t", &chain.table, "-S", &chain.name]),
            reply(false, "iptables: No chain/target/match by that name.\n"),
        );
    }
    replies
}
fn installed_replies(plan: &OwnedRulesPlan) -> BTreeMap<Vec<String>, CommandResult> {
    let mut replies = BTreeMap::new();
    replies.insert(
        command(&["ip", "-4", "route", "show", "table", "16500"]),
        reply(true, "default dev b6p-test proto static scope link\n"),
    );
    replies.insert(command(&["ip","-4","rule","show"]),reply(true,"0: from all lookup local\n16500: from 192.168.50.0/24 iif br-lan fwmark 0x4000/0x4000 lookup owned\n32766: from all lookup main\n"));
    for chain in &plan.ownership.chains {
        let mut rows = vec![format!("-N {}", chain.name)];
        for argv in &plan.apply {
            if argv.len() > 6
                && argv[0] == "iptables"
                && argv[4] == chain.table
                && argv[5] == "-A"
                && argv[6] == chain.name
            {
                rows.push(argv[5..].join(" "));
            }
        }
        replies.insert(
            command(&["iptables", "-w", "5", "-t", &chain.table, "-S", &chain.name]),
            reply(true, rows.join("\n") + "\n"),
        );
        let mut hooks = vec![format!("-P {} ACCEPT", chain.hook)];
        for argv in plan.apply.iter().rev() {
            if argv.len() > 8
                && argv[0] == "iptables"
                && argv[4] == chain.table
                && argv[5] == "-I"
                && argv[6] == chain.hook
            {
                let mut row = argv[5..].to_vec();
                row[0] = "-A".into();
                row.remove(2);
                hooks.push(row.join(" "));
            }
        }
        replies.insert(
            command(&["iptables", "-w", "5", "-t", &chain.table, "-S", &chain.hook]),
            reply(true, hooks.join("\n") + "\n"),
        );
    }
    replies
}
fn runner(
    replies: &BTreeMap<Vec<String>, CommandResult>,
    argv: &[String],
) -> Result<CommandResult, CommandError> {
    replies
        .get(argv)
        .map(|reply| CommandResult {
            success: reply.success,
            output: reply.output.clone(),
        })
        .ok_or(CommandError::Failure)
}
#[test]
fn fixed_reservation_preflight_does_no_mutation() {
    let input = input();
    let plan = plan_owned_rules(&input).unwrap();
    let replies = preflight_replies(&plan);
    let mut calls = Vec::new();
    capture_kernel::preflight(
        &input,
        &names(),
        |argv, _| {
            calls.push(argv.to_vec());
            runner(&replies, argv)
        },
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(calls.len(), 10);
    assert!(
        calls
            .iter()
            .all(|argv| argv.iter().any(|value| value == "show" || value == "-S"))
    );
}
#[test]
fn occupied_mark_table_chain_and_prefix_are_refused() {
    let input = input();
    let plan = plan_owned_rules(&input).unwrap();
    let original = preflight_replies(&plan);
    let variants = [
        (
            command(&["ip", "-4", "rule", "show"]),
            "16500: from all lookup main\n",
        ),
        (
            command(&["ip", "-4", "rule", "show"]),
            "99: from all fwmark 0x1/0xffff lookup main\n",
        ),
        (
            command(&["iptables", "-w", "5", "-t", "mangle", "-S"]),
            "-A PREROUTING -j CONNMARK --restore-mark\n",
        ),
        (
            command(&["ip", "-4", "route", "show", "table", "all"]),
            "172.30.0.0/16 dev other proto kernel scope link\n",
        ),
        (
            command(&["iptables", "-w", "5", "-t", "nat", "-S", "B6P_V4_DNS"]),
            "-N B6P_V4_DNS\n",
        ),
    ];
    for (key, value) in variants {
        let mut replies = original
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    reply(v.success, std::str::from_utf8(&v.output).unwrap()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        replies.insert(key, reply(true, value));
        assert!(
            capture_kernel::preflight(
                &input,
                &names(),
                |argv, _| runner(&replies, argv),
                Instant::now() + Duration::from_secs(1)
            )
            .is_err()
        );
    }
}
#[test]
fn installed_exact_order_namespace_rules_and_hooks_are_required() {
    let input = input();
    let plan = plan_owned_rules(&input).unwrap();
    let original = installed_replies(&plan);
    capture_kernel::observe_installed(
        &input,
        &names(),
        |argv, _| runner(&original, argv),
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    for change in 0..5 {
        let mut replies = original
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    reply(v.success, std::str::from_utf8(&v.output).unwrap()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let key = match change {
            0 | 1 => command(&["iptables", "-w", "5", "-t", "nat", "-S", "B6P_V4_DNS"]),
            2 => command(&["ip", "-4", "route", "show", "table", "16500"]),
            3 => command(&["ip", "-4", "rule", "show"]),
            _ => command(&["iptables", "-w", "5", "-t", "mangle", "-S", "PREROUTING"]),
        };
        let mut text = String::from_utf8(replies[&key].output.clone()).unwrap();
        match change {
            0 => {
                let mut rows = text.lines().map(str::to_owned).collect::<Vec<_>>();
                rows.swap(1, 2);
                text = rows.join("\n") + "\n";
            }
            1 => text.push_str("-A B6P_V4_DNS -j ACCEPT\n"),
            2 => text = "default via 192.0.2.1 dev b6p-test\n".into(),
            3 => text.push_str(
                "16500: from 192.168.50.0/24 iif br-lan fwmark 0x4000/0x4000 lookup owned\n",
            ),
            _ => text.push_str("-A PREROUTING -j B6P_V4_TUN_MARK\n"),
        };
        replies.insert(key, reply(true, text));
        assert_eq!(
            capture_kernel::observe_installed(
                &input,
                &names(),
                |argv, _| runner(&replies, argv),
                Instant::now() + Duration::from_secs(1)
            ),
            Err(KernelError::Missing)
        );
    }
}
#[test]
fn quoting_alias_errors_limits_and_deadline_fail_closed() {
    assert_eq!(
        capture_kernel::split_args("-A X --comment 'plain value'").unwrap(),
        ["-A", "X", "--comment", "plain value"]
    );
    assert!(capture_kernel::split_args("-A 'bad").is_err());
    assert!(capture_kernel::table_names(b"16500 main\n").is_err());
    let input = input();
    let plan = plan_owned_rules(&input).unwrap();
    let mut replies = preflight_replies(&plan);
    replies.insert(
        command(&["ip", "-4", "route", "show", "table", "16500"]),
        reply(false, "Dump terminated"),
    );
    assert!(
        capture_kernel::preflight(
            &input,
            &names(),
            |argv, _| runner(&replies, argv),
            Instant::now() + Duration::from_secs(1)
        )
        .is_ok()
    );
    replies.insert(
        command(&["ip", "-4", "route", "show", "table", "all"]),
        reply(true, "default dev b6p-test table owned\n"),
    );
    assert!(
        capture_kernel::preflight(
            &input,
            &names(),
            |argv, _| runner(&replies, argv),
            Instant::now() + Duration::from_secs(1)
        )
        .is_err()
    );
    let mut called = false;
    assert_eq!(
        capture_kernel::preflight(
            &input,
            &names(),
            |_, _| {
                called = true;
                Ok(reply(true, ""))
            },
            Instant::now()
        ),
        Err(KernelError::Deadline)
    );
    assert!(!called);
}
