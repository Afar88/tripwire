/**
 * Tripwire checker: asks the on-chain Tripwire registry (and a few local signals)
 * whether an account is safe to pay or call, and returns ALLOW / WARN / BLOCK.
 *
 * Read-only: it never signs or sends anything.
 */
import { JsonRpcProvider } from "near-api-js";

export type Verdict = "ALLOW" | "WARN" | "BLOCK";

export interface Flag {
  reporter: string;
  category: string;
  severity: number;
  reason: string;
  evidence_url: string;
  timestamp_ms: number;
}

export interface RiskReport {
  account_id: string;
  score: number;
  level: "none" | "warn" | "block";
  flags: Flag[];
}

export interface CheckResult {
  accountId: string;
  verdict: Verdict;
  score: number;
  reasons: string[];
  flags: Flag[];
  registry: string;
}

/** Anything that can answer get_risk: the live contract, or a fixture for offline demos. */
export interface RegistrySource {
  name: string;
  getRisk(accountId: string): Promise<RiskReport>;
  /** Returns false when the account does not exist on chain; null when unknown (offline). */
  accountExists(accountId: string): Promise<boolean | null>;
}

export const BLOCK_AT = 70;
export const WARN_AT = 1;

export const DEFAULT_TESTNET_RPCS = [
  "https://test.rpc.fastnear.com",
  "https://rpc.testnet.near.org",
];

/** Live registry over public NEAR testnet JSON-RPC (view calls only, free, no key). */
export class RpcRegistry implements RegistrySource {
  readonly name: string;
  private providers: JsonRpcProvider[];

  constructor(private registryId: string, rpcUrls: string[] = DEFAULT_TESTNET_RPCS) {
    this.name = `${registryId} via ${rpcUrls[0]}`;
    this.providers = rpcUrls.map((url) => new JsonRpcProvider({ url }));
  }

  private async withFailover<T>(fn: (p: JsonRpcProvider) => Promise<T>): Promise<T> {
    let last: unknown;
    for (const p of this.providers) {
      try {
        return await fn(p);
      } catch (e) {
        last = e;
        // a contract/account error is an answer, not an outage: don't try the next RPC
        if (isAccountMissing(e)) throw e;
      }
    }
    throw last;
  }

  async getRisk(accountId: string): Promise<RiskReport> {
    const r = await this.withFailover((p) =>
      p.callFunction<RiskReport>({
        contractId: this.registryId,
        method: "get_risk",
        args: { account_id: accountId },
      }),
    );
    if (!r || typeof (r as RiskReport).score !== "number") {
      throw new Error("Registry returned an unexpected response");
    }
    return r as RiskReport;
  }

  async accountExists(accountId: string): Promise<boolean | null> {
    try {
      await this.withFailover((p) => p.viewAccount({ accountId }));
      return true;
    } catch (e) {
      if (isAccountMissing(e)) return false;
      return null;
    }
  }
}

function isAccountMissing(e: unknown): boolean {
  const msg = String((e as Error)?.message ?? e);
  return /does not exist|UNKNOWN_ACCOUNT|AccountDoesNotExist/i.test(msg);
}

/** Offline registry backed by a JSON object { "<account>": RiskReport-like }. */
export class FixtureRegistry implements RegistrySource {
  readonly name: string;
  constructor(private data: Record<string, Partial<RiskReport> & { exists?: boolean }>, label = "fixture") {
    this.name = label;
  }
  async getRisk(accountId: string): Promise<RiskReport> {
    const d = this.data[accountId];
    const flags = d?.flags ?? [];
    return { account_id: accountId, score: d?.score ?? 0, level: d?.level ?? "none", flags };
  }
  async accountExists(accountId: string): Promise<boolean | null> {
    const d = this.data[accountId];
    return d?.exists ?? true;
  }
}

/** Well-known NEAR names that scammers imitate. Extend as needed. */
export const KNOWN_NAMES = [
  "ref-finance.near",
  "v2.ref-finance.near",
  "rhealab.near",
  "lst.rhealab.near",
  "wrap.near",
  "intents.near",
  "aurora",
  "meta-pool.near",
  "usdt.tether-token.near",
  "ref-finance-101.testnet",
  "lst.ref-dev.testnet",
  "wrap.testnet",
];

