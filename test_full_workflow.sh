#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "=== RGB-on-Tondi full contract workflow ==="
echo

TONDI_RPC="${TONDI_RPC:-127.0.0.1:16310}"
NETWORK="${NETWORK:-testnet2}"
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
FEE="${FEE:-10000}"
SAT_GIVEAWAY="${SAT_GIVEAWAY:-1000}"
TRANSFER_AMOUNT="${TRANSFER_AMOUNT:-100}"
ISSUED_AMOUNT="${ISSUED_AMOUNT:-1000000}"
TOKEN_TICKER="${TOKEN_TICKER:-TEST}"
TOKEN_NAME="${TOKEN_NAME:-Test Token}"
TOKEN_PRECISION="${TOKEN_PRECISION:-8}"
API_NAME="${API_NAME:-Test}"
STATE_NAME="${STATE_NAME:-amount}"
TRANSFER_METHOD="${TRANSFER_METHOD:-transfer}"
AUTO_BROADCAST="${AUTO_BROADCAST:-0}"
REQUIRE_BROADCAST_ACCEPTED="${REQUIRE_BROADCAST_ACCEPTED:-0}"
BROADCAST_STATUS_ATTEMPTS="${BROADCAST_STATUS_ATTEMPTS:-30}"
BROADCAST_STATUS_POLL_SECONDS="${BROADCAST_STATUS_POLL_SECONDS:-2}"
MIN_CONFIRMATIONS="${MIN_CONFIRMATIONS:-32}"
AUTO_DEV_KEYS="${AUTO_DEV_KEYS:-1}"
ALICE_DEV_SEED="${ALICE_DEV_SEED:-0001020304050607080900010203040506070809000102030405060708090001}"
BOB_DEV_SEED="${BOB_DEV_SEED:-1001020304050607080900010203040506070809000102030405060708090010}"
WAIT_FOR_FUNDING="${WAIT_FOR_FUNDING:-0}"
FUNDING_TIMEOUT_SECONDS="${FUNDING_TIMEOUT_SECONDS:-300}"
FUNDING_POLL_SECONDS="${FUNDING_POLL_SECONDS:-5}"
NON_INTERACTIVE="${NON_INTERACTIVE:-0}"

dev_key_field() {
  local output="$1"
  local field="$2"
  printf '%s\n' "$output" | awk -F= -v field="$field" '$1 == field { print $2; exit }'
}

if [[ "$AUTO_DEV_KEYS" == "1" && ( -z "${ALICE_XPUB:-}" || -z "${BOB_XPUB:-}" || -z "${ALICE_P2MR_SECRET_KEY:-}" ) ]]; then
  echo "Generating deterministic devnet wallet keys (AUTO_DEV_KEYS=1)."
  echo "Do not use these keys outside isolated devnet testing."
  ALICE_DEV_KEYS="$(cargo run --quiet --bin rgb -- dev-keys --seed-hex "$ALICE_DEV_SEED")"
  BOB_DEV_KEYS="$(cargo run --quiet --bin rgb -- dev-keys --seed-hex "$BOB_DEV_SEED")"
  ALICE_XPUB="${ALICE_XPUB:-$(dev_key_field "$ALICE_DEV_KEYS" xpub)}"
  BOB_XPUB="${BOB_XPUB:-$(dev_key_field "$BOB_DEV_KEYS" xpub)}"
  ALICE_P2MR_SECRET_KEY="${ALICE_P2MR_SECRET_KEY:-$(dev_key_field "$ALICE_DEV_KEYS" outer0_tondi_p2mr_secret_key)}"
fi

if [[ -z "${ALICE_XPUB:-}" || -z "${BOB_XPUB:-}" ]]; then
  cat >&2 <<'MSG'
ALICE_XPUB and BOB_XPUB are required.
Create Tondi-compatible extended public keys, or leave AUTO_DEV_KEYS=1 for deterministic devnet keys, then rerun:
  ALICE_XPUB='...' BOB_XPUB='...' TONDI_RPC=127.0.0.1:16310 NETWORK=testnet2 bash test_full_workflow.sh
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

if [[ "$AUTO_BROADCAST" == "1" && -z "${ALICE_P2MR_SECRET_KEY:-}" ]]; then
  cat >&2 <<'MSG'
ALICE_P2MR_SECRET_KEY is required when AUTO_BROADCAST=1.
Generate deterministic development key material with:
  cargo run --bin rgb -- dev-keys --seed-hex <64-byte-hex-seed>
