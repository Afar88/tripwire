/**
 * Optional NEAR AI Cloud second opinion for Tripwire.
 *
 * The registry + local rules decide the verdict. NEAR AI only:
 *   1. explains the verdict in one plain-English line for a human, and
 *   2. reads the payment's purpose/memo (e.g. "pay 5 NEAR gas fee to unlock your airdrop")
 *      for scam patterns the registry can't see.
 *
 * Safety rule: the AI can only RAISE caution (ALLOW -> WARN). It can never lower a verdict
 * and never produces a BLOCK on its own. If NEAR AI is unreachable, the base verdict stands.
 *
 * API: OpenAI-compatible, https://cloud-api.near.ai/v1/chat/completions, key in NEARAI_API_KEY.
 */
import type { CheckResult, Verdict } from "./tripwire.js";

export const NEARAI_BASE_URL = "https://cloud-api.near.ai/v1";
export const NEARAI_DEFAULT_MODEL = "openai/gpt-5-nano";
/** Risk score given when the AI raises ALLOW to WARN (inside the vault's owner-approval band 1-69). */
export const AI_WARN_SCORE = 30;

export interface AiOpinion {
  model: string;
  /** true when the AI thinks the payment purpose looks like a scam / social-engineering. */
  suspicious: boolean;
  /** One short line a non-technical user can read. */
  explanation: string;
}

export interface AiCheckResult extends CheckResult {
  ai?: AiOpinion;
  /** Set when NEAR AI could not be used; the base verdict is returned unchanged. */
  aiError?: string;
}

export interface NearAiOptions {
  apiKey?: string;
  model?: string;
  baseUrl?: string;
  fetchImpl?: typeof fetch;
  timeoutMs?: number;
}

const SYSTEM_PROMPT = `You are Tripwire, a safety checker for NEAR payments made by AI agents and wallets.
You get a deterministic verdict (ALLOW/WARN/BLOCK) with its reasons, plus an optional payment purpose written by whoever requested the payment.
Reply with JSON only: {"suspicious": boolean, "explanation": string}.
- "suspicious" is true only if the payment purpose shows clear scam or social-engineering signs (pay a fee to unlock/claim/verify, urgency, impersonating support, "send X get 2X", asking for seed phrases or keys).
- "explanation": one sentence under 30 words, plain English, telling the user what Tripwire decided and why. Never claim an account is safe beyond what the reasons say.`;

export function buildPrompt(result: CheckResult, purpose?: string): string {
  return JSON.stringify({
    account: result.accountId,
    verdict: result.verdict,
    risk_score: result.score,
    reasons: result.reasons,
    payment_purpose: purpose ?? null,
  });
}

/** Pull {"suspicious","explanation"} out of a model reply, tolerating code fences / extra text. */
export function parseOpinion(text: string): { suspicious: boolean; explanation: string } {
  const m = text.match(/\{[\s\S]*\}/);
  if (!m) throw new Error("NEAR AI reply had no JSON");
  const j = JSON.parse(m[0]);
  if (typeof j.suspicious !== "boolean" || typeof j.explanation !== "string") {
    throw new Error("NEAR AI reply missing fields");
  }
  return { suspicious: j.suspicious, explanation: j.explanation.trim() };
}

/** The AI may only raise caution: ALLOW -> WARN when suspicious. WARN/BLOCK never change. */
export function combine(base: Verdict, suspicious: boolean): Verdict {
  return base === "ALLOW" && suspicious ? "WARN" : base;
}

export async function askNearAi(result: CheckResult, purpose: string | undefined, opts: NearAiOptions = {}): Promise<AiOpinion> {
  const apiKey = opts.apiKey ?? process.env.NEARAI_API_KEY;
  if (!apiKey) throw new Error("NEARAI_API_KEY is not set");
  const model = opts.model ?? process.env.NEARAI_MODEL ?? NEARAI_DEFAULT_MODEL;
  const f = opts.fetchImpl ?? fetch;
  const res = await f(`${opts.baseUrl ?? NEARAI_BASE_URL}/chat/completions`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${apiKey}` },
    body: JSON.stringify({
      model,
      messages: [
        { role: "system", content: SYSTEM_PROMPT },
        { role: "user", content: buildPrompt(result, purpose) },
      ],
      max_tokens: 1200,
      reasoning_effort: "minimal",
      response_format: { type: "json_object" },
    }),
    signal: AbortSignal.timeout(opts.timeoutMs ?? 30_000),
  });
  if (!res.ok) throw new Error(`NEAR AI HTTP ${res.status}`);
  const body = (await res.json()) as { choices?: Array<{ message?: { content?: string } }> };
  const text = body.choices?.[0]?.message?.content ?? "";
  return { model, ...parseOpinion(text) };
}

/** Add a NEAR AI opinion to a check. Never throws: on any AI failure the base result is returned with aiError. */
export async function withNearAi(result: CheckResult, purpose?: string, opts: NearAiOptions = {}): Promise<AiCheckResult> {
  try {
    const ai = await askNearAi(result, purpose, opts);
    const verdict = combine(result.verdict, ai.suspicious);
    const reasons = [...result.reasons];
    let score = result.score;
    if (verdict !== result.verdict) {
      score = Math.max(score, AI_WARN_SCORE);
      reasons.push(`NEAR AI (${ai.model}) flagged the payment purpose as a likely scam; raised to WARN`);
    }
    return { ...result, verdict, score, reasons, ai };
  } catch (e) {
    return { ...result, aiError: (e as Error).message };
  }
}
