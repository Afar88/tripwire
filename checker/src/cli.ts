#!/usr/bin/env node
/**
 * tripwire check <account_id> [--registry <contract> (default: live testnet registry)] [--rpc <url>] [--fixture <file.json>] [--json]
 *                [--ai] [--purpose "why this payment is being made"]
 *
 * --ai asks NEAR AI Cloud (NEARAI_API_KEY) for a plain-English explanation and a scam read of --purpose.
 * The AI can only raise ALLOW to WARN; it never lowers a verdict. If NEAR AI fails, the base verdict stands.
 *
 * Exit code: 0 = ALLOW, 1 = WARN, 2 = BLOCK, 3 = usage error.
 */
import { readFileSync } from "node:fs";
import { withNearAi, type AiCheckResult } from "./nearai.js";
import { checkAccount, FixtureRegistry, RpcRegistry, DEFAULT_TESTNET_RPCS, DEFAULT_TESTNET_REGISTRY, type RegistrySource } from "./tripwire.js";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

export function registryFromArgs(): RegistrySource {
  const fixture = arg("fixture") ?? process.env.TRIPWIRE_FIXTURE;
  if (fixture) return new FixtureRegistry(JSON.parse(readFileSync(fixture, "utf8")), `fixture ${fixture}`);
  // Default: the live testnet registry. --registry / TRIPWIRE_REGISTRY point elsewhere; --fixture works offline.
  const registryId = arg("registry") ?? process.env.TRIPWIRE_REGISTRY ?? DEFAULT_TESTNET_REGISTRY;
  const rpc = arg("rpc") ?? process.env.NEAR_RPC_URL;
  return new RpcRegistry(registryId, rpc ? [rpc] : DEFAULT_TESTNET_RPCS);
}

const ICON = { ALLOW: "✅", WARN: "⚠️ ", BLOCK: "⛔" } as const;

async function main() {
  const [cmd, accountId] = process.argv.slice(2);
  if (cmd !== "check" || !accountId || accountId.startsWith("--")) {
    console.error("usage: tripwire check <account_id> [--registry <contract>] [--rpc <url>] [--fixture <file>] [--json] [--ai] [--purpose <text>]");
    process.exit(3);
  }
  let res: AiCheckResult = await checkAccount(accountId, registryFromArgs());
  if (process.argv.includes("--ai")) res = await withNearAi(res, arg("purpose"));
  if (process.argv.includes("--json")) {
    console.log(JSON.stringify(res, null, 2));
  } else {
    console.log(`${ICON[res.verdict]} ${res.verdict}  ${res.accountId}  (risk ${res.score}/100, source: ${res.registry})`);
    for (const r of res.reasons) console.log(`   - ${r}`);
    if (res.ai) console.log(`   🤖 NEAR AI (${res.ai.model}): ${res.ai.explanation}`);
    if (res.aiError) console.log(`   🤖 NEAR AI unavailable (${res.aiError}); verdict is from the registry only`);
  }
  process.exit({ ALLOW: 0, WARN: 1, BLOCK: 2 }[res.verdict]);
}

main().catch((e) => {
  console.error(e);
  process.exit(3);
});
