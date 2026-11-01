#!/usr/bin/env bash
# Deploy your own Tripwire registry to NEAR TESTNET (free faucet, no real funds).
# Needs: near-cli-rs (`near`) and cargo-near. Keys are written by near-cli to
# ~/.near-credentials (outside this repo). Never commit them.
#
#   ./scripts/deploy-registry.sh <suffix>
# creates tripwire-registry-<suffix>.testnet + tripwire-reporter-<suffix>.testnet,
# deploys contract/ with new(owner_id = registry account), and adds the reporter.
set -euo pipefail
S="${1:?usage: deploy-registry.sh <short-suffix>}"
R="tripwire-registry-$S.testnet"; P="tripwire-reporter-$S.testnet"
NET="network-config testnet"

for A in "$R" "$P"; do
  near account create-account sponsor-by-faucet-service "$A" autogenerate-new-keypair save-to-legacy-keychain $NET create
done

(cd "$(dirname "$0")/../contract" && cargo near build non-reproducible-wasm)
near contract deploy "$R" use-file "$(dirname "$0")/../contract/target/near/tripwire_registry.wasm" \
  with-init-call new json-args "{\"owner_id\":\"$R\"}" prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
  $NET sign-with-legacy-keychain send

near contract call-function as-transaction "$R" add_reporter json-args "{\"account_id\":\"$P\"}" \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "$R" $NET sign-with-legacy-keychain send

echo "Registry: $R  Reporter: $P"
echo "Next: ./scripts/seed-demo-flags.sh $S"
