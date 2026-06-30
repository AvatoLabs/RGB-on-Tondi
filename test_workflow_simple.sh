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
template_terms=("bit""coin" "tap""root" "tap""ret" "op_""return" "w""pkh" "p2""tr" "elect""rum" "bit""coind")
template_pattern="$(IFS='|'; echo "${template_terms[*]}")"
if grep -qiE "$template_pattern" issue_params_template.yaml; then
  echo "issue_params_template.yaml still contains legacy chain wording" >&2
  exit 1
fi

echo "[4/6] Check production workflow script is Tondi-native"
workflow_terms=("rgb/examples" "--w""pkh" "bit""coin\\.testnet" "tap""root" "tap""ret" "op_""return" "elect""rum" "bit""coind")
workflow_pattern="$(IFS='|'; echo "${workflow_terms[*]}")"
if grep -qiE "$workflow_pattern" test_full_workflow.sh; then
  echo "test_full_workflow.sh still contains legacy workflow wiring" >&2
  exit 1
fi

echo "[5/6] Check runtime source has no legacy workflow entry points"
runtime_terms=(
  "new_""bit""coin""_testnet"
  "Consensus::""Bit""coin"
  "Witness""Out"
  "wout""\\("
  "--wout"
  "opret""_hosts"
  "set_""opret""_host"
  "ScriptPubkey::op_""return"
  "tap""ret"
  "Tap""ret"
)
runtime_pattern="$(IFS='|'; echo "${runtime_terms[*]}")"
if rg -n "$runtime_pattern" \
  rgb/src rgb/cli rgb/pstt rgb/descriptors rgb-std/src rgb-std/tests rgb-std/invoice bp-tondi-client; then
  echo "legacy runtime/workflow entry point found" >&2
  exit 1
fi

echo "[6/6] Syntax-check devnet and full workflow scripts"
bash -n scripts/devnet_p2mr_rgb_verify.sh
bash -n test_full_workflow.sh

echo
echo "Workflow smoke check passed."
