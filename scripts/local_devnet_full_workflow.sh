#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TONDI_REPO="${TONDI_REPO:-"$ROOT/../Tondi"}"
DATA_ROOT="${DATA_ROOT:-/tmp/rgb-on-tondi-local-devnet}"
NETWORK="${NETWORK:-devnet}"
WALLET_ALICE="${WALLET_ALICE:-alice}"
WALLET_BOB="${WALLET_BOB:-bob}"
TONDI_RPC_HOST="${TONDI_RPC_HOST:-127.0.0.1}"
TONDI_RPC_PORT="${TONDI_RPC_PORT:-16610}"
TONDI_WRPC_PORT="${TONDI_WRPC_PORT:-17610}"
TONDI_P2P_PORT="${TONDI_P2P_PORT:-16611}"
TONDI_RPC="${TONDI_RPC:-$TONDI_RPC_HOST:$TONDI_RPC_PORT}"
TONDI_WRPC="${TONDI_WRPC:-ws://$TONDI_RPC_HOST:$TONDI_WRPC_PORT}"
TONDID_APPDIR="${TONDID_APPDIR:-$DATA_ROOT/tondid}"
TONDID_LOG="${TONDID_LOG:-$DATA_ROOT/tondid.log}"
MINER_LOG="${MINER_LOG:-$DATA_ROOT/devnet-miner.log}"
ISSUER_FILE="${ISSUER_FILE:-$DATA_ROOT/dev.issuer}"
PREALLOC_UTXOS="${PREALLOC_UTXOS:-4}"
PREALLOC_AMOUNT="${PREALLOC_AMOUNT:-10000000000}"
FUNDING_AMOUNT="${FUNDING_AMOUNT:-4000000000}"
MINER_THREADS="${MINER_THREADS:-2}"
MIN_CONFIRMATIONS="${MIN_CONFIRMATIONS:-1}"
ALICE_DEV_SEED="${ALICE_DEV_SEED:-0001020304050607080900010203040506070809000102030405060708090001}"
BOB_DEV_SEED="${BOB_DEV_SEED:-1001020304050607080900010203040506070809000102030405060708090010}"
FAUCET_DEV_SEED="${FAUCET_DEV_SEED:-2001020304050607080900010203040506070809000102030405060708090020}"

dev_key_field() {
  local output="$1"
  local field="$2"
  printf '%s\n' "$output" | awk -F= -v field="$field" '$1 == field { print $2; exit }'
}

wait_for_rpc_port() {
  local deadline=$(( $(date +%s) + 60 ))
  while (( $(date +%s) <= deadline )); do
    if nc -z "$TONDI_RPC_HOST" "$TONDI_RPC_PORT" >/dev/null 2>&1; then
      return 0
    fi
    if ! kill -0 "$TONDID_PID" >/dev/null 2>&1; then
      echo "tondid exited before RPC became available. Last log lines:" >&2
      tail -n 80 "$TONDID_LOG" >&2 || true
      return 1
    fi
    sleep 1
  done
  echo "Timed out waiting for Tondi RPC at $TONDI_RPC" >&2
  tail -n 80 "$TONDID_LOG" >&2 || true
  return 1
}

wait_for_tx_accepted() {
  local txid="$1"
  local deadline=$(( $(date +%s) + 120 ))
  local status_output status
  while (( $(date +%s) <= deadline )); do
    status_output="$(TONDI_RPC="$TONDI_RPC" TONDI_TXID="$txid" cargo run --quiet -p bp-tondi --features grpc --example query_status 2>&1 || true)"
    status="$(printf '%s\n' "$status_output" | awk -F= '$1 == "status" { print $2; exit }')"
    if [[ "$status" == "accepted" ]]; then
      printf '%s\n' "$status_output"
      return 0
    fi
    if ! kill -0 "$TONDID_PID" >/dev/null 2>&1; then
      echo "tondid exited while waiting for transaction acceptance" >&2
      tail -n 80 "$TONDID_LOG" >&2 || true
      return 1
    fi
    sleep 2
  done
  echo "Timed out waiting for funding transaction acceptance: $txid" >&2
  printf '%s\n' "$status_output" >&2
  tail -n 80 "$MINER_LOG" >&2 || true
  return 1
}

