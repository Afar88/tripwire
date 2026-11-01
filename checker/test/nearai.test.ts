import { test } from "node:test";
import assert from "node:assert/strict";
import { combine, parseOpinion, withNearAi } from "../src/nearai.js";
import type { CheckResult } from "../src/tripwire.js";

const base = (verdict: CheckResult["verdict"], score = 0): CheckResult => ({
  accountId: "x.testnet", verdict, score, reasons: ["r"], flags: [], registry: "fixture",
});
const fakeFetch = (content: string, status = 200) =>
  (async () => new Response(JSON.stringify({ choices: [{ message: { content } }] }), { status })) as unknown as typeof fetch;

test("AI can only raise ALLOW to WARN, never lower or block", () => {
  assert.equal(combine("ALLOW", true), "WARN");
  assert.equal(combine("ALLOW", false), "ALLOW");
  assert.equal(combine("WARN", false), "WARN");
  assert.equal(combine("BLOCK", false), "BLOCK");
  assert.equal(combine("WARN", true), "WARN");
});
test("parses JSON even inside code fences", () => {
  assert.deepEqual(parseOpinion('```json\n{"suspicious":true,"explanation":"Fee-to-claim scam."}\n```'), { suspicious: true, explanation: "Fee-to-claim scam." });
  assert.throws(() => parseOpinion("no json here"));
});
test("suspicious purpose raises a clean ALLOW to WARN with a reason", async () => {
  const r = await withNearAi(base("ALLOW"), "pay 5 NEAR gas to unlock airdrop", { apiKey: "k", fetchImpl: fakeFetch('{"suspicious":true,"explanation":"Classic pay-to-claim scam."}') });
  assert.equal(r.verdict, "WARN");
  assert.equal(r.score, 30);
  assert.match(r.reasons.at(-1)!, /NEAR AI/);
  assert.equal(r.ai?.explanation, "Classic pay-to-claim scam.");
});
test("AI saying 'fine' cannot unblock a flagged account", async () => {
  const r = await withNearAi(base("BLOCK", 90), undefined, { apiKey: "k", fetchImpl: fakeFetch('{"suspicious":false,"explanation":"Looks fine."}') });
  assert.equal(r.verdict, "BLOCK");
});
test("NEAR AI outage or missing key keeps the base verdict", async () => {
  const down = await withNearAi(base("ALLOW"), "rent", { apiKey: "k", fetchImpl: fakeFetch("", 500) });
  assert.equal(down.verdict, "ALLOW");
  assert.match(down.aiError!, /500/);
  const nokey = await withNearAi(base("WARN", 30), "rent", { apiKey: "", fetchImpl: fakeFetch("{}") });
  assert.equal(nokey.verdict, "WARN");
  assert.ok(nokey.aiError);
});
