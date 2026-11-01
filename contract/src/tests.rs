use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn as_caller(who: &str, ts_ms: u64) {
    let ctx = VMContextBuilder::new()
        .predecessor_account_id(acc(who))
        .block_timestamp(ts_ms * 1_000_000)
        .build();
    testing_env!(ctx);
}

/// Owner "owner.testnet", reporters "alice.testnet" and "bob.testnet".
fn setup() -> Tripwire {
    as_caller("owner.testnet", 1_000);
    let mut c = Tripwire::new(acc("owner.testnet"));
    c.add_reporter(acc("alice.testnet"));
    c.add_reporter(acc("bob.testnet"));
    c
}

fn alice_flags(c: &mut Tripwire, target: &str, severity: u8) {
    as_caller("alice.testnet", 2_000);
    c.flag(
        acc(target),
        "drainer".into(),
        severity,
        "Drained 3 wallets via fake airdrop".into(),
        "https://testnet.nearblocks.io/txns/abc".into(),
    );
}

// ---------- happy path ----------

#[test]
fn unflagged_account_is_clean() {
    let c = setup();
    let r = c.get_risk(acc("friend.testnet"));
    assert_eq!(r.score, 0);
    assert_eq!(r.level, "none");
    assert!(r.flags.is_empty());
}

#[test]
fn reporter_can_flag_and_risk_shows_it() {
    let mut c = setup();
    alice_flags(&mut c, "ref-flnance.testnet", 90);
    let r = c.get_risk(acc("ref-flnance.testnet"));
    assert_eq!(r.score, 90);
    assert_eq!(r.level, "block");
    assert_eq!(r.flags.len(), 1);
    let f = &r.flags[0];
    assert_eq!(f.reporter, acc("alice.testnet"));
    assert_eq!(f.category, "drainer");
    assert_eq!(f.evidence_url, "https://testnet.nearblocks.io/txns/abc");
    assert_eq!(f.timestamp_ms, 2_000);
}

#[test]
fn low_severity_is_warn() {
    let mut c = setup();
    alice_flags(&mut c, "sus.testnet", 40);
    assert_eq!(c.get_risk(acc("sus.testnet")).level, "warn");
}

#[test]
fn second_reporter_raises_score_and_caps_at_100() {
    let mut c = setup();
    alice_flags(&mut c, "sus.testnet", 60);
    as_caller("bob.testnet", 3_000);
    c.flag(acc("sus.testnet"), "phishing".into(), 50, "Phishing link in DMs".into(), "https://example.com/r/1".into());
    let r = c.get_risk(acc("sus.testnet"));
    assert_eq!(r.flags.len(), 2);
    assert_eq!(r.score, 70); // max 60 + 10 for one extra reporter
    assert_eq!(r.level, "block");

    alice_flags(&mut c, "sus.testnet", 100);
    assert_eq!(c.get_risk(acc("sus.testnet")).score, 100); // capped
}

#[test]
fn reflagging_updates_instead_of_duplicating() {
    let mut c = setup();
    alice_flags(&mut c, "sus.testnet", 30);
    alice_flags(&mut c, "sus.testnet", 80);
    let r = c.get_risk(acc("sus.testnet"));
    assert_eq!(r.flags.len(), 1);
    assert_eq!(r.score, 80);
}

#[test]
fn reporter_can_unflag_own_flag() {
    let mut c = setup();
    alice_flags(&mut c, "oops.testnet", 50);
    c.unflag(acc("oops.testnet"));
    assert_eq!(c.get_risk(acc("oops.testnet")).score, 0);
    assert_eq!(c.flagged_count(), 0);
}

#[test]
fn owner_can_moderate_any_flag() {
    let mut c = setup();
    alice_flags(&mut c, "victim.testnet", 90);
    as_caller("owner.testnet", 4_000);
    c.remove_flag(acc("victim.testnet"), acc("alice.testnet"));
    assert_eq!(c.get_risk(acc("victim.testnet")).score, 0);
}

#[test]
fn list_views_paginate() {
    let mut c = setup();
    for i in 0..5 {
        alice_flags(&mut c, &format!("bad{}.testnet", i), 75);
    }
    assert_eq!(c.flagged_count(), 5);
    let page = c.list_flagged(Some(1), Some(2));
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].account_id, acc("bad1.testnet"));
    assert_eq!(page[0].score, 75);
    assert_eq!(page[0].flag_count, 1);
    assert_eq!(c.get_reporters(None, None).len(), 2);
    assert!(c.is_reporter(acc("bob.testnet")));
    assert_eq!(c.get_owner(), acc("owner.testnet"));
    assert_eq!(c.get_flags(acc("bad0.testnet")).len(), 1);
}

