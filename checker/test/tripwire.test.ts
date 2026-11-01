import { test } from "node:test";
import assert from "node:assert/strict";
import { checkAccount, checkTransaction, FixtureRegistry, lookalikeOf, verdictFor, type RegistrySource } from "../src/tripwire.js";

const reg = new FixtureRegistry({
  "drainer.testnet": { score: 90, flags: [{ reporter: "r.testnet", category: "drainer", severity: 90, reason: "x", evidence_url: "https://e", timestamp_ms: 1 }] },
  "sus.testnet": { score: 30, flags: [{ reporter: "r.testnet", category: "spam", severity: 30, reason: "y", evidence_url: "https://e", timestamp_ms: 1 }] },
  "ghost.testnet": { exists: false },
});

test("clean account is ALLOW", async () => {
  assert.equal((await checkAccount("friend.testnet", reg)).verdict, "ALLOW");
});
test("flagged high is BLOCK, low is WARN", async () => {
  assert.equal((await checkAccount("drainer.testnet", reg)).verdict, "BLOCK");
  assert.equal((await checkAccount("sus.testnet", reg)).verdict, "WARN");
});
test("missing account is WARN", async () => {
  assert.equal((await checkAccount("ghost.testnet", reg)).verdict, "WARN");
});
test("lookalike names are caught, real names are not", () => {
  assert.equal(lookalikeOf("ref-flnance.near"), "ref-finance.near");
  assert.equal(lookalikeOf("ref-finance.near"), null);
  assert.equal(lookalikeOf("lst-ref-dev.testnet"), "lst.ref-dev.testnet");
  assert.equal(lookalikeOf("alice.testnet"), null);
});
test("unreachable registry fails safe (WARN, never silent ALLOW)", async () => {
  const down: RegistrySource = { name: "down", getRisk: async () => { throw new Error("timeout"); }, accountExists: async () => null };
  assert.equal((await checkAccount("friend.testnet", down)).verdict, "WARN");
});
test("transaction check covers receiver_id inside args; worst verdict wins", async () => {
  const tx = { receiverId: "token.testnet", actions: [{ type: "FunctionCall", params: { methodName: "ft_transfer", args: { receiver_id: "drainer.testnet", amount: "1" } } }] };
  const r = await checkTransaction(tx, reg);
  assert.equal(r.verdict, "BLOCK");
  assert.deepEqual(r.results.map((x) => x.accountId), ["token.testnet", "drainer.testnet"]);
});
test("thresholds", () => {
  assert.equal(verdictFor(0), "ALLOW");
  assert.equal(verdictFor(69), "WARN");
  assert.equal(verdictFor(70), "BLOCK");
});
