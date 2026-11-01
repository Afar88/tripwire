//! Tripwire guard vault: a tiny NEAR wallet contract for an AI agent.
//!
//! The agent can do exactly one thing: `request_spend(receiver_id, amount, memo)`.
//! (Give it a function-call access key limited to `request_spend`, or name its own
//! account as `agent`.) Before any NEAR leaves the vault:
//!
//! 1. Local checks: caller is the agent, vault not frozen, amount within the per-payment
//!    cap, the daily cap and the free balance, payee is not the vault itself.
//! 2. Cross-contract view call to the Tripwire registry: `get_risk(receiver_id)`.
//! 3. Callback decides:
//!    - score 0 (registry `none`)        -> pay now
//!    - score 1..69 (registry `warn`)     -> HOLD for the owner (`approve` / `reject`)
//!    - score >= 70 (registry `block`)    -> BLOCK, nothing is paid
//!    - registry unreadable / bad answer  -> HOLD (fail safe: "couldn't check" is never "pay")
//!
//! The owner (a different account) sets caps, approves or rejects held payments,
//! freezes the vault, changes the agent and withdraws funds. The agent can do none of that.

use near_sdk::store::IterableMap;
use near_sdk::{
    env, ext_contract, log, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise,
    PromiseError,
};

/// Registry score at or above which the vault blocks.
pub const BLOCK_AT: u8 = 70;
/// Max payments waiting for the owner (stops an agent from spamming storage).
pub const MAX_HELD: u32 = 20;
/// Recent decisions kept on chain for the owner dashboard.
pub const MAX_RECENT: usize = 20;
/// NEAR kept back so the vault can always pay for its own storage.
pub const STORAGE_RESERVE: NearToken = NearToken::from_millinear(500);
pub const MAX_MEMO_LEN: usize = 140;

const GAS_GET_RISK: Gas = Gas::from_tgas(10);
const GAS_ON_RISK: Gas = Gas::from_tgas(20);
const NANOS_PER_DAY: u64 = 86_400_000_000_000;

#[ext_contract(ext_registry)]
pub trait TripwireRegistry {
    fn get_risk(&self, account_id: AccountId) -> RiskView;
}

#[ext_contract(ext_self)]
pub trait SelfCallbacks {
    fn on_risk(&mut self, request_id: u64) -> String;
}

