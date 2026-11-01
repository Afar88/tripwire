#!/usr/bin/env node
/**
 * tripwire check <account_id> [--registry <contract>] [--rpc <url>] [--fixture <file.json>] [--json]
 *
 * Exit code: 0 = ALLOW, 1 = WARN, 2 = BLOCK, 3 = usage error.
 */
import { readFileSync } from "node:fs";
import { checkAccount, FixtureRegistry, RpcRegistry, DEFAULT_TESTNET_RPCS, type RegistrySource } from "./tripwire.js";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

export function registryFromArgs(): RegistrySource {
  const fixture = arg("fixture") ?? process.env.TRIPWIRE_FIXTURE;
  if (fixture) return new FixtureRegistry(JSON.parse(readFileSync(fixture, "utf8")), `fixture ${fixture}`);
  const registryId = arg("registry") ?? process.env.TRIPWIRE_REGISTRY;
  if (!registryId) {
    console.error("No registry: pass --registry <testnet contract> (or TRIPWIRE_REGISTRY), or --fixture <file> for offline mode.");
    process.exit(3);
  }
  const rpc = arg("rpc") ?? process.env.NEAR_RPC_URL;
  return new RpcRegistry(registryId, rpc ? [rpc] : DEFAULT_TESTNET_RPCS);
}

const ICON = { ALLOW: "✅", WARN: "⚠️ ", BLOCK: "⛔" } as const;

async function main() {
  const [cmd, accountId] = process.argv.slice(2);
  if (cmd !== "check" || !accountId || accountId.startsWith("--")) {
    console.error("usage: tripwire check <account_id> [--registry <contract>] [--rpc <url>] [--fixture <file>] [--json]");
    process.exit(3);
  }
  const res = await checkAccount(accountId, registryFromArgs());
  if (process.argv.includes("--json")) {
    console.log(JSON.stringify(res, null, 2));
  } else {
    console.log(`${ICON[res.verdict]} ${res.verdict}  ${res.accountId}  (risk ${res.score}/100, source: ${res.registry})`);
    for (const r of res.reasons) console.log(`   - ${r}`);
  }
  process.exit({ ALLOW: 0, WARN: 1, BLOCK: 2 }[res.verdict]);
}

main().catch((e) => {
  console.error(e);
  process.exit(3);
});
