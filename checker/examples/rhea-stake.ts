/**
 * Rhea example: an agent wants to stake NEAR for rNEAR on testnet.
 *
 * 1. The Rhea rnear SDK builds the stake transaction (build only: nothing is signed or sent).
 * 2. Before the transaction would go to a signer, Tripwire checks every account it touches.
 * 3. We then simulate a prompt-injected agent whose transaction was redirected to a fake
 *    staking contract, and show Tripwire blocking it.
 *
 * Registry: uses the live testnet registry when TRIPWIRE_REGISTRY is set, otherwise the
 * offline demo fixture (fixtures/demo-registry.json).
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { RNearClient } from "@rhea-finance/rnear-sdk";
import { checkTransaction, FixtureRegistry, RpcRegistry, type RegistrySource, type TxLike } from "../src/tripwire.js";

const here = dirname(fileURLToPath(import.meta.url));
const AGENT = process.env.AGENT_ACCOUNT ?? "tripwire-demo-agent.testnet";

function registry(): RegistrySource {
  if (process.env.TRIPWIRE_REGISTRY) return new RpcRegistry(process.env.TRIPWIRE_REGISTRY);
  const file = join(here, "..", "fixtures", "demo-registry.json");
  return new FixtureRegistry(JSON.parse(readFileSync(file, "utf8")), "offline demo fixture");
}

async function guard(label: string, tx: TxLike, reg: RegistrySource) {
  const { verdict, results } = await checkTransaction(tx, reg);
  console.log(`\n${label}`);
  console.log(`  tx: ${tx.signerId ?? AGENT} -> ${tx.receiverId} : ${tx.actions.map((a) => a.params?.methodName).join(", ")}`);
  for (const r of results) {
    console.log(`  ${r.verdict.padEnd(5)} ${r.accountId} (risk ${r.score}/100)`);
    for (const why of r.reasons) console.log(`        - ${why}`);
  }
  const action = verdict === "ALLOW" ? "hand to signer" : verdict === "WARN" ? "hold for owner approval" : "refuse to sign";
  console.log(`  => Tripwire verdict: ${verdict} (${action}). [demo never signs or sends]`);
  return verdict;
}

async function main() {
  const reg = registry();
  console.log(`Tripwire x Rhea rNEAR (testnet). Registry: ${reg.name}`);

  const rnear = new RNearClient({ network: "testnet" });
  const stakeTx = (await rnear.buildStakeTransaction({ accountId: AGENT, nearAmount: "1" })) as unknown as TxLike;

  const ok = await guard("1) Honest stake of 1 NEAR, built by the Rhea rnear SDK:", stakeTx, reg);

  // A poisoned web page told the agent the "new" rNEAR contract is lst-ref-dev.testnet.
  const poisoned: TxLike = { ...stakeTx, receiverId: "lst-ref-dev.testnet" };
  const bad = await guard("2) Same stake after a prompt injection swapped the contract:", poisoned, reg);

  console.log(`\nSummary: honest stake -> ${ok}, poisoned stake -> ${bad}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
