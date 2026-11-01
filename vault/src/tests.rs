use super::*;
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::{testing_env, PromiseResult};

const OWNER: &str = "owner.testnet";
const AGENT: &str = "agent.testnet";
const VAULT: &str = "vault.testnet";
const REGISTRY: &str = "tripwire-registry.testnet";
const DAY_MS: u64 = 86_400_000;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}
fn near(n: u128) -> NearToken {
    NearToken::from_near(n)
}

fn ctx(who: &str, ts_ms: u64) {
    testing_env!(VMContextBuilder::new()
        .current_account_id(acc(VAULT))
        .predecessor_account_id(acc(who))
        .account_balance(near(20))
        .block_timestamp(ts_ms * 1_000_000)
        .build());
}

/// Vault with 20 NEAR, caps 2 NEAR per payment / 5 NEAR per day.
fn setup() -> GuardVault {
    ctx(OWNER, 1_000);
    GuardVault::new(acc(OWNER), acc(AGENT), acc(REGISTRY), near(2), near(5))
}

/// Agent asks to pay; returns the request id.
fn request(v: &mut GuardVault, to: &str, n: u128) -> u64 {
    ctx(AGENT, 2_000);
    let id = v.next_id;
    let _ = v.request_spend(acc(to), near(n), Some("test".into()));
    id
}

/// Registry callback, as the vault itself.
fn answer(v: &mut GuardVault, id: u64, score: Option<u8>) -> String {
    ctx(VAULT, 3_000);
    let r = match score {
        Some(s) => Ok(RiskView { score: s }),
        None => Err(PromiseError::Failed),
    };
    match v.on_risk(id, r) {
        PromiseOrValue::Value(s) => s,
        PromiseOrValue::Promise(_) => "sending".into(),
    }
}

/// Deliver the payout transfer's on-chain result to `on_transfer_complete`, as `who`.
/// `balance` is the vault balance at that point (a failed transfer has been refunded).
fn settle_as(v: &mut GuardVault, who: &str, id: u64, ok: bool, balance: NearToken) -> String {
    let res = if ok { PromiseResult::Successful(vec![]) } else { PromiseResult::Failed };
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(VAULT))
            .predecessor_account_id(acc(who))
            .account_balance(balance)
            .block_timestamp(3_500 * 1_000_000)
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![res],
    );
    v.on_transfer_complete(id)
}

fn settle(v: &mut GuardVault, id: u64, ok: bool) -> String {
    settle_as(v, VAULT, id, ok, near(20))
}

// ---------- happy paths ----------

#[test]
fn clean_payee_is_paid() {
    let mut v = setup();
    let id = request(&mut v, "friend.testnet", 1);
    assert_eq!(v.get_open()[0].status, Status::Checking);
    assert_eq!(answer(&mut v, id, Some(0)), "sending");
    assert_eq!(v.get_request(id).unwrap().status, Status::Sending);
    assert_eq!(settle(&mut v, id, true), "paid");
    assert!(v.get_open().is_empty());
    assert_eq!(v.get_recent()[0].status, Status::Paid);
    assert_eq!(v.get_config().spent_today, near(1));
}

#[test]
fn flagged_payee_is_blocked_and_cap_refunded() {
    let mut v = setup();
    let id = request(&mut v, "tripwire-drainer-demo.testnet", 2);
    assert_eq!(answer(&mut v, id, Some(95)), "blocked");
    let r = v.get_request(id).unwrap();
    assert_eq!(r.status, Status::Blocked);
    assert_eq!(r.risk_score, Some(95));
    assert_eq!(v.get_config().spent_today, near(0));
    assert_eq!(v.reserved, near(0));
}