#[test]
fn removed_reporter_loses_rights_but_flags_stay_until_moderated() {
    let mut c = setup();
    alice_flags(&mut c, "bad.testnet", 90);
    as_caller("owner.testnet", 5_000);
    c.remove_reporter(acc("alice.testnet"));
    assert!(!c.is_reporter(acc("alice.testnet")));
    assert_eq!(c.get_risk(acc("bad.testnet")).score, 90);
}

#[test]
fn ownership_transfer_moves_admin_rights() {
    let mut c = setup();
    as_caller("owner.testnet", 6_000);
    c.transfer_ownership(acc("dao.testnet"));
    as_caller("dao.testnet", 6_001);
    c.add_reporter(acc("carol.testnet"));
    assert!(c.is_reporter(acc("carol.testnet")));
}

// ---------- try to cheat ----------

#[test]
#[should_panic(expected = "Only reporters can flag")]
fn cheat_non_reporter_cannot_flag() {
    let mut c = setup();
    as_caller("mallory.testnet", 2_000);
    c.flag(acc("honest.testnet"), "drainer".into(), 100, "trust me".into(), "https://x.com/lie".into());
}

#[test]
#[should_panic(expected = "Only reporters can unflag")]
fn cheat_scammer_cannot_unflag_itself() {
    let mut c = setup();
    alice_flags(&mut c, "scammer.testnet", 95);
    as_caller("scammer.testnet", 3_000);
    c.unflag(acc("scammer.testnet"));
}

#[test]
#[should_panic(expected = "No flag from this reporter on this account")]
fn cheat_reporter_cannot_remove_another_reporters_flag() {
    let mut c = setup();
    alice_flags(&mut c, "scammer.testnet", 95);
    // bob is a reporter but has no flag here; unflag only ever touches the caller's own flag
    as_caller("bob.testnet", 3_000);
    c.unflag(acc("scammer.testnet"));
}

#[test]
fn cheat_reporter_unflag_leaves_other_flags_intact() {
    let mut c = setup();
    alice_flags(&mut c, "scammer.testnet", 95);
    as_caller("bob.testnet", 3_000);
    c.flag(acc("scammer.testnet"), "drainer".into(), 20, "minor".into(), "https://example.com/b".into());
    c.unflag(acc("scammer.testnet"));
    let r = c.get_risk(acc("scammer.testnet"));
    assert_eq!(r.flags.len(), 1);
    assert_eq!(r.flags[0].reporter, acc("alice.testnet"));
    assert_eq!(r.score, 95);
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_reporter_cannot_use_owner_moderation() {
    let mut c = setup();
    alice_flags(&mut c, "scammer.testnet", 95);
    as_caller("bob.testnet", 3_000);
    c.remove_flag(acc("scammer.testnet"), acc("alice.testnet"));
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_reporter_cannot_add_reporters() {
    let mut c = setup();
    as_caller("alice.testnet", 2_000);
    c.add_reporter(acc("sockpuppet.testnet"));
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_outsider_cannot_remove_reporters() {
    let mut c = setup();
    as_caller("scammer.testnet", 2_000);
    c.remove_reporter(acc("alice.testnet"));
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_outsider_cannot_take_ownership() {
    let mut c = setup();
    as_caller("scammer.testnet", 2_000);
    c.transfer_ownership(acc("scammer.testnet"));
}

#[test]
#[should_panic(expected = "Only reporters can flag")]
fn cheat_removed_reporter_cannot_flag() {
    let mut c = setup();
    as_caller("owner.testnet", 2_000);
    c.remove_reporter(acc("alice.testnet"));
    alice_flags(&mut c, "honest.testnet", 90);
}

#[test]
#[should_panic(expected = "Reporters cannot flag themselves")]
fn cheat_reporter_cannot_self_flag() {
    let mut c = setup();
    alice_flags(&mut c, "alice.testnet", 1);
}

#[test]
#[should_panic(expected = "Severity must be 1..=100")]
fn rejects_severity_over_100() {
    let mut c = setup();
    alice_flags(&mut c, "x.testnet", 101);
}

#[test]
#[should_panic(expected = "Evidence URL must be https://")]
fn rejects_non_https_evidence() {
    let mut c = setup();
    as_caller("alice.testnet", 2_000);
    c.flag(acc("x.testnet"), "drainer".into(), 50, "bad".into(), "javascript:alert(1)".into());
}

#[test]
#[should_panic(expected = "Reason must be 1..=280 chars")]
fn rejects_oversized_reason() {
    let mut c = setup();
    as_caller("alice.testnet", 2_000);
    c.flag(acc("x.testnet"), "drainer".into(), 50, "a".repeat(281), "https://e.com".into());
}

#[test]
fn score_rule_is_deterministic() {
    assert_eq!(score_flags(&[]), 0);
    assert_eq!(level_for(0), "none");
    assert_eq!(level_for(1), "warn");
    assert_eq!(level_for(69), "warn");
    assert_eq!(level_for(70), "block");
}