Then pass outer0_tondi_p2mr_secret_key as ALICE_P2MR_SECRET_KEY.
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
echo "  Broadcast final: $REQUIRE_BROADCAST_ACCEPTED"
echo "  Auto dev keys:   $AUTO_DEV_KEYS"
echo "  Confirmations:   $MIN_CONFIRMATIONS"
echo

cargo build --bin rgb

RGB_ALICE=(cargo run --quiet --bin rgb -- --data-dir "$DATA_DIR_ALICE" --network "$NETWORK" --min-confirmations "$MIN_CONFIRMATIONS" --tondi="$TONDI_RPC")
RGB_BOB=(cargo run --quiet --bin rgb -- --data-dir "$DATA_DIR_BOB" --network "$NETWORK" --min-confirmations "$MIN_CONFIRMATIONS" --tondi="$TONDI_RPC")

run_alice_wallet() {
  RGB_WALLET="$WALLET_ALICE" "${RGB_ALICE[@]}" "$@"
}

run_bob_wallet() {
  RGB_WALLET="$WALLET_BOB" "${RGB_BOB[@]}" "$@"
}

select_first_seal() {
  awk '
    NF && $1 !~ /^0{14}/ { print $1; found = 1; exit }
    NF && fallback == "" { fallback = $1 }
    END { if (!found && fallback != "") print fallback }
  '
}

wait_for_alice_seal() {
  local deadline=$(( $(date +%s) + FUNDING_TIMEOUT_SECONDS ))
  while (( $(date +%s) <= deadline )); do
    run_alice_wallet sync >/dev/null 2>&1 || true
    if run_alice_wallet seals 2>/dev/null | select_first_seal | grep -q .; then
      return 0
    fi
    sleep "$FUNDING_POLL_SECONDS"
  done
  return 1
}

wait_for_bob_seal() {
  local deadline=$(( $(date +%s) + FUNDING_TIMEOUT_SECONDS ))
  while (( $(date +%s) <= deadline )); do
    run_bob_wallet sync >/dev/null 2>&1 || true
    if run_bob_wallet seals 2>/dev/null | select_first_seal | grep -q .; then
      return 0
    fi
    sleep "$FUNDING_POLL_SECONDS"
  done
  return 1
}

echo "[1/16] Initialize RGB stores"
"${RGB_ALICE[@]}" init --quiet >/dev/null 2>&1 || true
"${RGB_BOB[@]}" init --quiet >/dev/null 2>&1 || true

echo "[2/16] Create P2MR wallets"
"${RGB_ALICE[@]}" create "$WALLET_ALICE" "$ALICE_XPUB" >/dev/null 2>&1 || true
"${RGB_BOB[@]}" create "$WALLET_BOB" "$BOB_XPUB" >/dev/null 2>&1 || true

echo "[3/16] Import issuer on both sides"
"${RGB_ALICE[@]}" import "$ISSUER_FILE" || true
"${RGB_BOB[@]}" import "$ISSUER_FILE" || true

echo "[4/16] Generate Tondi P2MR funding addresses"
ALICE_FUND_ADDR="${ALICE_FUND_ADDR:-$(run_alice_wallet fund)}"
BOB_FUND_ADDR="${BOB_FUND_ADDR:-$(run_bob_wallet fund)}"
echo "  Alice: $ALICE_FUND_ADDR"
echo "  Bob:   $BOB_FUND_ADDR"
cat <<MSG

Fund both addresses with Tondi coins before continuing.
Alice needs enough coins for issue anchor/change and transfer fees.
Bob needs at least one P2MR UTXO for invoice authority tokens.
MSG
if [[ "$WAIT_FOR_FUNDING" == "1" ]]; then
  echo "Waiting for Alice and Bob P2MR funding UTXOs..."
  wait_for_alice_seal || {
    echo "Timed out waiting for Alice P2MR funding at $ALICE_FUND_ADDR" >&2
    exit 1
  }
  wait_for_bob_seal || {
    echo "Timed out waiting for Bob P2MR funding at $BOB_FUND_ADDR" >&2
    exit 1
  }
elif [[ "$NON_INTERACTIVE" == "1" ]]; then
  echo "NON_INTERACTIVE=1 and WAIT_FOR_FUNDING!=1; fund the addresses above and rerun." >&2
  exit 2
else
  read -r -p "Press Enter after funding is confirmed..." _
fi

echo "[5/16] Sync wallets"
run_alice_wallet sync
run_bob_wallet sync

echo "[6/16] Select Alice issue seal"
ALICE_SEAL="${ALICE_SEAL:-}"
if [[ -z "$ALICE_SEAL" ]]; then
  ALICE_SEAL="$(run_alice_wallet seals | awk '
    NF && $1 !~ /^0{14}/ { print $1; found = 1; exit }
    NF && fallback == "" { fallback = $1 }
    END { if (!found && fallback != "") print fallback }
  ')"
