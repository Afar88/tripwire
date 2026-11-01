//! Tripwire registry: an open, on-chain scam / risk registry for NEAR.
//!
//! Wallets and AI agents call `get_risk(account_id)` before they send funds to an
//! account or call a contract. Trusted reporters flag accounts with a reason, an
//! evidence URL and a severity; the owner manages who may report.
//!
//! Roles
//! - owner: adds/removes reporters, can remove any flag (moderation), can hand over ownership.
//! - reporter: can flag an account (one active flag per reporter per account; flagging
//!   again updates it) and can remove only their own flag.
//! - everyone: read-only view methods.

use near_sdk::store::{IterableMap, IterableSet};
use near_sdk::{env, log, near, require, AccountId, BorshStorageKey, PanicOnDefault};

/// Max length of a flag's free-text reason.
pub const MAX_REASON_LEN: usize = 280;
/// Max length of an evidence URL.
pub const MAX_URL_LEN: usize = 256;
/// Max length of a category tag.
pub const MAX_CATEGORY_LEN: usize = 32;
/// Max page size for list views.
pub const MAX_PAGE: u32 = 100;
/// Each additional independent reporter adds this much to the score.
pub const EXTRA_REPORTER_BONUS: u32 = 10;

/// Score at or above which a checker should BLOCK.
pub const BLOCK_THRESHOLD: u8 = 70;
/// Score at or above which a checker should WARN.
pub const WARN_THRESHOLD: u8 = 1;

#[near]
#[derive(BorshStorageKey)]
enum StorageKey {
    Reporters,
    Flags,
}

/// One reporter's flag on one account.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct Flag {
    /// Who filed it.
    pub reporter: AccountId,
    /// Short tag, e.g. "drainer", "phishing", "lookalike", "rug".
    pub category: String,
    /// 1..=100: how bad the reporter believes this is.
    pub severity: u8,
    /// Human-readable reason.
    pub reason: String,
    /// Link to evidence (explorer tx, report, screenshot). Must be https://.
    pub evidence_url: String,
    /// Block timestamp (ms) when the flag was filed or last updated.
    pub timestamp_ms: u64,
}

/// What `get_risk` returns.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq)]
pub struct RiskReport {
    pub account_id: AccountId,
    /// 0 = no flags, 100 = maximum risk.
    pub score: u8,
    /// "none" | "warn" | "block": the registry's own suggestion from the score.
    pub level: String,
    pub flags: Vec<Flag>,
}

/// One row of `list_flagged`.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq)]
pub struct FlaggedAccount {
    pub account_id: AccountId,
    pub score: u8,
    pub flag_count: u32,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Tripwire {
    owner: AccountId,
    reporters: IterableSet<AccountId>,
    flags: IterableMap<AccountId, Vec<Flag>>,
}

/// Score rule: the worst single flag, plus a bonus for each extra independent reporter, capped at 100.
pub fn score_flags(flags: &[Flag]) -> u8 {
    if flags.is_empty() {
        return 0;
    }
    let max = flags.iter().map(|f| f.severity as u32).max().unwrap_or(0);
    let extra = (flags.len() as u32 - 1) * EXTRA_REPORTER_BONUS;
    (max + extra).min(100) as u8
}

pub fn level_for(score: u8) -> &'static str {
    if score >= BLOCK_THRESHOLD {
        "block"
    } else if score >= WARN_THRESHOLD {
        "warn"
    } else {
        "none"
    }
}

fn emit(event: &str, data: String) {
    log!(
        "EVENT_JSON:{{\"standard\":\"tripwire\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":[{}]}}",
        event,
        data
    );
}

