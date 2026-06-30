#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "=== RGB-on-Tondi full contract workflow ==="
echo

TONDI_RPC="${TONDI_RPC:-127.0.0.1:16210}"
NETWORK="${NETWORK:-testnet}"
DATA_ROOT="${DATA_ROOT:-/tmp/rgb-on-tondi-full-workflow}"
DATA_DIR_ALICE="${DATA_DIR_ALICE:-$DATA_ROOT/alice}"
DATA_DIR_BOB="${DATA_DIR_BOB:-$DATA_ROOT/bob}"
WALLET_ALICE="${WALLET_ALICE:-alice}"
WALLET_BOB="${WALLET_BOB:-bob}"
ISSUER_FILE="${ISSUER_FILE:-}"
ISSUE_PARAMS="${ISSUE_PARAMS:-$DATA_ROOT/issue_params.yaml}"
CONTRACT_BACKUP="${CONTRACT_BACKUP:-$DATA_ROOT/contract.rgb}"
TRANSFER_CONSIGNMENT="${TRANSFER_CONSIGNMENT:-$DATA_ROOT/transfer.rgb}"
TRANSFER_PSTT="${TRANSFER_PSTT:-$DATA_ROOT/transfer.pstt}"
TRANSFER_TX="${TRANSFER_TX:-$DATA_ROOT/transfer.tx}"
FEE="${FEE:-1000}"
SAT_GIVEAWAY="${SAT_GIVEAWAY:-1000}"
TRANSFER_AMOUNT="${TRANSFER_AMOUNT:-100}"
ISSUED_AMOUNT="${ISSUED_AMOUNT:-1000000}"
TOKEN_TICKER="${TOKEN_TICKER:-TEST}"
TOKEN_NAME="${TOKEN_NAME:-Test Token}"
TOKEN_PRECISION="${TOKEN_PRECISION:-8}"
AUTO_BROADCAST="${AUTO_BROADCAST:-0}"

if [[ -z "${ALICE_XPUB:-}" || -z "${BOB_XPUB:-}" ]]; then
  cat >&2 <<'MSG'
ALICE_XPUB and BOB_XPUB are required.
Create Tondi-compatible extended public keys, then rerun:
  ALICE_XPUB='...' BOB_XPUB='...' TONDI_RPC=127.0.0.1:16210 bash test_full_workflow.sh
MSG
  exit 2
fi

if [[ -z "$ISSUER_FILE" ]]; then
  cat >&2 <<'MSG'
ISSUER_FILE is required and must point to a Tondi-compatible contract issuer file.
Import/export issuer files with the RGB-on-Tondi CLI; this script is issuer-agnostic and drives
the same workflow for fungible, NFT, and other RGB contract schemas whose method/state names are
provided below.
MSG
  exit 2
fi

mkdir -p "$DATA_ROOT"

echo "Config:"
echo "  Tondi RPC:       $TONDI_RPC"
echo "  Network:         $NETWORK"
echo "  Alice data dir:  $DATA_DIR_ALICE"
echo "  Bob data dir:    $DATA_DIR_BOB"
echo "  Issuer:          $ISSUER_FILE"
echo "  Auto broadcast:  $AUTO_BROADCAST"
echo

cargo build --bin rgb

RGB_ALICE=(cargo run --quiet --bin rgb -- --data-dir "$DATA_DIR_ALICE" --network "$NETWORK" --tondi="$TONDI_RPC")
RGB_BOB=(cargo run --quiet --bin rgb -- --data-dir "$DATA_DIR_BOB" --network "$NETWORK" --tondi="$TONDI_RPC")

run_alice_wallet() {
  RGB_WALLET="$WALLET_ALICE" "${RGB_ALICE[@]}" "$@"
}

run_bob_wallet() {
  RGB_WALLET="$WALLET_BOB" "${RGB_BOB[@]}" "$@"
}

echo "[1/16] Initialize RGB stores"
"${RGB_ALICE[@]}" init --quiet || true
"${RGB_BOB[@]}" init --quiet || true

echo "[2/16] Create P2MR wallets"
"${RGB_ALICE[@]}" create "$WALLET_ALICE" "$ALICE_XPUB" || true
"${RGB_BOB[@]}" create "$WALLET_BOB" "$BOB_XPUB" || true

echo "[3/16] Import issuer on both sides"
"${RGB_ALICE[@]}" import "$ISSUER_FILE" || true
"${RGB_BOB[@]}" import "$ISSUER_FILE" || true

