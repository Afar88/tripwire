import { test } from "node:test";
import assert from "node:assert/strict";
import { FixtureRegistry } from "../src/tripwire.js";
import { decide, decideApproval, opToTx, submitVote, voteMessage, type PendingApproval } from "../src/outlayer-approver.js";

const reg = new FixtureRegistry({
  "drainer.testnet": { score: 95, flags: [{ reporter: "r.testnet", category: "drainer", severity: 95, reason: "x", evidence_url: "https://e", timestamp_ms: 1 }] },
  "sus.testnet": { score: 40, flags: [{ reporter: "r.testnet", category: "suspicious", severity: 40, reason: "y", evidence_url: "https://e", timestamp_ms: 1 }] },
});
const ap = (receiver_id: string, extra: Partial<PendingApproval["op"]> = {}): PendingApproval => ({
  approval_id: "a1", wallet_pubkey: "ed25519:PK", request_hash: "h1", op: { type: "transfer", receiver_id, ...extra },
});

test("verdict -> vote mapping", () => {
  assert.equal(decide("ALLOW"), "approve");
  assert.equal(decide("WARN"), "leave_for_owner");
  assert.equal(decide("BLOCK"), "reject");
});
test("clean payee is approved with the NEP-413 vote message", async () => {
  const d = await decideApproval(ap("friend.testnet"), reg);
  assert.equal(d.vote, "approve");
  assert.equal(d.message, "approve:a1:ed25519:PK:h1");
});
test("flagged payee is rejected", async () => {
  assert.equal((await decideApproval(ap("drainer.testnet"), reg)).vote, "reject");
});
test("medium risk is left for the owner (no vote)", async () => {
  const d = await decideApproval(ap("sus.testnet"), reg);
  assert.equal(d.vote, "leave_for_owner");
  assert.equal(d.message, null);
});
test("a token transfer hiding a flagged payee in args is rejected", async () => {
  const d = await decideApproval(ap("wrap.testnet", { type: "call", method: "ft_transfer", args: { receiver_id: "drainer.testnet" } }), reg);
  assert.equal(d.vote, "reject");
  assert.deepEqual(opToTx(ap("wrap.testnet").op).receiverId, "wrap.testnet");
});
test("submitVote is an honest stub", async () => {
  await assert.rejects(submitVote("x", ap("friend.testnet"), "approve"), /stub/);
  assert.equal(voteMessage("reject", ap("x.testnet")), "reject:a1:ed25519:PK:h1");
});