cleanup() {
  if [[ -n "${MINER_PID:-}" ]] && kill -0 "$MINER_PID" >/dev/null 2>&1; then
    kill "$MINER_PID" >/dev/null 2>&1 || true
    wait "$MINER_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "${TONDID_PID:-}" ]] && kill -0 "$TONDID_PID" >/dev/null 2>&1; then
    kill "$TONDID_PID" >/dev/null 2>&1 || true
    wait "$TONDID_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ ! "$PREALLOC_UTXOS" =~ ^[0-9]+$ ]] || (( PREALLOC_UTXOS < 4 )); then
  echo "PREALLOC_UTXOS must be at least 4 for funding and public-context probes" >&2
  exit 2
fi

if [[ ! -d "$TONDI_REPO" ]]; then
  echo "TONDI_REPO does not exist: $TONDI_REPO" >&2
  exit 2
fi

if nc -z "$TONDI_RPC_HOST" "$TONDI_RPC_PORT" >/dev/null 2>&1; then
  echo "Tondi RPC port is already in use: $TONDI_RPC" >&2
  exit 2
fi
if nc -z "$TONDI_RPC_HOST" "$TONDI_WRPC_PORT" >/dev/null 2>&1; then
  echo "Tondi wRPC port is already in use: $TONDI_WRPC" >&2
  exit 2
fi

rm -rf "$DATA_ROOT"
mkdir -p "$DATA_ROOT"

cd "$ROOT"

echo "[1/8] Build RGB CLI and generate deterministic workflow material"
cargo build --bin rgb
ALICE_DEV_KEYS="$(cargo run --quiet --bin rgb -- dev-keys --seed-hex "$ALICE_DEV_SEED")"
BOB_DEV_KEYS="$(cargo run --quiet --bin rgb -- dev-keys --seed-hex "$BOB_DEV_SEED")"
FAUCET_DEV_KEYS="$(cargo run --quiet --bin rgb -- dev-keys --seed-hex "$FAUCET_DEV_SEED")"
ALICE_XPUB="$(dev_key_field "$ALICE_DEV_KEYS" xpub)"
BOB_XPUB="$(dev_key_field "$BOB_DEV_KEYS" xpub)"
FAUCET_XPUB="$(dev_key_field "$FAUCET_DEV_KEYS" xpub)"
ALICE_P2MR_SECRET_KEY="$(dev_key_field "$ALICE_DEV_KEYS" outer0_tondi_p2mr_secret_key)"
FAUCET_P2MR_SECRET_KEY="$(dev_key_field "$FAUCET_DEV_KEYS" outer0_tondi_p2mr_secret_key)"
cargo run --quiet --bin rgb -- dev-issuer --force "$ISSUER_FILE"

RGB_FAUCET=(cargo run --quiet --bin rgb -- --data-dir "$DATA_ROOT/faucet" --network "$NETWORK" --tondi="$TONDI_RPC")
RGB_ALICE=(cargo run --quiet --bin rgb -- --data-dir "$DATA_ROOT/alice" --network "$NETWORK" --tondi="$TONDI_RPC")
RGB_BOB=(cargo run --quiet --bin rgb -- --data-dir "$DATA_ROOT/bob" --network "$NETWORK" --tondi="$TONDI_RPC")

echo "[2/8] Create P2MR wallets before node start"
"${RGB_FAUCET[@]}" init --quiet || true
"${RGB_ALICE[@]}" init --quiet || true
"${RGB_BOB[@]}" init --quiet || true
"${RGB_FAUCET[@]}" create faucet "$FAUCET_XPUB" || true
"${RGB_ALICE[@]}" create "$WALLET_ALICE" "$ALICE_XPUB" || true
"${RGB_BOB[@]}" create "$WALLET_BOB" "$BOB_XPUB" || true
FAUCET_FUND_ADDR="$(RGB_WALLET=faucet "${RGB_FAUCET[@]}" fund)"
ALICE_FUND_ADDR="$(RGB_WALLET="$WALLET_ALICE" "${RGB_ALICE[@]}" fund)"
BOB_FUND_ADDR="$(RGB_WALLET="$WALLET_BOB" "${RGB_BOB[@]}" fund)"
echo "  Faucet prealloc address: $FAUCET_FUND_ADDR"
echo "  Alice funding address:   $ALICE_FUND_ADDR"
echo "  Bob funding address:     $BOB_FUND_ADDR"

echo "[3/8] Build patched Tondi node and local devnet miner"
(cd "$TONDI_REPO" && cargo build -p tondid --features devnet-prealloc && cargo build --bin tondi-devnet-miner)
TONDID_BIN="$TONDI_REPO/target/debug/tondid"
MINER_BIN="$TONDI_REPO/target/debug/tondi-devnet-miner"

echo "[4/8] Start local Tondi devnet with faucet P2MR preallocation"
"$TONDID_BIN" \
  --devnet \
  --utxoindex \
  --enable-unsynced-mining \
  --reset-db \
  --nologfiles \
  --loglevel=warn \
  --appdir="$TONDID_APPDIR" \
  --rpclisten="$TONDI_RPC" \
  --rpclisten-borsh="$TONDI_RPC_HOST:$TONDI_WRPC_PORT" \
  --listen="$TONDI_RPC_HOST:$TONDI_P2P_PORT" \
  --prealloc-target="$FAUCET_FUND_ADDR,$PREALLOC_UTXOS,$PREALLOC_AMOUNT" \
  >"$TONDID_LOG" 2>&1 &
TONDID_PID=$!
wait_for_rpc_port
echo "  Tondi RPC: $TONDI_RPC"
echo "  Tondi wRPC: $TONDI_WRPC"
echo "  Tondi log: $TONDID_LOG"

echo "[5/8] Mine local devnet and fund Alice/Bob with a real transaction"
"$MINER_BIN" "$TONDI_WRPC" "$FAUCET_FUND_ADDR" "$MINER_THREADS" >"$MINER_LOG" 2>&1 &
MINER_PID=$!
sleep 2
FUNDING_OUTPUT="$(
  TONDI_RPC="$TONDI_RPC" \
  FAUCET_P2MR_SECRET_KEY="$FAUCET_P2MR_SECRET_KEY" \
  FAUCET_PREALLOC_ID=1 \
  FAUCET_PREALLOC_AMOUNT="$PREALLOC_AMOUNT" \
  FUND_TARGETS="$ALICE_FUND_ADDR=$FUNDING_AMOUNT,$BOB_FUND_ADDR=$FUNDING_AMOUNT" \
  cargo run --quiet -p bp-tondi --features grpc --example devnet_p2mr_fund
)"
printf '%s\n' "$FUNDING_OUTPUT"
FUNDING_TXID="$(printf '%s\n' "$FUNDING_OUTPUT" | awk -F= '$1 == "funding_txid" { print $2; exit }')"
if [[ -z "$FUNDING_TXID" ]]; then
  echo "Unable to parse funding transaction id" >&2
  exit 1