echo "[4/16] Generate Tondi P2MR funding addresses"
ALICE_FUND_ADDR="$(run_alice_wallet fund)"
BOB_FUND_ADDR="$(run_bob_wallet fund)"
echo "  Alice: $ALICE_FUND_ADDR"
echo "  Bob:   $BOB_FUND_ADDR"
cat <<MSG

Fund both addresses with Tondi coins before continuing.
Alice needs enough coins for issue anchor/change and transfer fees.
Bob needs at least one P2MR UTXO for invoice authority tokens.
MSG
read -r -p "Press Enter after funding is confirmed..." _

echo "[5/16] Sync wallets"
run_alice_wallet sync
run_bob_wallet sync

echo "[6/16] Select Alice issue seal"
ALICE_SEAL="$(run_alice_wallet seals | awk 'NF {print $1; exit}')"
if [[ -z "$ALICE_SEAL" ]]; then
  echo "Alice has no P2MR UTXO seal; fund $ALICE_FUND_ADDR and sync again" >&2
  exit 1
fi
echo "  Alice issue seal: $ALICE_SEAL"

echo "[7/16] Create issue params"
CODEX_ID="$("${RGB_ALICE[@]}" contracts --issuers | awk 'NF >= 2 && $1 != "Contract" && $1 != "Codex" && $1 != "---" {print $1; exit}')"
if [[ -z "$CODEX_ID" ]]; then
  echo "No issuer codex found after importing $ISSUER_FILE" >&2
  exit 1
fi
cat >"$ISSUE_PARAMS" <<EOF
consensus: tondi-testnet
testnet: true
issuer:
  codexId: $CODEX_ID
name: $TOKEN_TICKER
method: issue
timestamp: "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

global:
  - name: ticker
    verified: $TOKEN_TICKER
  - name: name
    verified: $TOKEN_NAME
  - name: precision
    verified: $TOKEN_PRECISION
  - name: issued
    verified: $ISSUED_AMOUNT

owned:
  - name: balance
    seal: $ALICE_SEAL
    data: $ISSUED_AMOUNT
EOF

echo "[8/16] Issue contract"
ISSUE_OUTPUT="$(run_alice_wallet issue "$ISSUE_PARAMS")"
echo "$ISSUE_OUTPUT"
CONTRACT_ID="$(printf '%s\n' "$ISSUE_OUTPUT" | awk '/A new contract issued with ID/ {print $NF; exit}')"
if [[ -z "$CONTRACT_ID" ]]; then
  echo "Unable to parse issued contract id" >&2
  exit 1
fi

echo "[9/16] Backup and accept contract"
run_alice_wallet backup --force "$CONTRACT_ID" "$CONTRACT_BACKUP"
run_bob_wallet accept --unknown "$CONTRACT_BACKUP" || true

echo "[10/16] Show issuer-side state"
run_alice_wallet state --global --owned "$CONTRACT_ID"

echo "[11/16] Bob creates invoice"
INVOICE="$(run_bob_wallet invoice --nonce 0 "$CONTRACT_ID" balance "$TRANSFER_AMOUNT" | tail -n 1)"
if [[ -z "$INVOICE" ]]; then
  echo "Unable to create invoice" >&2
  exit 1
fi
echo "  $INVOICE"

echo "[12/16] Alice pays invoice, creating PSTT and consignment"
run_alice_wallet pay --strategy aggregate --fee "$FEE" --sats "$SAT_GIVEAWAY" --force "$INVOICE" "$TRANSFER_CONSIGNMENT" "$TRANSFER_PSTT"

echo "[13/16] Bob accepts transfer consignment"
run_bob_wallet accept "$TRANSFER_CONSIGNMENT" || true

echo "[14/16] Alice completes PSTT with RGB bundle proof"
BUNDLE="${TRANSFER_CONSIGNMENT%.*}.bundle"
if [[ -f "$BUNDLE" ]]; then
  run_alice_wallet complete "$BUNDLE" "$TRANSFER_PSTT"
fi

echo "[15/16] Finalize signed PSTT"
if [[ "$AUTO_BROADCAST" == "1" ]]; then
  run_alice_wallet finalize --broadcast "$TRANSFER_PSTT"
else
  run_alice_wallet finalize "$TRANSFER_PSTT" "$TRANSFER_TX"
  echo "  Extracted tx saved to $TRANSFER_TX"
fi

echo "[16/16] Sync and print final state"
run_alice_wallet sync || true
run_bob_wallet sync || true
run_alice_wallet state --global --owned "$CONTRACT_ID" || true
run_bob_wallet state --global --owned "$CONTRACT_ID" || true

echo
echo "Full workflow completed."