#[test]
fn medium_risk_is_held_then_owner_approves() {
    let mut v = setup();
    let id = request(&mut v, "wrap-near-swap.testnet", 1);
    assert_eq!(answer(&mut v, id, Some(40)), "held");
    assert_eq!(v.get_config().held_count, 1);
    ctx(OWNER, 4_000);
    let _ = v.approve(id);
    assert_eq!(v.get_request(id).unwrap().status, Status::Sending);
    assert_eq!(v.get_config().held_count, 0);
    assert_eq!(v.reserved, near(0));
    assert_eq!(settle(&mut v, id, true), "paid");
    let r = v.get_request(id).unwrap();
    assert_eq!(r.status, Status::Paid);
    assert!(r.note.contains("approved by owner"));
}

#[test]
fn owner_rejects_held_and_cap_is_refunded() {
    let mut v = setup();
    let id = request(&mut v, "wrap-near-swap.testnet", 2);
    answer(&mut v, id, Some(40));
    ctx(OWNER, 4_000);
    v.reject(id);
    assert_eq!(v.get_request(id).unwrap().status, Status::Rejected);
    assert_eq!(v.get_config().spent_today, near(0));
}

#[test]
fn registry_unreadable_fails_safe_to_held() {
    let mut v = setup();
    let id = request(&mut v, "friend.testnet", 1);
    assert_eq!(answer(&mut v, id, None), "held");
    let r = v.get_request(id).unwrap();
    assert_eq!(r.status, Status::Held);
    assert_eq!(r.risk_score, None);
}

#[test]
fn daily_cap_resets_next_day() {
    let mut v = setup();
    for _ in 0..2 {
        let id = request(&mut v, "friend.testnet", 2);
        answer(&mut v, id, Some(0));
    }
    assert_eq!(v.get_config().spent_today, near(4));
    ctx(AGENT, 2_000 + DAY_MS);
    let _ = v.request_spend(acc("friend.testnet"), near(2), None);
    assert_eq!(v.spent_today, near(2));
}

#[test]
fn owner_controls_work() {
    let mut v = setup();
    ctx(OWNER, 5_000);
    v.set_caps(near(1), near(3));
    v.freeze();
    assert!(v.get_config().frozen);
    v.unfreeze();
    v.set_agent(acc("agent2.testnet"));
    assert_eq!(v.get_config().agent, acc("agent2.testnet"));
    let _ = v.withdraw(near(1));
}

// ---------- cheat cases ----------

#[test]
#[should_panic(expected = "Only the agent can request a spend")]
fn cheat_stranger_cannot_request_spend() {
    let mut v = setup();
    ctx("mallory.testnet", 2_000);
    let _ = v.request_spend(acc("mallory.testnet"), near(1), None);
}

#[test]
#[should_panic(expected = "Only the agent can request a spend")]
fn cheat_owner_key_is_not_agent_path() {
    // Even the owner cannot use request_spend; owner money moves only via withdraw/approve.
    let mut v = setup();
    ctx(OWNER, 2_000);
    let _ = v.request_spend(acc("friend.testnet"), near(1), None);
}

#[test]
#[should_panic(expected = "Over the per-payment cap")]
fn cheat_agent_over_per_payment_cap() {
    let mut v = setup();
    request(&mut v, "friend.testnet", 3);
}

#[test]
#[should_panic(expected = "Over the daily cap")]
fn cheat_agent_splits_payments_past_daily_cap() {
    let mut v = setup();
    for _ in 0..3 {
        request(&mut v, "friend.testnet", 2); // 2 + 2 + 2 > 5, even before callbacks land
    }
}

