# How to test in 2 minutes

Tripwire is **live on NEAR testnet**. Nothing below needs a wallet, a key or any NEAR: the checker only makes free read-only RPC calls.

**Fastest: open https://tripwire-near.vercel.app** → tap an example (⛔ known drainer, ✅ real Rhea contract, 🎭 its lookalike, 🤖 clean account with a scam reason) → see ALLOW / WARN / BLOCK with reasons, read live from the registry contract below. The page is `web/` in this repo: a static page plus one read-only Vercel function (`web/api/check.js`, same rules as `checker/src`); the NEAR AI key stays server-side and AI calls are rate-limited.

| What | Testnet account | Explorer |
| --- | --- | --- |
| Registry contract | `tripwire-registry-44ea74.testnet` | https://testnet.nearblocks.io/address/tripwire-registry-44ea74.testnet |
| Guard vault contract | `tripwire-vault-44ea74.testnet` | https://testnet.nearblocks.io/address/tripwire-vault-44ea74.testnet |
| Reporter (files flags) | `tripwire-reporter-44ea74.testnet` | https://testnet.nearblocks.io/address/tripwire-reporter-44ea74.testnet |
| Demo agent / owner | `tripwire-agent-44ea74.testnet` / `tripwire-owner-44ea74.testnet` | |
| Demo "drainer" (test data) | `tripwire-drainer-demo-44ea74.testnet` | |

**Demo video (66 s, narrated, real runs against testnet):** [`docs/tripwire-demo.mp4`](docs/tripwire-demo.mp4)

Requirements: Node.js 18+ (and Rust stable for the cargo tests in step 6).

1. **Ask the live registry about a known-bad account**
   ```bash
   cd checker && npm install
   npm run -s check -- tripwire-drainer-demo-44ea74.testnet
   ```
   → `⛔ BLOCK  tripwire-drainer-demo-44ea74.testnet  (risk 95/100, source: tripwire-registry-44ea74.testnet …)` with the reporter, reason and evidence link. Exit code `2`.

2. **A clean account, and a medium-risk one**
   ```bash
   npm run -s check -- lst.ref-dev.testnet        # ✅ ALLOW (risk 0/100), exit 0
   npm run -s check -- wrap-near-swap.testnet     # ⚠️ WARN (risk 40/100), exit 1
   npm run -s check -- ref-flnance.testnet        # ⛔ BLOCK: registry flag + "account does not exist"
   ```

3. **Rhea example: guard a real rNEAR stake transaction**
   ```bash
   npm run -s demo:rhea
   ```
   → The Rhea `rnear-sdk` builds a testnet stake transaction to `lst.ref-dev.testnet`; Tripwire answers **ALLOW**. The same transaction redirected by a simulated prompt injection to `lst-ref-dev.testnet` gets **BLOCK**. Ends with `Summary: honest stake -> ALLOW, poisoned stake -> BLOCK`. (Build only, never signed.)

4. **OutLayer approver (dry run)**
   ```bash
   npm run -s demo:outlayer
   ```
   → Three *simulated* OutLayer pending approvals checked against the live registry: friend → `vote: approve`, drainer → `vote: reject`, token transfer to a medium-risk account → `leave_for_owner`. Nothing is signed or sent (see "What is live vs stubbed").

5. **NEAR AI second opinion (optional, needs a NEAR AI Cloud key)**
   ```bash
   export NEARAI_API_KEY=...   # free key from https://cloud.near.ai
   npm run -s check -- tripwire-reporter-44ea74.testnet --ai \
     --purpose "Your airdrop is ready. Send 2 NEAR gas fee to verify your wallet and unlock 500 NEAR"
   ```
   → The registry says the account is clean, but NEAR AI reads the payment purpose, spots a pay-to-claim scam and raises the verdict to **WARN** (exit `1`):
   ```
   ⚠️  WARN  tripwire-reporter-44ea74.testnet  (risk 30/100, …)
      - No flags in the Tripwire registry and no local warning signs
      - NEAR AI (openai/gpt-5-nano) flagged the payment purpose as a likely scam; raised to WARN
      🤖 NEAR AI (openai/gpt-5-nano): Payment request asks to send 2 NEAR gas fee to verify wallet and unlock 500 NEAR, a typical scam/claim-based urgency tactic.
   ```
   With `--purpose "paying Ada for logo design, invoice #12"` it stays **ALLOW** with a one-line explanation. The AI can only raise ALLOW → WARN: it can never unblock a flagged account or lower any verdict, and if NEAR AI is down or no key is set, the registry verdict stands. Behind a corporate/sandbox proxy, run with `NODE_USE_ENV_PROXY=1` (Node 22+).

