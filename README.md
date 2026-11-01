# How to test in 2 minutes

Requirements: Rust (stable, via rustup) and Node.js 18+. Everything below runs locally or against free public NEAR **testnet** RPC. Nothing is signed or sent.

1. **Run the contract's tests, including the "try to cheat" cases**
   ```bash
   cd contract && cargo test
   ```
   → See `test result: ok. 24 passed`. Tests named `cheat_*` show that a non-reporter cannot flag, a scammer cannot unflag itself, a reporter cannot remove another reporter's flag or use owner powers, and outsiders cannot add reporters or take ownership.

2. **Install the checker and run its tests**
   ```bash
   cd ../checker && npm install && npm test
   ```
   → See `# pass 7`, `# fail 0`.

3. **Check a known-bad account (offline demo registry)**
   ```bash
   npm run -s check -- ref-flnance.testnet --fixture fixtures/demo-registry.json
   ```
   → See `⛔ BLOCK  ref-flnance.testnet  (risk 90/100 …)` with the reporter, reason and evidence link. Exit code is `2`.

4. **Check a clean account**
   ```bash
   npm run -s check -- lst.ref-dev.testnet --fixture fixtures/demo-registry.json
   ```
   → See `✅ ALLOW … (risk 0/100 …)`. Exit code `0`.

5. **Rhea example: guard a real rNEAR stake transaction**
   ```bash
   npm run -s demo:rhea
   ```
   → The Rhea `rnear-sdk` builds a testnet stake transaction to `lst.ref-dev.testnet` (it reads the live testnet RPC), and Tripwire answers **ALLOW**. Then the same transaction, redirected by a simulated prompt injection to `lst-ref-dev.testnet`, gets **BLOCK** (registry flag + lookalike name). The script ends with `Summary: honest stake -> ALLOW, poisoned stake -> BLOCK`.

6. **Fail-safe against the live network**
   ```bash
   npm run -s check -- friend.testnet --registry lst.ref-dev.testnet
   ```
   → Pointing the checker at a contract that is not a Tripwire registry gives `⚠️ WARN … Could not read the Tripwire registry … treat as unverified` (exit `1`). Tripwire never turns "I couldn't check" into ALLOW.

> Status (Nov 1, 2026): the registry contract is built and tested locally. Testnet deployment is the next step; once deployed, replace `--fixture …` with `--registry <tripwire-contract>.testnet` in steps 3–4, or set `TRIPWIRE_REGISTRY` for step 5.

---

## What it is

Tripwire is a wallet-agnostic scam / risk registry and checker for NEAR. Before an AI agent or a wallet sends funds to an account or calls a contract, it asks Tripwire, and gets back **ALLOW**, **WARN** or **BLOCK** with plain-English reasons.

It has two parts:

- **Registry contract** (Rust, `contract/`): an on-chain list of flagged accounts. Anyone can call `get_risk(account_id)`; trusted reporters file flags with a category, severity, reason, evidence URL and timestamp; the owner manages reporters and moderates.
- **Checker** (TypeScript, `checker/`): a library + CLI that reads the registry over public NEAR RPC, adds local signals (lookalike names, account does not exist), and returns a verdict. `checkTransaction()` checks every account a transaction touches (its receiver plus `receiver_id`/`account_id`-style arguments) before it is signed.

## The problem

AI agents on NEAR now hold real money. One poisoned web page or message can tell an agent to "send 50 NEAR to `ref-flnance.near`" or swap a contract address, and the agent will happily sign it. Spending caps limit the damage but do not tell the agent *who* it is paying. NEAR has no public, machine-readable scam address list that agents can query. Tripwire is that list plus the check that uses it.

## Architecture

```
 agent / wallet ──build tx──▶ Tripwire checker ──view call (free RPC)──▶ Tripwire registry contract
                                   │                                        get_risk(account) → score + flags
                                   │  + local signals: lookalike name, account missing
                                   ▼
                     ALLOW → hand to signer · WARN → hold for owner · BLOCK → refuse
```

### Contract API (`contract/src/lib.rs`)

| Method | Who | What |
| --- | --- | --- |
| `new(owner_id)` | deployer | init |
| `add_reporter(account_id)` / `remove_reporter(account_id)` | owner | manage reporters |
| `remove_flag(account_id, reporter)` | owner | moderation |
| `transfer_ownership(new_owner)` | owner | e.g. hand to a DAO |
| `flag(account_id, category, severity 1–100, reason ≤280, evidence_url https://)` | reporter | file or update own flag (one per reporter per account; cannot flag self) |
| `unflag(account_id)` | reporter | remove **own** flag only |
| `get_risk(account_id)` | view | `{ account_id, score 0–100, level: none/warn/block, flags[] }` |
| `get_flags`, `list_flagged(from_index, limit)`, `flagged_count`, `get_reporters`, `is_reporter`, `get_owner` | view | listings |

**Score rule:** highest severity among flags, +10 per extra independent reporter, capped at 100. Level: `block` at ≥70, `warn` at ≥1, else `none`. All changes emit NEP-297-style `EVENT_JSON` logs (`standard: "tripwire"`).

### Checker verdict

`score = max(registry score, local signals)`: lookalike of a well-known name → 60, account does not exist → 40, registry unreadable → 50. ≥70 BLOCK, ≥1 WARN, else ALLOW. CLI exit code 0/1/2 = ALLOW/WARN/BLOCK so it can gate scripts.

### Repository layout

```
contract/            Rust NEAR contract + unit tests (src/tests.rs)
checker/src/         tripwire.ts (library), cli.ts (CLI)
checker/examples/    rhea-stake.ts (Rhea rnear SDK example)
checker/fixtures/    demo-registry.json (offline demo data, clearly labelled)
checker/test/        checker unit tests (node:test)
```

## Libraries, SDKs and partners used

- **near-sdk** 5.30 (Rust) — contract.
- **near-api-js** 7.4 — `JsonRpcProvider` view calls from the checker.
- **Rhea Finance `@rhea-finance/rnear-sdk`** 0.1.1 — `RNearClient({ network: "testnet" }).buildStakeTransaction()` in the Rhea example. Testnet contracts: `lst.ref-dev.testnet` (rNEAR), `ref-finance-101.testnet` (exchange). Build only; the example never signs or sends.
- **TypeScript**, **tsx**, **@types/node** — tooling. Tests use Node's built-in `node:test`.
- Public NEAR testnet RPC: `https://test.rpc.fastnear.com` with failover to `https://rpc.testnet.near.org`.
- **AI-generated code disclosure:** this code was written with the help of an AI assistant (Hark), reviewed and directed by the author.

All code in this repository was written during NEAR HACKATHON (from Nov 1, 2026).

## Network

Tripwire is **testnet-only**. No mainnet deployment, no real funds. The demo fixture accounts (`ref-flnance.testnet`, `lst-ref-dev.testnet`, `ghost-wallet-x9.testnet`) are illustrative data for offline testing, not real accusations.

## License

MIT, see [LICENSE](LICENSE).
