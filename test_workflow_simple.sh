#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "=== RGB-on-Tondi workflow smoke check ==="
echo

TONDI_RPC="${TONDI_RPC:-127.0.0.1:16210}"
NETWORK="${NETWORK:-testnet}"
WORKDIR="${WORKDIR:-/tmp/rgb-on-tondi-workflow-smoke}"

echo "Config:"
echo "  Tondi RPC: $TONDI_RPC"
echo "  Network:   $NETWORK"
echo "  Workdir:   $WORKDIR"
echo

echo "[1/6] Build Tondi-native RGB CLI"
cargo build --bin rgb

RGB=(cargo run --quiet --bin rgb -- --data-dir "$WORKDIR" --network "$NETWORK" --tondi="$TONDI_RPC")

echo "[2/6] Check command surface"
"${RGB[@]}" --help >/tmp/rgb-on-tondi-cli-help.txt
for cmd in init create fund sync seals import export issue backup state invoice pay script exec complete consign finalize accept; do
  grep -q "$cmd" /tmp/rgb-on-tondi-cli-help.txt
done

echo "[3/6] Check issue template is Tondi-native"
grep -q "consensus: tondi-testnet" issue_params_template.yaml
grep -q "seal: <REPLACE_WITH_TONDI_OUTPOINT>" issue_params_template.yaml
if grep -qiE "bitcoin|taproot|tapret|op_return|wpkh|p2tr|electrum|bitcoind" issue_params_template.yaml; then
  echo "issue_params_template.yaml still contains legacy chain wording" >&2
  exit 1
fi

echo "[4/6] Check production workflow script is Tondi-native"
if grep -qiE "rgb/examples|--wpkh|bitcoin\\.testnet|taproot|tapret|op_return|electrum|bitcoind" test_full_workflow.sh; then
  echo "test_full_workflow.sh still contains legacy workflow wiring" >&2
  exit 1
fi

echo "[5/6] Check runtime source has no legacy workflow entry points"
if rg -n "new_bitcoin_testnet|Consensus::Bitcoin|WitnessOut|wout\\(|--wout|opret_hosts|set_opret_host|ScriptPubkey::op_return|tapret|Tapret" \
  rgb/src rgb/cli rgb/pstt rgb/descriptors rgb-std/src rgb-std/tests rgb-std/invoice bp-tondi-client; then
  echo "legacy runtime/workflow entry point found" >&2
  exit 1
fi

echo "[6/6] Syntax-check devnet and full workflow scripts"
bash -n scripts/devnet_p2mr_rgb_verify.sh
bash -n test_full_workflow.sh

echo
echo "Workflow smoke check passed."
