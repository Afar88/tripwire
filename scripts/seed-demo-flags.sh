#!/usr/bin/env bash
# Seed clearly labelled DEMO/TEST flags into a testnet Tripwire registry.
# These are illustrative accounts for the demo, not accusations against real projects.
#   ./scripts/seed-demo-flags.sh <suffix>
set -euo pipefail
S="${1:?usage: seed-demo-flags.sh <short-suffix>}"
R="tripwire-registry-$S.testnet"; P="tripwire-reporter-$S.testnet"
E="https://github.com/Afar88/tripwire#demo-data"

flag() {
  local args
  args=$(python3 -c 'import json,sys; a,c,s,r,e=sys.argv[1:]; print(json.dumps({"account_id":a,"category":c,"severity":int(s),"reason":r,"evidence_url":e}))' "$@")
  near contract call-function as-transaction "$R" flag json-args "$args" \
    prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "$P" network-config testnet sign-with-legacy-keychain send
}

flag "tripwire-drainer-demo-$S.testnet" drainer 95 "DEMO/TEST DATA: Tripwire's own demo drainer account. A poisoned prompt tells an agent to send funds here." "$E"
flag ref-flnance.testnet lookalike 90 "DEMO/TEST DATA: lookalike of ref-finance (l instead of i). Prompt-injected agents are told to pay it." "$E"
flag lst-ref-dev.testnet lookalike 85 "DEMO/TEST DATA: fake rNEAR staking contract imitating Rhea's lst.ref-dev.testnet." "$E"
flag rhea-airdrop-claim.testnet phishing 80 "DEMO/TEST DATA: fake 'claim your Rhea airdrop' account used in phishing-style demo." "$E"
flag wrap-near-swap.testnet suspicious 40 "DEMO/TEST DATA: unverified swap account, not the official wrap contract. Hold for owner review." "$E"