const HOMOGLYPHS: Record<string, string> = { "0": "o", "1": "l", i: "l", "5": "s", rn: "m", vv: "w" };

function normalise(name: string): string {
  let s = name.toLowerCase();
  for (const [from, to] of Object.entries(HOMOGLYPHS)) s = s.split(from).join(to);
  return s;
}

function editDistance(a: string, b: string): number {
  const dp = Array.from({ length: a.length + 1 }, (_, i) => [i, ...Array(b.length).fill(0)]);
  for (let j = 1; j <= b.length; j++) dp[0][j] = j;
  for (let i = 1; i <= a.length; i++)
    for (let j = 1; j <= b.length; j++)
      dp[i][j] = Math.min(
        dp[i - 1][j] + 1,
        dp[i][j - 1] + 1,
        dp[i - 1][j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
  return dp[a.length][b.length];
}

/** Returns the known name this account imitates, or null. Exact matches are not lookalikes. */
export function lookalikeOf(accountId: string, known: string[] = KNOWN_NAMES): string | null {
  const id = accountId.toLowerCase();
  if (known.includes(id)) return null;
  for (const k of known) {
    if (normalise(id) === normalise(k)) return k;
    if (Math.abs(id.length - k.length) <= 2 && editDistance(id, k) <= 2 && id.split(".").pop() === k.split(".").pop())
      return k;
  }
  return null;
}

export function verdictFor(score: number): Verdict {
  if (score >= BLOCK_AT) return "BLOCK";
  if (score >= WARN_AT) return "WARN";
  return "ALLOW";
}

/**
 * Check one account. Fail-safe: if the registry cannot be reached the verdict is WARN,
 * never a silent ALLOW.
 */
export async function checkAccount(accountId: string, registry: RegistrySource): Promise<CheckResult> {
  const reasons: string[] = [];
  let score = 0;
  let flags: Flag[] = [];

  try {
    const risk = await registry.getRisk(accountId);
    flags = risk.flags;
    score = Math.max(score, risk.score);
    for (const f of risk.flags) {
      reasons.push(`Flagged "${f.category}" (severity ${f.severity}) by ${f.reporter}: ${f.reason} [${f.evidence_url}]`);
    }
  } catch (e) {
    score = Math.max(score, 50);
    reasons.push(`Could not read the Tripwire registry (${(e as Error).message}); treat as unverified`);
  }

  const look = lookalikeOf(accountId);
  if (look) {
    score = Math.max(score, 60);
    reasons.push(`Name looks like "${look}" but is not it (possible impersonation)`);
  }

  const exists = await registry.accountExists(accountId);
  if (exists === false) {
    score = Math.max(score, 40);
    reasons.push("Account does not exist on chain (typo or not yet created)");
  }

  if (reasons.length === 0) reasons.push("No flags in the Tripwire registry and no local warning signs");
  return { accountId, verdict: verdictFor(score), score, reasons, flags, registry: registry.name };
}

/** Minimal NEAR transaction shape (matches what wallet SDKs such as rnear-sdk build). */
export interface TxLike {
  signerId?: string;
  receiverId: string;
  actions: Array<{ type: string; params?: { methodName?: string; args?: unknown } }>;
}

/** Every account a transaction would pay or call: the receiver plus receiver_id-style args. */
export function accountsTouched(tx: TxLike): string[] {
  const out = new Set<string>([tx.receiverId]);
  for (const a of tx.actions) {
    const args = a.params?.args as Record<string, unknown> | undefined;
    if (args && typeof args === "object") {
      for (const key of ["receiver_id", "account_id", "beneficiary_id", "recipient"]) {
        const v = args[key];
        if (typeof v === "string") out.add(v);
      }
    }
  }
  return [...out];
}

/** Check every account a transaction touches; the worst verdict wins. */
export async function checkTransaction(tx: TxLike, registry: RegistrySource) {
  const results = await Promise.all(accountsTouched(tx).map((id) => checkAccount(id, registry)));
  const rank: Record<Verdict, number> = { ALLOW: 0, WARN: 1, BLOCK: 2 };
  const verdict = results.reduce<Verdict>((w, r) => (rank[r.verdict] > rank[w] ? r.verdict : w), "ALLOW");
  return { verdict, results };
}