6. **Run all tests, including the "try to cheat" cases**
   ```bash
   npm test                          # checker + NEAR AI layer + OutLayer adapter: # pass 18, # fail 0
   cd ../contract && cargo test      # registry: 24 passed
   cd ../vault && cargo test         # guard vault: 33 passed
   ```

7. **See the guard vault's real on-chain decisions** (read-only)
   ```bash
   curl -s https://rpc.testnet.near.org -H 'content-type: application/json' \
     -d '{"jsonrpc":"2.0","id":1,"method":"query","params":{"request_type":"call_function","finality":"final","account_id":"tripwire-vault-44ea74.testnet","method_name":"get_recent","args_base64":"e30="}}' \
     | python3 -c "import sys,json; print(json.dumps(json.loads(bytes(json.load(sys.stdin)['result']['result'])), indent=1))"
   ```
   → Five requests the demo agent made on Nov 1, 2026: 0.5 NEAR to a clean account `Paid` (tx [`Ci3N1eb…`](https://testnet.nearblocks.io/txns/Ci3N1ebz8j3eraMjp1WDPUhxQP5iuBNKu3kf8d3WLx6S)), 0.5 NEAR to the demo drainer `Blocked` with `risk_score: 95` (tx [`EfsiaTb…`](https://testnet.nearblocks.io/txns/EfsiaTbkCjWF5iWobteRBWVaa6pFitjTyzy3TQpwXhCh)), 0.5 NEAR to a medium-risk account `Held`, then `Rejected` by the owner (txs [`C2jCMZY…`](https://testnet.nearblocks.io/txns/C2jCMZYYY3s3hKf2FRwwVrTFFgQ7MYBjvHqUJyh76xnk), [`5VGorUA…`](https://testnet.nearblocks.io/txns/5VGorUAMKsx6xgsZtkNpeBfzjEj22RVunbcLcFEJYQx6)), then after the payout-callback upgrade: 0.5 NEAR to `tripwire-ghost-44ea74.testnet`, an account that does not exist → `Reverted` (the transfer failed on chain, the 0.5 NEAR came back to the vault and today's cap was restored; tx [`3974CpF…`](https://testnet.nearblocks.io/txns/3974CpFzvWPYd5bC4cocQ23T1ygMRWENj8EQo7bQDyDK)) and 0.5 NEAR to a clean account → `Paid` only after the transfer succeeded (tx [`A261Yib…`](https://testnet.nearblocks.io/txns/A261YibAzGr3DFwG4SLZmtcJEfwpwy7MAqtZVxnUc9zH)).

**Offline mode:** add `--fixture fixtures/demo-registry.json` to any `check`, or `TRIPWIRE_OFFLINE=1` for the Rhea demo. **Fail-safe:** `npm run -s check -- friend.testnet --registry lst.ref-dev.testnet` (not a registry) gives `⚠️ WARN … Could not read the Tripwire registry` — "couldn't check" is never ALLOW.

### Deploy your own copy (free testnet NEAR)

Install [near-cli-rs](https://github.com/near/near-cli-rs) and [cargo-near](https://github.com/near/cargo-near). Testnet accounts are funded for free by the faucet: `near account create-account sponsor-by-faucet-service <name>.testnet autogenerate-new-keypair save-to-legacy-keychain network-config testnet create` (the scripts below do this for you; a web faucet is also at https://near-faucet.io). Keys go to `~/.near-credentials`, never into this repo.

```bash
./scripts/deploy-registry.sh <suffix>   # registry + reporter accounts, build, deploy, add reporter
./scripts/seed-demo-flags.sh <suffix>   # the labelled demo flags
./scripts/deploy-vault.sh <suffix>      # vault + owner + agent, caps 1 NEAR/payment, 3 NEAR/day
./scripts/vault-demo.sh <suffix>        # agent asks to pay: friend -> paid, drainer -> blocked, medium -> held
export TRIPWIRE_REGISTRY=tripwire-registry-<suffix>.testnet
```

---

## What it is

Tripwire is a wallet-agnostic scam / risk registry and checker for NEAR. Before an AI agent or a wallet sends funds to an account or calls a contract, it asks Tripwire, and gets back **ALLOW**, **WARN** or **BLOCK** with plain-English reasons.

It has three parts, plus an optional AI layer:

- **Registry contract** (Rust, `contract/`): an on-chain list of flagged accounts. Anyone can call `get_risk(account_id)`; trusted reporters file flags with a category, severity, reason, evidence URL and timestamp; the owner manages reporters and moderates.
- **Guard vault** (Rust, `vault/`): a tiny wallet contract for an AI agent. The agent can only call `request_spend`; the vault checks caps and asks the registry before any NEAR leaves.
- **Checker** (TypeScript, `checker/`): a library + CLI that reads the registry over public NEAR RPC, adds local signals (lookalike names, account does not exist), and returns a verdict. `checkTransaction()` checks every account a transaction touches (its receiver plus `receiver_id`/`account_id`-style arguments) before it is signed.
- **NEAR AI second opinion** (optional, `checker/src/nearai.ts`): with `--ai --purpose "<why this payment>"`, NEAR AI Cloud reads the payment's stated purpose for scam patterns (pay-a-fee-to-claim, fake support, "send X get 2X") and explains the verdict in one line. It can only raise ALLOW → WARN, never lower a verdict.

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

### Guard vault (`vault/src/lib.rs`)

The agent's only power is `request_spend(receiver_id, amount, memo)` (give it a function-call key limited to that method, or name its own account as `agent`). Flow:

1. Local checks: caller is the agent, vault not frozen, amount ≤ per-payment cap, today's total ≤ daily cap, enough free balance, payee is not the vault, ≤ 20 payments waiting for the owner. The amount is reserved immediately, so parallel requests cannot slip past the caps.
2. Cross-contract call `get_risk(receiver_id)` on the Tripwire registry.
3. Callback `on_risk` (private): score 0 → pay; 1–69 → **Held** for the owner; ≥ 70 → **Blocked** (cap refunded); registry unreadable → **Held** (fail safe).
4. Every payout (auto-pay above, or owner `approve` of a held request) is `Promise::transfer` chained to a private callback `on_transfer_complete` (15 Tgas reserved). The request shows **Sending** until the transfer's on-chain result is known:
   - transfer succeeded → **Paid** (event `spend_paid`);
   - transfer failed (e.g. the receiver account does not exist) → **Reverted** (event `spend_reverted`): the runtime refunds the NEAR to the vault, the amount is given back to today's cap (only if the request was made today), and `note` says why. `request_spend` / `approve` resolve to `"paid"` or `"reverted"`.

Status values: `Checking`, `Held`, `Sending`, `Paid`, `Reverted`, `Blocked`, `Rejected` (`Approved` only appears on records made before the payout callback existed; owner-approved payouts now end as `Paid`/`Reverted` with "approved by owner" in `note`).

Live demo of a reverted payment: [`3974CpF…`](https://testnet.nearblocks.io/txns/3974CpFzvWPYd5bC4cocQ23T1ygMRWENj8EQo7bQDyDK) (0.5 NEAR to never-created `tripwire-ghost-44ea74.testnet` → `Reverted`, vault balance unchanged, `spent_today` unchanged); clean payment: [`A261Yib…`](https://testnet.nearblocks.io/txns/A261YibAzGr3DFwG4SLZmtcJEfwpwy7MAqtZVxnUc9zH) → `Paid`.

Owner-only: `approve`, `reject`, `set_caps`, `set_agent`, `set_registry`, `freeze`/`unfreeze`, `withdraw` (cannot touch reserved funds). Views: `get_config`, `get_open`, `get_request`, `get_recent`, `free_balance`. Events: NEP-297-style `EVENT_JSON` with `standard: "tripwire-vault"`.

Cheat tests (`vault/src/tests.rs`, `cheat_*`): stranger or owner calling `request_spend`, over per-payment cap, splitting payments past the daily cap, agent faking a "clean" registry answer, replayed callback, agent approving its own held payment, agent raising caps / withdrawing / swapping the registry / unfreezing, frozen vault, approving a request still being checked, spamming held requests, paying the vault itself, making the agent the owner, withdrawing reserved funds, an outsider or the agent calling `on_transfer_complete` to fake a failed payout (and free the cap), a payout callback on a request that was never sent, a replayed payout callback. Payout tests: success → `Paid`; failed transfer → `Reverted` with balance and daily cap restored; a reverted payment does not eat the cap; owner-approved payout that fails → `Reverted`; a failure reported the next day does not refund the new day's cap.

Known limits: the payout callback only learns that the transfer failed, not the runtime's exact error, so the stored reason is generic ("receiver missing or rejected it"). Unit tests mock the transfer result; the real on-chain failure path is shown by the testnet demo tx above. The vault account keeps its deploy key on testnet for redeploys; in production you would remove it so only the owner methods control funds.

### OutLayer adapter (`checker/src/outlayer-approver.ts`)

OutLayer has no external pre-sign hook, but its policies support multisig approvers. Plan: a Tripwire-run NEAR account is one approver in the owner's OutLayer policy. For each pending operation Tripwire checks every account it touches and votes: ALLOW → approve, BLOCK → reject, WARN → no vote (the owner decides).

### What is live vs stubbed

| Piece | Status |
| --- | --- |
| Registry contract on testnet, reporter, 5 demo flags | **Live** (`tripwire-registry-44ea74.testnet`) |
| Checker CLI/library reading the live registry | **Live** |
| Rhea rNEAR example | **Live read** (real SDK, real testnet RPC); transactions are built, never signed |
| Guard vault on testnet calling the registry before paying | **Live** (`tripwire-vault-44ea74.testnet`, real cross-contract calls, real testnet transfers) |
| Payout result tracking (`Sending` → `Paid` / `Reverted`, cap rollback on failure) | **Live** on testnet; failed transfer to a nonexistent account demoed ([`3974CpF…`](https://testnet.nearblocks.io/txns/3974CpFzvWPYd5bC4cocQ23T1ygMRWENj8EQo7bQDyDK)) |
| OutLayer approver decision logic | **Real**, tested, runs against the live registry |
| OutLayer integration (reading real pending approvals, signing + submitting votes) | **Stub / dry run.** Input approvals are simulated; their shape is our assumption from OutLayer's docs; `submitVote()` throws. Not yet tested with a real OutLayer wallet. |
| NEAR AI second opinion (`--ai`): plain-English explanation + scam read of the payment purpose | **Live** (NEAR AI Cloud, OpenAI-compatible API, default model `openai/gpt-5-nano`, override with `NEARAI_MODEL`). Can only raise ALLOW → WARN. Not yet wired into the guard vault on chain. |
| Dashboard | Not built yet |

### Repository layout

```
contract/            Registry contract (Rust) + unit tests (src/tests.rs)
vault/               Guard vault contract (Rust) + unit tests (src/tests.rs)
checker/src/         tripwire.ts (library), cli.ts (CLI), nearai.ts (NEAR AI second opinion), outlayer-approver.ts (adapter, dry run)
checker/examples/    rhea-stake.ts (Rhea rnear SDK), outlayer-approver-dryrun.ts
checker/fixtures/    demo-registry.json, outlayer-pending.sample.json (offline / simulated data, labelled)
checker/test/        checker + NEAR AI layer + adapter unit tests (node:test)
scripts/             deploy-registry.sh, seed-demo-flags.sh, deploy-vault.sh, vault-demo.sh (testnet only)
```

## Libraries, SDKs and partners used

- **near-sdk** 5.30 (Rust) — registry and guard vault contracts (`store::IterableMap/IterableSet`, `ext_contract`, `Promise`, `#[callback_result]`).
- **cargo-near** 0.22 — wasm build (`cargo near build non-reproducible-wasm`, wasm-opt post-step).
- **near-cli-rs** 0.30 — testnet account creation via the faucet service, deploy, calls (scripts only).
- **near-api-js** 7.4 — `JsonRpcProvider` view calls from the checker.
- **Rhea Finance `@rhea-finance/rnear-sdk`** 0.1.1 — `RNearClient({ network: "testnet" }).buildStakeTransaction()` in the Rhea example. Testnet contracts: `lst.ref-dev.testnet` (rNEAR), `ref-finance-101.testnet` (exchange). Build only; the example never signs or sends.
- **TypeScript**, **tsx**, **@types/node** — tooling. Tests use Node's built-in `node:test`.
- **OutLayer** (agent custody) — design target for the approver adapter, from its public docs (https://skills.outlayer.ai/agent-custody/SKILL.md). No OutLayer SDK or API is called yet.
- **NEAR AI Cloud** (`https://cloud-api.near.ai/v1`, OpenAI-compatible chat completions) — optional `--ai` second opinion in `checker/src/nearai.ts`. Called with plain `fetch`, no SDK.
- **NearBlocks** testnet explorer — links only.
- Public NEAR testnet RPC: `https://test.rpc.fastnear.com` with failover to `https://rpc.testnet.near.org`.
- **AI-generated code disclosure:** this code was written with the help of an AI assistant (Hark), reviewed and directed by the author.

All code in this repository was written during NEAR HACKATHON (from Nov 1, 2026).

## Network

Tripwire is **testnet-only**. No mainnet deployment, no real funds.

<a id="demo-data"></a>**Demo data.** Every flag in the testnet registry starts with `DEMO/TEST DATA` and is seeded by `scripts/seed-demo-flags.sh`: `tripwire-drainer-demo-44ea74.testnet` (an account we created to play the drainer), `ref-flnance.testnet`, `lst-ref-dev.testnet`, `rhea-airdrop-claim.testnet` (lookalike / phishing-style names that did not exist on testnet when flagged) and `wrap-near-swap.testnet` (medium risk). They are illustrations, not accusations against real projects. Offline fixtures (`checker/fixtures/`) are labelled the same way.

## License

MIT, see [LICENSE](LICENSE).