/// The part of the registry's `get_risk` answer the vault needs (extra fields ignored).
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq)]
pub struct RiskView {
    pub score: u8,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Waiting for the registry answer.
    Checking,
    /// Held for the owner (medium risk or registry unreadable).
    Held,
    Paid,
    Blocked,
    Approved,
    Rejected,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct SpendRequest {
    pub id: u64,
    pub receiver_id: AccountId,
    pub amount: NearToken,
    pub memo: String,
    pub status: Status,
    /// Registry score (None if the registry could not be read).
    pub risk_score: Option<u8>,
    pub note: String,
    pub created_ms: u64,
}

#[near(serializers = [json])]
pub struct VaultConfig {
    pub owner: AccountId,
    pub agent: AccountId,
    pub registry: AccountId,
    pub per_tx_cap: NearToken,
    pub daily_cap: NearToken,
    pub spent_today: NearToken,
    pub frozen: bool,
    pub held_count: u32,
}

#[near]
#[derive(BorshStorageKey)]
enum StorageKey {
    Open,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct GuardVault {
    owner: AccountId,
    agent: AccountId,
    registry: AccountId,
    per_tx_cap: NearToken,
    daily_cap: NearToken,
    day: u64,
    spent_today: NearToken,
    /// NEAR promised to requests that are still Checking or Held.
    reserved: NearToken,
    frozen: bool,
    next_id: u64,
    /// Requests still Checking or Held.
    open: IterableMap<u64, SpendRequest>,
    held_count: u32,
    /// Last decisions (newest last).
    recent: Vec<SpendRequest>,
}

fn emit(event: &str, r: &SpendRequest) {
    log!(
        "EVENT_JSON:{{\"standard\":\"tripwire-vault\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":[{{\"id\":{},\"receiver_id\":\"{}\",\"amount\":\"{}\",\"risk_score\":{}}}]}}",
        event,
        r.id,
        r.receiver_id,
        r.amount.as_yoctonear(),
        r.risk_score.map(|s| s.to_string()).unwrap_or_else(|| "null".into())
    );
}

#[near]
impl GuardVault {
    #[init]
    pub fn new(
        owner_id: AccountId,
        agent_id: AccountId,
        registry_id: AccountId,
        per_tx_cap: NearToken,
        daily_cap: NearToken,
    ) -> Self {
        require!(owner_id != agent_id, "Owner and agent must be different accounts");
        require!(per_tx_cap <= daily_cap, "per_tx_cap must be <= daily_cap");
        Self {
            owner: owner_id,
            agent: agent_id,
            registry: registry_id,
            per_tx_cap,
            daily_cap,
            day: env::block_timestamp() / NANOS_PER_DAY,
            spent_today: NearToken::from_yoctonear(0),
            reserved: NearToken::from_yoctonear(0),
            frozen: false,
            next_id: 0,
            open: IterableMap::new(StorageKey::Open),
            held_count: 0,
            recent: Vec::new(),
        }
    }

    // ---------- agent ----------

    /// The only thing the agent can do. Returns a promise that resolves to the decision
    /// ("paid" / "held" / "blocked").
    pub fn request_spend(&mut self, receiver_id: AccountId, amount: NearToken, memo: Option<String>) -> Promise {
        require!(env::predecessor_account_id() == self.agent, "Only the agent can request a spend");
        require!(!self.frozen, "Vault is frozen by the owner");
        require!(amount.as_yoctonear() > 0, "Amount must be > 0");
        require!(receiver_id != env::current_account_id(), "Cannot pay the vault itself");
        let memo = memo.unwrap_or_default();
        require!(memo.len() <= MAX_MEMO_LEN, "Memo too long");
        require!(amount <= self.per_tx_cap, "Over the per-payment cap");
        self.roll_day();
        let spent = self.spent_today.saturating_add(amount);
        require!(spent <= self.daily_cap, "Over the daily cap");
        require!(amount <= self.free_balance(), "Not enough free balance in the vault");
        require!(self.held_count < MAX_HELD, "Too many payments waiting for the owner");

        // Reserve now so two requests in flight cannot both pass the caps.
        self.spent_today = spent;
        self.reserved = self.reserved.saturating_add(amount);
        let id = self.next_id;
        self.next_id += 1;
        let req = SpendRequest {
            id,
            receiver_id: receiver_id.clone(),
            amount,
            memo,
            status: Status::Checking,
            risk_score: None,
            note: String::new(),
            created_ms: env::block_timestamp_ms(),
        };
        emit("spend_requested", &req);
        self.open.insert(id, req);

        ext_registry::ext(self.registry.clone())
            .with_static_gas(GAS_GET_RISK)
            .get_risk(receiver_id)
            .then(ext_self::ext(env::current_account_id()).with_static_gas(GAS_ON_RISK).on_risk(id))
    }

    /// Callback with the registry's answer. Only the vault itself can call it.
    #[private]
    pub fn on_risk(&mut self, request_id: u64, #[callback_result] risk: Result<RiskView, PromiseError>) -> String {
        // #[private] already enforces this in the deployed contract; checked again so unit tests cover it.
        require!(
            env::predecessor_account_id() == env::current_account_id(),
            "Only the vault can deliver registry answers"
        );
        let mut req = self.open.remove(&request_id).unwrap_or_else(|| env::panic_str("Unknown request"));
        require!(req.status == Status::Checking, "Request already decided");
        match risk {
            Ok(r) if r.score >= BLOCK_AT => {
                req.risk_score = Some(r.score);
                req.status = Status::Blocked;
                req.note = format!("Tripwire registry score {} (block)", r.score);
                self.release(&req, true);
                emit("spend_blocked", &req);
                self.remember(req);
                "blocked".into()
            }
            Ok(r) if r.score == 0 => {
                req.risk_score = Some(0);
                req.status = Status::Paid;
                req.note = "No flags in the Tripwire registry".into();
                self.release(&req, false);
                let _ = Promise::new(req.receiver_id.clone()).transfer(req.amount);
                emit("spend_paid", &req);
                self.remember(req);
                "paid".into()
            }
            Ok(r) => {
                req.risk_score = Some(r.score);
                req.note = format!("Tripwire registry score {} (warn): owner must approve", r.score);
                self.hold(req);
                "held".into()
            }
            Err(_) => {
                req.note = "Tripwire registry unreadable: held for owner (fail safe)".into();
                self.hold(req);
                "held".into()
            }
        }
    }

    // ---------- owner ----------

    /// Pay a held request.
    pub fn approve(&mut self, request_id: u64) -> Promise {
        self.assert_owner();
        let mut req = self.take_held(request_id);
        req.status = Status::Approved;
        req.note = format!("{} | approved by owner", req.note);
        self.release(&req, false);
        emit("spend_approved", &req);
        let p = Promise::new(req.receiver_id.clone()).transfer(req.amount);
        self.remember(req);
        p
    }

    /// Refuse a held request.
    pub fn reject(&mut self, request_id: u64) {
        self.assert_owner();
        let mut req = self.take_held(request_id);
        req.status = Status::Rejected;
        req.note = format!("{} | rejected by owner", req.note);
        self.release(&req, true);
        emit("spend_rejected", &req);
        self.remember(req);
    }

    pub fn set_caps(&mut self, per_tx_cap: NearToken, daily_cap: NearToken) {
        self.assert_owner();
        require!(per_tx_cap <= daily_cap, "per_tx_cap must be <= daily_cap");
        self.per_tx_cap = per_tx_cap;
        self.daily_cap = daily_cap;
    }

    pub fn set_agent(&mut self, agent_id: AccountId) {
        self.assert_owner();
        require!(agent_id != self.owner, "Owner and agent must be different accounts");
        self.agent = agent_id;
    }

    pub fn set_registry(&mut self, registry_id: AccountId) {
        self.assert_owner();
        self.registry = registry_id;
    }

    pub fn freeze(&mut self) {
        self.assert_owner();
        self.frozen = true;
    }

    pub fn unfreeze(&mut self) {
        self.assert_owner();
        self.frozen = false;
    }

    /// Owner takes NEAR out (cannot touch amounts reserved for open requests).
    pub fn withdraw(&mut self, amount: NearToken) -> Promise {
        self.assert_owner();
        require!(amount <= self.free_balance(), "Not enough free balance");
        Promise::new(self.owner.clone()).transfer(amount)
    }

    // ---------- views ----------

    pub fn get_config(&self) -> VaultConfig {
        VaultConfig {
            owner: self.owner.clone(),
            agent: self.agent.clone(),
            registry: self.registry.clone(),
            per_tx_cap: self.per_tx_cap,
            daily_cap: self.daily_cap,
            spent_today: if env::block_timestamp() / NANOS_PER_DAY == self.day {
                self.spent_today
            } else {
                NearToken::from_yoctonear(0)
            },
            frozen: self.frozen,
            held_count: self.held_count,
        }
    }

    pub fn get_open(&self) -> Vec<SpendRequest> {
        self.open.values().cloned().collect()
    }

    pub fn get_request(&self, request_id: u64) -> Option<SpendRequest> {
        self.open
            .get(&request_id)
            .cloned()
            .or_else(|| self.recent.iter().find(|r| r.id == request_id).cloned())
    }

    pub fn get_recent(&self) -> Vec<SpendRequest> {
        self.recent.clone()
    }

    pub fn free_balance(&self) -> NearToken {
        env::account_balance()
            .saturating_sub(self.reserved)
            .saturating_sub(STORAGE_RESERVE)
            .saturating_sub(env::storage_byte_cost().saturating_mul(env::storage_usage() as u128))
    }

    // ---------- internal ----------

    fn assert_owner(&self) {
        require!(env::predecessor_account_id() == self.owner, "Only the owner can do this");
    }

    fn roll_day(&mut self) {
        let today = env::block_timestamp() / NANOS_PER_DAY;
        if today != self.day {
            self.day = today;
            self.spent_today = NearToken::from_yoctonear(0);
        }
    }

    /// Drop the reservation; if `refund_cap`, also give the amount back to today's cap.
    fn release(&mut self, req: &SpendRequest, refund_cap: bool) {
        self.reserved = self.reserved.saturating_sub(req.amount);
        if refund_cap {
            self.roll_day();
            self.spent_today = self.spent_today.saturating_sub(req.amount);
        }
    }

    fn hold(&mut self, mut req: SpendRequest) {
        req.status = Status::Held;
        self.held_count += 1;
        emit("spend_held", &req);
        self.open.insert(req.id, req);
    }

    fn take_held(&mut self, request_id: u64) -> SpendRequest {
        let req = self.open.remove(&request_id).unwrap_or_else(|| env::panic_str("No such open request"));
        require!(req.status == Status::Held, "Request is not held");
        self.held_count -= 1;
        req
    }

    fn remember(&mut self, req: SpendRequest) {
        self.recent.push(req);
        if self.recent.len() > MAX_RECENT {
            self.recent.remove(0);
        }
    }
}

#[cfg(test)]
mod tests;