fi
wait_for_tx_accepted "$FUNDING_TXID"

echo "[6/8] Verify public Datum, reusable read-only context and spent-reference rejection"
TONDI_RPC="$TONDI_RPC" \
FAUCET_P2MR_ADDRESS="$FAUCET_FUND_ADDR" \
FAUCET_P2MR_SECRET_KEY="$FAUCET_P2MR_SECRET_KEY" \
FAUCET_PREALLOC_AMOUNT="$PREALLOC_AMOUNT" \
cargo run --quiet -p bp-tondi --features grpc --example devnet_public_context

echo "[7/8] Run full RGB-on-Tondi workflow with real broadcast"
AUTO_DEV_KEYS=0 \
WAIT_FOR_FUNDING=1 \
NON_INTERACTIVE=1 \
AUTO_BROADCAST=1 \
REQUIRE_BROADCAST_ACCEPTED=1 \
BROADCAST_STATUS_ATTEMPTS="${BROADCAST_STATUS_ATTEMPTS:-90}" \
BROADCAST_STATUS_POLL_SECONDS="${BROADCAST_STATUS_POLL_SECONDS:-2}" \
MIN_CONFIRMATIONS="$MIN_CONFIRMATIONS" \
TONDI_RPC="$TONDI_RPC" \
NETWORK="$NETWORK" \
DATA_ROOT="$DATA_ROOT" \
DATA_DIR_ALICE="$DATA_ROOT/alice" \
DATA_DIR_BOB="$DATA_ROOT/bob" \
WALLET_ALICE="$WALLET_ALICE" \
WALLET_BOB="$WALLET_BOB" \
ISSUER_FILE="$ISSUER_FILE" \
ALICE_FUND_ADDR="$ALICE_FUND_ADDR" \
BOB_FUND_ADDR="$BOB_FUND_ADDR" \
ALICE_XPUB="$ALICE_XPUB" \
BOB_XPUB="$BOB_XPUB" \
ALICE_P2MR_SECRET_KEY="$ALICE_P2MR_SECRET_KEY" \
"$ROOT/test_full_workflow.sh"

echo "[8/8] Local devnet broadcast workflow completed"
echo "  Data root: $DATA_ROOT"
cleanup
trap - EXIT