#[test]
#[should_panic(expected = "Only the vault can deliver registry answers")]
fn cheat_agent_fakes_clean_registry_answer() {
    let mut v = setup();
    let id = request(&mut v, "tripwire-drainer-demo.testnet", 1);
    ctx(AGENT, 3_000);
    let _ = v.on_risk(id, Ok(RiskView { score: 0 }));
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_agent_approves_own_held_payment() {
    let mut v = setup();
    let id = request(&mut v, "wrap-near-swap.testnet", 1);
    answer(&mut v, id, Some(40));
    ctx(AGENT, 4_000);
    let _ = v.approve(id);
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_agent_raises_caps() {
    let mut v = setup();
    ctx(AGENT, 4_000);
    v.set_caps(near(100), near(100));
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_agent_withdraws() {
    let mut v = setup();
    ctx(AGENT, 4_000);
    let _ = v.withdraw(near(1));
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_agent_swaps_registry() {
    let mut v = setup();
    ctx(AGENT, 4_000);
    v.set_registry(acc("fake-registry.testnet"));
}

#[test]
#[should_panic(expected = "Only the owner can do this")]
fn cheat_agent_unfreezes() {
    let mut v = setup();
    ctx(OWNER, 4_000);
    v.freeze();
    ctx(AGENT, 4_000);
    v.unfreeze();
}

#[test]
#[should_panic(expected = "Vault is frozen by the owner")]
fn cheat_frozen_vault_refuses() {
    let mut v = setup();
    ctx(OWNER, 4_000);
    v.freeze();
    request(&mut v, "friend.testnet", 1);
}

#[test]
#[should_panic(expected = "Request already decided")]
fn cheat_replayed_callback() {
    let mut v = setup();
    let id = request(&mut v, "wrap-near-swap.testnet", 1);
    answer(&mut v, id, Some(40)); // held
    answer(&mut v, id, Some(0)); // a second "clean" answer must not pay it
}

#[test]
#[should_panic(expected = "Request is not held")]
fn cheat_owner_approves_while_still_checking() {
    let mut v = setup();
    let id = request(&mut v, "tripwire-drainer-demo.testnet", 1);
    ctx(OWNER, 4_000);
    let _ = v.approve(id);
}

#[test]
#[should_panic(expected = "Too many payments waiting for the owner")]
fn cheat_agent_spams_held_requests() {
    let mut v = setup();
    ctx(OWNER, 1_000);
    v.set_caps(NearToken::from_millinear(1), near(5));
    for _ in 0..=MAX_HELD {
        ctx(AGENT, 2_000);
        let id = v.next_id;
        let _ = v.request_spend(acc("wrap-near-swap.testnet"), NearToken::from_millinear(1), None);
        answer(&mut v, id, Some(40));
    }
}

#[test]
#[should_panic(expected = "Cannot pay the vault itself")]
fn cheat_pay_self_loop() {
    let mut v = setup();
    request(&mut v, VAULT, 1);
}

#[test]
#[should_panic(expected = "Owner and agent must be different accounts")]
fn cheat_agent_made_owner() {
    let mut v = setup();
    ctx(OWNER, 4_000);
    v.set_agent(acc(OWNER));
}

#[test]
#[should_panic(expected = "Not enough free balance")]
fn cheat_withdraw_reserved_funds() {
    let mut v = setup();
    ctx(OWNER, 1_000);
    v.set_caps(near(2), near(20));
    for _ in 0..5 {
        request(&mut v, "friend.testnet", 2); // 10 NEAR reserved while checking
    }
    ctx(OWNER, 4_000);
    let _ = v.withdraw(near(10)); // 20 - 10 reserved - storage reserve < 10
}

// ---------- payout result callback ----------

#[test]
fn successful_transfer_is_paid_with_event() {
    let mut v = setup();
    let id = request(&mut v, "friend.testnet", 1);
    answer(&mut v, id, Some(0));
    assert!(get_logs().iter().any(|l| l.contains("\"event\":\"spend_sending\"")));
    assert_eq!(settle(&mut v, id, true), "paid");
    assert!(get_logs().iter().any(|l| l.contains("\"event\":\"spend_paid\"")));
    let r = v.get_request(id).unwrap();
    assert_eq!(r.status, Status::Paid);
    assert_eq!(v.get_config().spent_today, near(1));
    assert_eq!(v.reserved, near(0));
}

#[test]
fn failed_transfer_to_missing_account_is_reverted() {
    let mut v = setup();
    let before = v.free_balance();
    let id = request(&mut v, "never-created-ghost.testnet", 2);
    assert_eq!(answer(&mut v, id, Some(0)), "sending");
    assert_eq!(v.get_config().spent_today, near(2));
    // Transfer failed on chain: the runtime refunded the 2 NEAR, so the balance is back to 20.
    assert_eq!(settle(&mut v, id, false), "reverted");
    assert!(get_logs().iter().any(|l| l.contains("\"event\":\"spend_reverted\"")));
    let r = v.get_request(id).unwrap();
    assert_eq!(r.status, Status::Reverted);
    assert!(r.note.contains("transfer failed on chain"));
    assert_eq!(v.get_config().spent_today, near(0), "daily cap restored");
    assert_eq!(v.reserved, near(0));
    assert_eq!(v.free_balance(), before, "vault balance fully available again");
    assert!(v.get_open().is_empty());
}

#[test]
fn reverted_payment_does_not_eat_the_daily_cap() {
    let mut v = setup(); // 2 per payment, 5 per day
    let id = request(&mut v, "never-created-ghost.testnet", 2);
    answer(&mut v, id, Some(0));
    settle(&mut v, id, false);
    // 2 + 2 + 1 = 5 still fits because the failed 2 was given back.
    for n in [2, 2, 1] {
        let id = request(&mut v, "friend.testnet", n);
        answer(&mut v, id, Some(0));
        assert_eq!(settle(&mut v, id, true), "paid");
    }
    assert_eq!(v.get_config().spent_today, near(5));
}

#[test]
fn owner_approved_transfer_that_fails_is_reverted() {
    let mut v = setup();
    let id = request(&mut v, "wrap-near-swap-ghost.testnet", 2);
    answer(&mut v, id, Some(40));
    ctx(OWNER, 4_000);
    let _ = v.approve(id);
    assert_eq!(settle(&mut v, id, false), "reverted");
    let r = v.get_request(id).unwrap();
    assert_eq!(r.status, Status::Reverted);
    assert!(r.note.contains("approved by owner"));
    assert_eq!(v.get_config().spent_today, near(0));
}

#[test]
#[should_panic(expected = "Only the vault can report payout results")]
fn cheat_outsider_calls_transfer_callback() {
    let mut v = setup();
    let id = request(&mut v, "never-created-ghost.testnet", 1);
    answer(&mut v, id, Some(0));
    settle_as(&mut v, "mallory.testnet", id, false, near(20)); // fake "failed" to free the cap
}

#[test]
#[should_panic(expected = "Only the vault can report payout results")]
fn cheat_agent_calls_transfer_callback() {
    let mut v = setup();
    let id = request(&mut v, "friend.testnet", 1);
    answer(&mut v, id, Some(0));
    settle_as(&mut v, AGENT, id, true, near(20));
}

#[test]
#[should_panic(expected = "Request is not sending")]
fn cheat_transfer_callback_on_held_request() {
    let mut v = setup();
    let id = request(&mut v, "wrap-near-swap.testnet", 1);
    answer(&mut v, id, Some(40)); // held, never sent
    settle(&mut v, id, false);
}

#[test]
#[should_panic(expected = "Unknown request")]
fn cheat_replayed_transfer_callback() {
    let mut v = setup();
    let id = request(&mut v, "friend.testnet", 1);
    answer(&mut v, id, Some(0));
    settle(&mut v, id, true);
    settle(&mut v, id, false); // a second "failed" must not refund the cap again
}

#[test]
fn reverted_payment_from_yesterday_does_not_refund_todays_cap() {
    let mut v = setup();
    let id = request(&mut v, "never-created-ghost.testnet", 2);
    answer(&mut v, id, Some(0));
    // next day: agent spends 1, then yesterday's failed transfer result lands
    ctx(AGENT, 2_000 + DAY_MS);
    let id2 = v.next_id;
    let _ = v.request_spend(acc("friend.testnet"), near(1), None);
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(VAULT))
            .predecessor_account_id(acc(VAULT))
            .account_balance(near(20))
            .block_timestamp((3_000 + DAY_MS) * 1_000_000)
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![PromiseResult::Failed],
    );
    assert_eq!(v.on_transfer_complete(id), "reverted");
    assert_eq!(v.spent_today, near(1));
    let _ = id2;
}
