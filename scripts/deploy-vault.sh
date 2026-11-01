#!/usr/bin/env bash
# Deploy a Tripwire guard vault to NEAR TESTNET (faucet accounts only).
#   ./scripts/deploy-vault.sh <suffix>
# Creates tripwire-vault/-owner/-agent-<suffix>.testnet, deploys vault/ with caps
# 1 NEAR per payment, 3 NEAR per day, pointed at tripwire-registry-<suffix>.testnet.
set -euo pipefail
S="${1:?usage: deploy-vault.sh <short-suffix>}"
V="tripwire-vault-$S.testnet"; O="tripwire-owner-$S.testnet"; A="tripwire-agent-$S.testnet"; R="tripwire-registry-$S.testnet"
for X in "$V" "$O" "$A"; do
  near account create-account sponsor-by-faucet-service "$X" autogenerate-new-keypair save-to-legacy-keychain network-config testnet create || true
done
(cd "$(dirname "$0")/../vault" && cargo near build non-reproducible-wasm)
near contract deploy "$V" use-file "$(dirname "$0")/../vault/target/near/tripwire_guard_vault.wasm" \
  with-init-call new json-args "{\"owner_id\":\"$O\",\"agent_id\":\"$A\",\"registry_id\":\"$R\",\"per_tx_cap\":\"1000000000000000000000000\",\"daily_cap\":\"3000000000000000000000000\"}" \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config testnet sign-with-legacy-keychain send