#[near]
impl Tripwire {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        Self {
            owner: owner_id,
            reporters: IterableSet::new(StorageKey::Reporters),
            flags: IterableMap::new(StorageKey::Flags),
        }
    }

    // ---------- owner ----------

    pub fn add_reporter(&mut self, account_id: AccountId) {
        self.assert_owner();
        if self.reporters.insert(account_id.clone()) {
            emit("reporter_added", format!("{{\"account_id\":\"{}\"}}", account_id));
        }
    }

    pub fn remove_reporter(&mut self, account_id: AccountId) {
        self.assert_owner();
        require!(self.reporters.remove(&account_id), "Not a reporter");
        emit("reporter_removed", format!("{{\"account_id\":\"{}\"}}", account_id));
    }

    /// Owner moderation: remove one reporter's flag from an account.
    pub fn remove_flag(&mut self, account_id: AccountId, reporter: AccountId) {
        self.assert_owner();
        self.delete_flag(&account_id, &reporter);
    }

    pub fn transfer_ownership(&mut self, new_owner: AccountId) {
        self.assert_owner();
        emit(
            "ownership_transferred",
            format!("{{\"old_owner\":\"{}\",\"new_owner\":\"{}\"}}", self.owner, new_owner),
        );
        self.owner = new_owner;
    }

    // ---------- reporters ----------

    /// File (or update) the caller's flag on `account_id`.
    pub fn flag(
        &mut self,
        account_id: AccountId,
        category: String,
        severity: u8,
        reason: String,
        evidence_url: String,
    ) {
        let reporter = env::predecessor_account_id();
        require!(self.reporters.contains(&reporter), "Only reporters can flag");
        require!(account_id != reporter, "Reporters cannot flag themselves");
        require!((1..=100).contains(&severity), "Severity must be 1..=100");
        require!(
            !category.is_empty() && category.len() <= MAX_CATEGORY_LEN,
            "Category must be 1..=32 chars"
        );
        require!(
            !reason.trim().is_empty() && reason.len() <= MAX_REASON_LEN,
            "Reason must be 1..=280 chars"
        );
        require!(
            evidence_url.starts_with("https://") && evidence_url.len() <= MAX_URL_LEN,
            "Evidence URL must be https:// and at most 256 chars"
        );

        let new_flag = Flag {
            reporter: reporter.clone(),
            category,
            severity,
            reason,
            evidence_url,
            timestamp_ms: env::block_timestamp_ms(),
        };

        let mut list = self.flags.get(&account_id).cloned().unwrap_or_default();
        match list.iter_mut().find(|f| f.reporter == reporter) {
            Some(existing) => *existing = new_flag,
            None => list.push(new_flag),
        }
        let score = score_flags(&list);
        self.flags.insert(account_id.clone(), list);
        emit(
            "flag",
            format!(
                "{{\"account_id\":\"{}\",\"reporter\":\"{}\",\"severity\":{},\"score\":{}}}",
                account_id, reporter, severity, score
            ),
        );
    }

    /// Remove the caller's own flag from `account_id`. Reporters cannot remove other reporters' flags.
    pub fn unflag(&mut self, account_id: AccountId) {
        let reporter = env::predecessor_account_id();
        require!(self.reporters.contains(&reporter), "Only reporters can unflag");
        self.delete_flag(&account_id, &reporter);
    }

    // ---------- views ----------

    /// The main query: risk score, suggested level and every flag on an account.
    pub fn get_risk(&self, account_id: AccountId) -> RiskReport {
        let flags = self.flags.get(&account_id).cloned().unwrap_or_default();
        let score = score_flags(&flags);
        RiskReport {
            account_id,
            score,
            level: level_for(score).to_string(),
            flags,
        }
    }

    pub fn get_flags(&self, account_id: AccountId) -> Vec<Flag> {
        self.flags.get(&account_id).cloned().unwrap_or_default()
    }

    /// Paged list of every flagged account with its score.
    pub fn list_flagged(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<FlaggedAccount> {
        let from = from_index.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(50).min(MAX_PAGE) as usize;
        self.flags
            .iter()
            .skip(from)
            .take(limit)
            .map(|(id, flags)| FlaggedAccount {
                account_id: id.clone(),
                score: score_flags(flags),
                flag_count: flags.len() as u32,
            })
            .collect()
    }

    pub fn flagged_count(&self) -> u32 {
        self.flags.len()
    }

    pub fn get_reporters(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<AccountId> {
        let from = from_index.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(50).min(MAX_PAGE) as usize;
        self.reporters.iter().skip(from).take(limit).cloned().collect()
    }

    pub fn is_reporter(&self, account_id: AccountId) -> bool {
        self.reporters.contains(&account_id)
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    // ---------- internal ----------

    fn assert_owner(&self) {
        require!(env::predecessor_account_id() == self.owner, "Only the owner can do this");
    }

    fn delete_flag(&mut self, account_id: &AccountId, reporter: &AccountId) {
        let mut list = self.flags.get(account_id).cloned().unwrap_or_default();
        let before = list.len();
        list.retain(|f| &f.reporter != reporter);
        require!(list.len() < before, "No flag from this reporter on this account");
        if list.is_empty() {
            self.flags.remove(account_id);
        } else {
            self.flags.insert(account_id.clone(), list);
        }
        emit(
            "unflag",
            format!("{{\"account_id\":\"{}\",\"reporter\":\"{}\"}}", account_id, reporter),
        );
    }
}

#[cfg(test)]
mod tests;
