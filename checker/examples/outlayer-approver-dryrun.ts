/**
 * Dry run of the OutLayer approver adapter against the LIVE testnet Tripwire registry.
 * Input approvals are SIMULATED (fixtures/outlayer-pending.sample.json). Nothing is signed or sent.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { RpcRegistry, DEFAULT_TESTNET_REGISTRY } from "../src/tripwire.js";
import { decideApproval, type PendingApproval } from "../src/outlayer-approver.js";

const here = dirname(fileURLToPath(import.meta.url));
const { approvals } = JSON.parse(readFileSync(join(here, "..", "fixtures", "outlayer-pending.sample.json"), "utf8")) as { approvals: PendingApproval[] };
const reg = new RpcRegistry(process.env.TRIPWIRE_REGISTRY ?? DEFAULT_TESTNET_REGISTRY);

console.log(`Tripwire as an OutLayer approver (DRY RUN, simulated approvals). Registry: ${reg.name}`);
for (const a of approvals) {
  const d = await decideApproval(a, reg);
  console.log(`\n${a.approval_id}: ${a.op.type} -> ${a.op.receiver_id}${a.op.args?.receiver_id ? ` (pays ${a.op.args.receiver_id})` : ""}`);
  for (const r of d.results) console.log(`  ${r.verdict.padEnd(5)} ${r.accountId} (risk ${r.score}/100)`);
  console.log(`  => vote: ${d.vote}${d.message ? `  [would sign NEP-413: "${d.message}"]` : "  [no vote: owner decides]"}`);
}
console.log("\n(dry run: no vote was signed or submitted to OutLayer)");
