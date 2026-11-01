#!/usr/bin/env bash
# Live guard-vault demo on NEAR TESTNET. Needs near-cli-rs and the agent/owner keys in
# ~/.near-credentials (see scripts/deploy-vault.sh). Each request moves 0.5 testnet NEAR at most.
#   ./scripts/vault-demo.sh <suffix>
set -euo pipefail
S="${1:-44ea74}"
V="tripwire-vault-$S.testnet"; A="tripwire-agent-$S.testnet"; O="tripwire-owner-$S.testnet"
NET="network-config testnet sign-with-legacy-keychain send"
spend() { # receiver memo
  echo "== agent asks: pay 0.5 NEAR to $1 ($2)"
  near contract call-function as-transaction "$V" request_spend \
    json-args "{\"receiver_id\":\"$1\",\"amount\":\"500000000000000000000000\",\"memo\":\"$2\"}" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' sign-as "$A" $NET
}
spend "tripwire-reporter-$S.testnet" "pay a known friend"            # -> "paid"
spend "tripwire-drainer-demo-$S.testnet" "poisoned prompt: drainer"  # -> "blocked"
spend "wrap-near-swap.testnet" "medium-risk swap"                    # -> "held"
near contract call-function as-read-only "$V" get_open json-args '{}' network-config testnet now
echo "Owner decides on held requests with: approve / reject {\"request_id\":N} signed by $O"