fi
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
consensus: tondiTestnet
testnet: true
issuer: $CODEX_ID
name: $TOKEN_TICKER
method: issue
timestamp: "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

global: []

owned:
  - name: $STATE_NAME
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
INVOICE="$(run_bob_wallet invoice --nonce 0 --api "$API_NAME" --method "$TRANSFER_METHOD" --state "$STATE_NAME" "$CONTRACT_ID" "$TRANSFER_AMOUNT" | tail -n 1)"
if [[ -z "$INVOICE" ]]; then
  echo "Unable to create invoice" >&2
  exit 1
fi
echo "  $INVOICE"

echo "[12/16] Alice pays invoice, creating PSTT and consignment"
run_alice_wallet pay --strategy aggregate --fee "$FEE" --sats "$SAT_GIVEAWAY" --force "$INVOICE" "$TRANSFER_CONSIGNMENT" "$TRANSFER_PSTT"

echo "[13/16] Bob accepts transfer consignment"
run_bob_wallet accept "$TRANSFER_CONSIGNMENT" || true

echo "[14/16] Alice keeps the completed PSTT produced by pay"

echo "[15/16] Finalize signed PSTT"
FINALIZE_ARGS=(finalize)
if [[ -n "${ALICE_P2MR_SECRET_KEY:-}" ]]; then
  FINALIZE_ARGS+=(--tondi-p2mr-secret-key "$ALICE_P2MR_SECRET_KEY")
fi
if [[ "$AUTO_BROADCAST" == "1" ]]; then
  FINALIZE_OUTPUT="$(run_alice_wallet "${FINALIZE_ARGS[@]}" --broadcast "$TRANSFER_PSTT" 2>&1)"
  printf '%s\n' "$FINALIZE_OUTPUT"
  BROADCAST_TXID="$(printf '%s\n' "$FINALIZE_OUTPUT" | awk '/Broadcast transaction ID:/ { print $NF; exit }')"
  if [[ -z "$BROADCAST_TXID" ]]; then
    echo "Broadcast succeeded without a parsable transaction id" >&2
    exit 1
  fi

  echo "  Broadcast txid: $BROADCAST_TXID"
  echo "  Waiting for transaction status via GetTransactionStatus..."
  BROADCAST_STATUS=""
  for ((attempt = 1; attempt <= BROADCAST_STATUS_ATTEMPTS; attempt++)); do
    STATUS_OUTPUT="$(TONDI_RPC="$TONDI_RPC" TONDI_TXID="$BROADCAST_TXID" cargo run --quiet -p bp-tondi --features grpc --example query_status 2>&1 || true)"
    BROADCAST_STATUS="$(printf '%s\n' "$STATUS_OUTPUT" | awk -F= '$1 == "status" { print $2; exit }')"
    if [[ "$BROADCAST_STATUS" == "accepted" ]]; then
      printf '%s\n' "$STATUS_OUTPUT"
      break
    fi
    if [[ "$REQUIRE_BROADCAST_ACCEPTED" != "1" && "$BROADCAST_STATUS" == "mempool" ]]; then
      printf '%s\n' "$STATUS_OUTPUT"
      break
    fi
    sleep "$BROADCAST_STATUS_POLL_SECONDS"
  done
  if [[ "$REQUIRE_BROADCAST_ACCEPTED" == "1" && "$BROADCAST_STATUS" != "accepted" ]]; then
    echo "Broadcast transaction did not reach accepted state:" >&2
    printf '%s\n' "$STATUS_OUTPUT" >&2
    exit 1
  fi
  if [[ "$REQUIRE_BROADCAST_ACCEPTED" != "1" && "$BROADCAST_STATUS" != "mempool" && "$BROADCAST_STATUS" != "accepted" ]]; then
    echo "Broadcast transaction did not reach mempool or accepted state:" >&2
    printf '%s\n' "$STATUS_OUTPUT" >&2
    exit 1
  fi
else
  run_alice_wallet "${FINALIZE_ARGS[@]}" "$TRANSFER_PSTT" "$TRANSFER_TX"
  echo "  Extracted tx saved to $TRANSFER_TX"
fi

echo "[16/16] Sync and print final state"
run_alice_wallet sync || true
run_bob_wallet sync || true
run_alice_wallet state --global --owned "$CONTRACT_ID" || true
run_bob_wallet state --global --owned "$CONTRACT_ID" || true

echo
echo "Full workflow completed."
