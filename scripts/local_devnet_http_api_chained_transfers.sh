#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TONDI_REPO="${TONDI_REPO:-"$ROOT/../Tondi"}"
TONDI_TARGET_DIR="${TONDI_TARGET_DIR:-"$TONDI_REPO/target"}"
DATA_ROOT="${DATA_ROOT:-/tmp/rgb-on-tondi-http-api-chain}"
TONDI_FORCE_REMOVED_TXIDS_FILE="${TONDI_FORCE_REMOVED_TXIDS_FILE:-$DATA_ROOT/tondi-force-removed-txids.txt}"
NETWORK="${NETWORK:-devnet}"
TONDI_RPC_HOST="${TONDI_RPC_HOST:-127.0.0.1}"
TONDI_RPC_PORT="${TONDI_RPC_PORT:-16620}"
TONDI_WRPC_PORT="${TONDI_WRPC_PORT:-17620}"
TONDI_P2P_PORT="${TONDI_P2P_PORT:-16621}"
TONDI_RPC="${TONDI_RPC:-$TONDI_RPC_HOST:$TONDI_RPC_PORT}"
TONDI_WRPC="${TONDI_WRPC:-ws://$TONDI_RPC_HOST:$TONDI_WRPC_PORT}"
TONDID_APPDIR="${TONDID_APPDIR:-$DATA_ROOT/tondid}"
TONDID_LOG="${TONDID_LOG:-$DATA_ROOT/tondid.log}"
MINER_LOG="${MINER_LOG:-$DATA_ROOT/devnet-miner.log}"
ISSUER_FILE="${ISSUER_FILE:-$DATA_ROOT/dev.issuer}"
PREALLOC_UTXOS="${PREALLOC_UTXOS:-6}"
PREALLOC_AMOUNT="${PREALLOC_AMOUNT:-32000000000}"
FUNDING_AMOUNT="${FUNDING_AMOUNT:-4000000000}"
MINER_THREADS="${MINER_THREADS:-2}"
MIN_CONFIRMATIONS="${MIN_CONFIRMATIONS:-1}"
API_NAME="${API_NAME:-Test}"
TRANSFER_METHOD="${TRANSFER_METHOD:-transfer}"
STATE_NAME="${STATE_NAME:-amount}"
TRANSFER_FEE_TONDI="${TRANSFER_FEE_TONDI:-0.00010000}"
TRANSFER_CARRIER_TONDI="${TRANSFER_CARRIER_TONDI:-0.00010000}"
TENTATIVE_CHAIN="${TENTATIVE_CHAIN:-0}"
FAUCET_DEV_SEED="${FAUCET_DEV_SEED:-2001020304050607080900010203040506070809000102030405060708090020}"
ALICE_HTTP_PORT="${ALICE_HTTP_PORT:-18781}"
BOB_HTTP_PORT="${BOB_HTTP_PORT:-18782}"
CAROL_HTTP_PORT="${CAROL_HTTP_PORT:-18783}"
ALICE_API="http://127.0.0.1:$ALICE_HTTP_PORT"
BOB_API="http://127.0.0.1:$BOB_HTTP_PORT"
CAROL_API="http://127.0.0.1:$CAROL_HTTP_PORT"
ALICE_DIR="$DATA_ROOT/alice-http"
BOB_DIR="$DATA_ROOT/bob-http"
CAROL_DIR="$DATA_ROOT/carol-http"
PASS="${HTTP_WALLET_PASSPHRASE:-avato-devnet-passphrase}"

ALICE_PHRASE="${ALICE_PHRASE:-abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about}"
BOB_PHRASE="${BOB_PHRASE:-legal winner thank year wave sausage worth useful legal winner thank yellow}"
CAROL_PHRASE="${CAROL_PHRASE:-letter advice cage absurd amount doctor acoustic avoid letter advice cage above}"

dev_key_field() {
  local output="$1"
  local field="$2"
  printf '%s\n' "$output" | awk -F= -v field="$field" '$1 == field { print $2; exit }'
}

json_get() {
  local path="$1"
  node -e '
    const path = process.argv[1].split(".");
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      let value = JSON.parse(input);
      for (const key of path) value = value?.[key];
      if (value === undefined || value === null) process.exit(2);
      if (typeof value === "object") console.log(JSON.stringify(value));
      else console.log(String(value));
    });
  ' "$path"
}

json_wallet_payload() {
  local wallet_id="$1"
  local phrase="$2"
  node -e '
    const [walletId, network, endpoint, passphrase, phrase] = process.argv.slice(1);
    console.log(JSON.stringify({
      walletId,
      network,
      tondiRpcEndpoint: endpoint,
      passphrase,
      recoveryPhrase: phrase.trim().split(/\s+/),
    }));
  ' "$wallet_id" "$NETWORK" "$TONDI_RPC" "$PASS" "$phrase"
}

json_field_payload() {
  local key="$1"
  local value="$2"
  node -e 'const [key, value] = process.argv.slice(1); console.log(JSON.stringify({ [key]: value }));' "$key" "$value"
}

json_unlock_payload() {
  node -e 'const [passphrase] = process.argv.slice(1); console.log(JSON.stringify({ passphrase }));' "$PASS"
}

api_post() {
  local base="$1"
  local path="$2"
  local payload="$3"
  local response body status
  response="$(curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$payload" "$base$path")"
  status="${response##*$'\n'}"
  body="${response%$'\n'*}"
  if (( status < 200 || status >= 300 )); then
    echo "HTTP POST $base$path failed with status $status" >&2
    printf '%s\n' "$body" >&2
    return 22
  fi
  printf '%s\n' "$body"
}

api_post_empty() {
  local base="$1"
  local path="$2"
  local response body status
  response="$(curl -sS -w $'\n%{http_code}' -X POST "$base$path")"
  status="${response##*$'\n'}"
  body="${response%$'\n'*}"
  if (( status < 200 || status >= 300 )); then
    echo "HTTP POST $base$path failed with status $status" >&2
    printf '%s\n' "$body" >&2
    return 22
  fi
  printf '%s\n' "$body"
}

api_get() {
  local base="$1"
  local path="$2"
  local response body status
  response="$(curl -sS -w $'\n%{http_code}' "$base$path")"
  status="${response##*$'\n'}"
  body="${response%$'\n'*}"
  if (( status < 200 || status >= 300 )); then
    echo "HTTP GET $base$path failed with status $status" >&2
    printf '%s\n' "$body" >&2
    return 22
  fi
  printf '%s\n' "$body"
}

api_post_expect_failure() {
  local base="$1"
  local path="$2"
  local payload="$3"
  local label="$4"
  local response body status
  response="$(curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$payload" "$base$path")"
  status="${response##*$'\n'}"
  body="${response%$'\n'*}"
  if (( status >= 200 && status < 300 )); then
    echo "$label: expected HTTP failure, got status $status" >&2
    printf '%s\n' "$body" >&2
    exit 1
  fi
  printf '%s\n' "$body"
}

response_status_file() {
  local file="$1"
  tail -n 1 "$file"
}

response_body_file() {
  local file="$1"
  sed '$d' "$file"
}

require_response_success() {
  local file="$1"
  local label="$2"
  local status
  status="$(response_status_file "$file")"
  if (( status < 200 || status >= 300 )); then
    echo "$label failed with status $status" >&2
    response_body_file "$file" >&2
    exit 1
  fi
}

wait_for_port() {
  local label="$1"
  local port="$2"
  local pid="$3"
  local log="$4"
  local deadline=$(( $(date +%s) + 90 ))
  while (( $(date +%s) <= deadline )); do
    if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
      return 0
    fi
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      echo "$label exited before port $port became available. Last log lines:" >&2
      tail -n 80 "$log" >&2 || true
      return 1
    fi
    sleep 1
  done
  echo "Timed out waiting for $label on port $port" >&2
  tail -n 80 "$log" >&2 || true
  return 1
}

wait_for_tx_accepted() {
  local txid="$1"
  local deadline=$(( $(date +%s) + 180 ))
  local status_output status
  while (( $(date +%s) <= deadline )); do
    status_output="$(TONDI_RPC="$TONDI_RPC" TONDI_TXID="$txid" cargo run --quiet -p bp-tondi --features grpc --example query_status 2>&1 || true)"
    status="$(printf '%s\n' "$status_output" | awk -F= '$1 == "status" { print $2; exit }')"
    if [[ "$status" == "accepted" ]]; then
      printf '%s\n' "$status_output"
      return 0
    fi
    if [[ -n "${TONDID_PID:-}" ]] && ! kill -0 "$TONDID_PID" >/dev/null 2>&1; then
      echo "tondid exited while waiting for transaction acceptance" >&2
      tail -n 80 "$TONDID_LOG" >&2 || true
      return 1
    fi
    sleep 2
  done
  echo "Timed out waiting for transaction acceptance: $txid" >&2
  printf '%s\n' "${status_output:-}" >&2
  tail -n 80 "$MINER_LOG" >&2 || true
  return 1
}

wait_for_tx_status() {
  local txid="$1"
  local expected="$2"
  local deadline=$(( $(date +%s) + 90 ))
  local status_output status
  while (( $(date +%s) <= deadline )); do
    status_output="$(TONDI_RPC="$TONDI_RPC" TONDI_TXID="$txid" cargo run --quiet -p bp-tondi --features grpc --example query_status 2>&1 || true)"
    status="$(printf '%s\n' "$status_output" | awk -F= '$1 == "status" { print $2; exit }')"
    if [[ "$status" == "$expected" ]]; then
      printf '%s\n' "$status_output"
      return 0
    fi
    if [[ -n "${TONDID_PID:-}" ]] && ! kill -0 "$TONDID_PID" >/dev/null 2>&1; then
      echo "tondid exited while waiting for transaction status '$expected'" >&2
      tail -n 80 "$TONDID_LOG" >&2 || true
      return 1
    fi
    sleep 1
  done
  echo "Timed out waiting for transaction status '$expected': $txid" >&2
  printf '%s\n' "${status_output:-}" >&2
  return 1
}

asset_balance() {
  local json="$1"
  local contract_id="$2"
  node -e '
    const contractId = process.argv[1];
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      const dashboard = JSON.parse(input);
      const asset = dashboard.assets.find(asset => asset.contractId === contractId);
      if (!asset) process.exit(2);
      console.log(asset.balance);
    });
  ' "$contract_id" <<<"$json"
}

wait_for_asset_balance() {
  local base="$1"
  local contract_id="$2"
  local expected="$3"
  local label="$4"
  local deadline=$(( $(date +%s) + 90 ))
  local dashboard balance
  while (( $(date +%s) <= deadline )); do
    dashboard="$(api_post_empty "$base" "/wallet/sync")"
    balance="$(asset_balance "$dashboard" "$contract_id" 2>/dev/null || true)"
    if [[ "$balance" == "$expected" ]]; then
      printf '%s\n' "$dashboard"
      return 0
    fi
    sleep 2
  done
  echo "$label: expected balance '$expected', got '${balance:-missing}'" >&2
  return 1
}

transfer_count() {
  local json="$1"
  node -e '
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => console.log(JSON.parse(input).transfers.length));
  ' <<<"$json"
}

count_p2mr_outpoint() {
  local json="$1"
  local outpoint="$2"
  node -e '
    const [outpoint] = process.argv.slice(1);
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      const dashboard = JSON.parse(input);
      console.log((dashboard.p2mrOutputs ?? []).filter(output => output.outpoint === outpoint).length);
    });
  ' "$outpoint" <<<"$json"
}

assert_unique_lines() {
  local label="$1"
  local expected_count="$2"
  node -e '
    const [label, expectedCountText] = process.argv.slice(1);
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      const values = input.split(/\r?\n/).map(value => value.trim()).filter(Boolean);
      const expectedCount = Number(expectedCountText);
      if (values.length !== expectedCount) {
        console.error(`${label}: expected ${expectedCount} values, got ${values.length}`);
        process.exit(1);
      }
      const seen = new Set();
      for (const value of values) {
        if (seen.has(value)) {
          console.error(`${label}: duplicate value ${value}`);
          process.exit(1);
        }
        seen.add(value);
      }
    });
  ' "$label" "$expected_count"
}

dashboard_transfer_field() {
  local json="$1"
  local transfer_id="$2"
  local field="$3"
  node -e '
    const [transferId, field] = process.argv.slice(1);
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      const dashboard = JSON.parse(input);
      const transfer = dashboard.transfers.find(item => item.id === transferId);
      if (!transfer || transfer[field] === undefined || transfer[field] === null) process.exit(2);
      console.log(String(transfer[field]));
    });
  ' "$transfer_id" "$field" <<<"$json"
}

assert_global_invariants() {
  local label="$1"
  local alice_json="$2"
  local bob_json="$3"
  local carol_json="$4"
  local test_contract="$5"
  local test_supply="$6"
  local cash_contract="$7"
  local cash_supply="$8"
  local history_file="$DATA_ROOT/transfer-state-history.json"
  node -e '
    const fs = require("fs");
    const [
      label,
      aliceJson,
      bobJson,
      carolJson,
      testContract,
      testSupplyText,
      cashContract,
      cashSupplyText,
      historyFile,
    ] = process.argv.slice(1);
    const dashboards = [
      ["alice", JSON.parse(aliceJson)],
      ["bob", JSON.parse(bobJson)],
      ["carol", JSON.parse(carolJson)],
    ];
    const fail = message => {
      console.error(`${label}: ${message}`);
      process.exit(1);
    };
    const balanceOf = (dashboard, contractId) => {
      const asset = dashboard.assets.find(item => item.contractId === contractId);
      return BigInt(asset?.balance ?? "0");
    };
    const sum = contractId => dashboards.reduce((total, [, dashboard]) => total + balanceOf(dashboard, contractId), 0n);
    if (sum(testContract) !== BigInt(testSupplyText)) {
      fail(`TEST supply mismatch: ${sum(testContract)} != ${testSupplyText}`);
    }
    if (sum(cashContract) !== BigInt(cashSupplyText)) {
      fail(`CASH supply mismatch: ${sum(cashContract)} != ${cashSupplyText}`);
    }
    const stateRank = {
      "invoice-created": 0,
      "pstt-prepared": 1,
      "consignment-ready": 2,
      "receiver-accepted": 3,
      "finalised": 4,
      "broadcast": 5,
    };
    const validationRank = {
      "not-started": 0,
      "pending": 1,
      "valid": 2,
      "rejected": 2,
    };
    const validationCanAdvance = (previous, current) => {
      if (previous === current) return true;
      if (previous === "pending" && (current === "valid" || current === "rejected")) return true;
      if (previous === "not-started") return validationRank[current] >= 0;
      if (previous === "valid" && current === "rejected") return true;
      return false;
    };
    let history = {};
    if (fs.existsSync(historyFile)) {
      history = JSON.parse(fs.readFileSync(historyFile, "utf8"));
    }
    const seenOutpoints = new Set();
    const seenSeals = new Set();
    for (const [wallet, dashboard] of dashboards) {
      for (const output of dashboard.p2mrOutputs ?? []) {
        if (seenOutpoints.has(output.outpoint)) fail(`duplicate live P2MR outpoint ${output.outpoint}`);
        seenOutpoints.add(output.outpoint);
        if (seenSeals.has(output.seal)) fail(`duplicate live P2MR seal ${output.seal}`);
        seenSeals.add(output.seal);
      }
      const seenTransfers = new Set();
      const seenBroadcastTxids = new Set();
      for (const transfer of dashboard.transfers ?? []) {
        if (seenTransfers.has(transfer.id)) fail(`duplicate transfer record ${transfer.id}`);
        seenTransfers.add(transfer.id);
        if (!Array.isArray(transfer.contractTransfers) || transfer.contractTransfers.length === 0) {
          fail(`transfer ${transfer.id} has no contract transfer entries`);
        }
        if (!(transfer.state in stateRank)) {
          fail(`transfer ${transfer.id} has unknown state ${transfer.state}`);
        }
        if (!(transfer.validation in validationRank)) {
          fail(`transfer ${transfer.id} has unknown validation ${transfer.validation}`);
        }
        const historyKey = `${wallet}:${transfer.id}`;
        const previous = history[historyKey];
        if (previous) {
          if (stateRank[transfer.state] < stateRank[previous.state]) {
            fail(`transfer ${transfer.id} state regressed ${previous.state} -> ${transfer.state}`);
          }
          if (!validationCanAdvance(previous.validation, transfer.validation)) {
            fail(`transfer ${transfer.id} validation regressed ${previous.validation} -> ${transfer.validation}`);
          }
        }
        history[historyKey] = {
          state: transfer.state,
          validation: transfer.validation,
          label,
        };
        if (transfer.state === "broadcast" && !transfer.broadcastTxid) {
          fail(`broadcast transfer ${transfer.id} is missing broadcastTxid`);
        }
        if (transfer.broadcastTxid) {
          if (seenBroadcastTxids.has(transfer.broadcastTxid)) {
            fail(`duplicate broadcast txid ${transfer.broadcastTxid}`);
          }
          seenBroadcastTxids.add(transfer.broadcastTxid);
        }
      }
    }
    fs.writeFileSync(historyFile, JSON.stringify(history, null, 2));
  ' "$label" "$alice_json" "$bob_json" "$carol_json" "$test_contract" "$test_supply" "$cash_contract" "$cash_supply" "$history_file"
}

sync_all_and_assert_invariants() {
  local label="$1"
  local alice_json bob_json carol_json
  alice_json="$(api_post_empty "$ALICE_API" "/wallet/sync")"
  bob_json="$(api_post_empty "$BOB_API" "/wallet/sync")"
  carol_json="$(api_post_empty "$CAROL_API" "/wallet/sync")"
  assert_global_invariants \
    "$label" \
    "$alice_json" \
    "$bob_json" \
    "$carol_json" \
    "$CONTRACT_ID" \
    "1000000" \
    "$CONTRACT_ID_2" \
    "500000"
}

p2mr_seal_at() {
  local json="$1"
  local index="$2"
  node -e '
    const index = Number(process.argv[1]);
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      const dashboard = JSON.parse(input);
      const seals = dashboard.p2mrOutputs
        .map(output => output.seal)
        .filter((seal, itemIndex, all) => all.indexOf(seal) === itemIndex);
      if (!seals[index]) process.exit(2);
      console.log(seals[index]);
    });
  ' "$index" <<<"$json"
}

transfer_contract_field() {
  local json="$1"
  local contract_id="$2"
  local field="$3"
  node -e '
    const [contractId, field] = process.argv.slice(1);
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      const transfer = JSON.parse(input);
      const entry = transfer.contractTransfers?.find(item => item.contractId === contractId);
      if (!entry || entry[field] === undefined || entry[field] === null) process.exit(2);
      console.log(String(entry[field]));
    });
  ' "$contract_id" "$field" <<<"$json"
}

kv_field() {
  local key="$1"
  awk -F= -v key="$key" '$1 == key { print $2; exit }'
}

write_synthetic_broadcast_transfer_record() {
  local path="$1"
  local id="$2"
  local contract_id="$3"
  local consignment_ref="$4"
  local txid="$5"
  local pstt_ref="$6"
  node -e '
    const fs = require("fs");
    const [path, id, contractId, consignmentRef, txid, psttRef] = process.argv.slice(1);
    fs.writeFileSync(path, JSON.stringify({
      id,
      contractTransfers: [{
        contractId,
        amount: "1",
        consignmentRef,
      }],
      invoices: [],
      psttRef,
      broadcastTxid: txid,
      state: "broadcast",
      validation: "pending",
      updatedAt: new Date().toISOString(),
    }, null, 2));
  ' "$path" "$id" "$contract_id" "$consignment_ref" "$txid" "$pstt_ref"
}

require_eq() {
  local actual="$1"
  local expected="$2"
  local label="$3"
  if [[ "$actual" != "$expected" ]]; then
    echo "$label: expected '$expected', got '$actual'" >&2
    exit 1
  fi
}

require_file_exists() {
  local path="$1"
  local label="$2"
  if [[ ! -s "$path" ]]; then
    echo "$label: expected file to exist and be non-empty: $path" >&2
    exit 1
  fi
}

abs_ref() {
  local base_dir="$1"
  local ref="$2"
  if [[ "$ref" = /* ]]; then
    printf '%s\n' "$ref"
  else
    printf '%s/%s\n' "$base_dir" "$ref"
  fi
}

cleanup() {
  for pid in "${ALICE_HTTP_PID:-}" "${BOB_HTTP_PID:-}" "${CAROL_HTTP_PID:-}" "${MINER_PID:-}" "${TONDID_PID:-}"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
      wait "$pid" >/dev/null 2>&1 || true
    fi
  done
}
trap cleanup EXIT

start_miner() {
  if [[ -n "${MINER_PID:-}" ]] && kill -0 "$MINER_PID" >/dev/null 2>&1; then
    return 0
  fi
  "$MINER_BIN" "$TONDI_WRPC" "$FAUCET_FUND_ADDR" "$MINER_THREADS" >"$MINER_LOG" 2>&1 &
  MINER_PID=$!
}

stop_miner() {
  if [[ -n "${MINER_PID:-}" ]] && kill -0 "$MINER_PID" >/dev/null 2>&1; then
    kill "$MINER_PID" >/dev/null 2>&1 || true
    wait "$MINER_PID" >/dev/null 2>&1 || true
  fi
  MINER_PID=""
}

start_http_service() {
  local label="$1"
  local data_dir="$2"
  local port="$3"
  local log_file="$4"
  local pid_var="$5"
  "$RGB_BIN" --no-network-prefix --data-dir "$data_dir" --network "$NETWORK" --min-confirmations "$MIN_CONFIRMATIONS" --tondi="$TONDI_RPC" serve-http --bind "127.0.0.1:$port" >"$log_file" 2>&1 &
  local pid=$!
  printf -v "$pid_var" '%s' "$pid"
  wait_for_port "$label HTTP" "$port" "$pid" "$log_file"
}

stop_http_service() {
  local pid_var="$1"
  local pid="${!pid_var:-}"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    kill "$pid" >/dev/null 2>&1 || true
    wait "$pid" >/dev/null 2>&1 || true
  fi
  printf -v "$pid_var" ''
}

restart_http_service() {
  local label="$1"
  local base="$2"
  local data_dir="$3"
  local port="$4"
  local log_file="$5"
  local pid_var="$6"
  echo "  Restarting $label HTTP service"
  stop_http_service "$pid_var"
  start_http_service "$label" "$data_dir" "$port" "$log_file" "$pid_var"
  api_post "$base" "/wallet/unlock" "$(json_unlock_payload)" >/dev/null
}

if [[ ! -d "$TONDI_REPO" ]]; then
  echo "TONDI_REPO does not exist: $TONDI_REPO" >&2
  exit 2
fi

for port in "$TONDI_RPC_PORT" "$TONDI_WRPC_PORT" "$ALICE_HTTP_PORT" "$BOB_HTTP_PORT" "$CAROL_HTTP_PORT"; do
  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    echo "Port is already in use: $port" >&2
    exit 2
  fi
done

rm -rf "$DATA_ROOT"
mkdir -p "$DATA_ROOT"
: >"$TONDI_FORCE_REMOVED_TXIDS_FILE"

cd "$ROOT"

echo "[1/12] Build RGB CLI and deterministic issuer/faucet material"
cargo build --bin rgb
RGB_BIN="$ROOT/target/debug/rgb"
FAUCET_DEV_KEYS="$("$RGB_BIN" dev-keys --seed-hex "$FAUCET_DEV_SEED")"
FAUCET_XPUB="$(dev_key_field "$FAUCET_DEV_KEYS" xpub)"
FAUCET_P2MR_SECRET_KEY="$(dev_key_field "$FAUCET_DEV_KEYS" outer0_tondi_p2mr_secret_key)"
"$RGB_BIN" dev-issuer --force "$ISSUER_FILE"

RGB_FAUCET=("$RGB_BIN" --data-dir "$DATA_ROOT/faucet" --network "$NETWORK" --tondi="$TONDI_RPC")
"${RGB_FAUCET[@]}" init --quiet || true
"${RGB_FAUCET[@]}" create faucet "$FAUCET_XPUB" || true
FAUCET_FUND_ADDR="$(RGB_WALLET=faucet "${RGB_FAUCET[@]}" fund)"
echo "  Faucet prealloc address: $FAUCET_FUND_ADDR"

echo "[2/12] Build patched Tondi node and local devnet miner"
(cd "$TONDI_REPO" && CARGO_TARGET_DIR="$TONDI_TARGET_DIR" cargo build -p tondid --features devnet-prealloc && CARGO_TARGET_DIR="$TONDI_TARGET_DIR" cargo build --bin tondi-devnet-miner)
TONDID_BIN="$TONDI_TARGET_DIR/debug/tondid"
MINER_BIN="$TONDI_TARGET_DIR/debug/tondi-devnet-miner"

echo "[3/12] Start local Tondi devnet"
TONDI_DEVNET_FORCE_REMOVED_TXIDS_FILE="$TONDI_FORCE_REMOVED_TXIDS_FILE" \
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
wait_for_port "tondid" "$TONDI_RPC_PORT" "$TONDID_PID" "$TONDID_LOG"
echo "  Tondi RPC: $TONDI_RPC"

echo "[4/12] Start Alice, Bob, and Carol HTTP wallet services"
start_http_service "Alice" "$ALICE_DIR" "$ALICE_HTTP_PORT" "$DATA_ROOT/alice-http.log" ALICE_HTTP_PID
start_http_service "Bob" "$BOB_DIR" "$BOB_HTTP_PORT" "$DATA_ROOT/bob-http.log" BOB_HTTP_PID
start_http_service "Carol" "$CAROL_DIR" "$CAROL_HTTP_PORT" "$DATA_ROOT/carol-http.log" CAROL_HTTP_PID

ALICE_PROFILE="$(api_post "$ALICE_API" "/wallets" "$(json_wallet_payload alice "$ALICE_PHRASE")")"
BOB_PROFILE="$(api_post "$BOB_API" "/wallets" "$(json_wallet_payload bob "$BOB_PHRASE")")"
CAROL_PROFILE="$(api_post "$CAROL_API" "/wallets" "$(json_wallet_payload carol "$CAROL_PHRASE")")"
ALICE_FUND_ADDR="$(json_get p2mrAddress <<<"$ALICE_PROFILE")"
BOB_FUND_ADDR="$(json_get p2mrAddress <<<"$BOB_PROFILE")"
CAROL_FUND_ADDR="$(json_get p2mrAddress <<<"$CAROL_PROFILE")"
ALICE_SECOND_ADDR_RESPONSE="$(api_post_empty "$ALICE_API" "/wallet/p2mr-address")"
ALICE_ISSUE_FUND_ADDR="$(json_get address <<<"$ALICE_SECOND_ADDR_RESPONSE")"
BOB_SECOND_ADDR_RESPONSE="$(api_post_empty "$BOB_API" "/wallet/p2mr-address")"
BOB_FUND_ADDR_2="$(json_get address <<<"$BOB_SECOND_ADDR_RESPONSE")"
BOB_THIRD_ADDR_RESPONSE="$(api_post_empty "$BOB_API" "/wallet/p2mr-address")"
BOB_FUND_ADDR_3="$(json_get address <<<"$BOB_THIRD_ADDR_RESPONSE")"
if [[ "$ALICE_FUND_ADDR" == "$ALICE_ISSUE_FUND_ADDR" ]]; then
  echo "Alice P2MR address derivation returned a duplicate address" >&2
  exit 1
fi
printf '%s\n%s\n%s\n' "$BOB_FUND_ADDR" "$BOB_FUND_ADDR_2" "$BOB_FUND_ADDR_3" | assert_unique_lines "Bob funding P2MR addresses" 3
echo "  Alice funding address: $ALICE_FUND_ADDR"
echo "  Alice second issue address: $ALICE_ISSUE_FUND_ADDR"
echo "  Bob funding address:   $BOB_FUND_ADDR"
echo "  Bob second funding address: $BOB_FUND_ADDR_2"
echo "  Bob third funding address:  $BOB_FUND_ADDR_3"
echo "  Carol funding address: $CAROL_FUND_ADDR"

echo "[5/12] Mine and fund HTTP wallets with a real P2MR transaction"
start_miner
sleep 2
FUNDING_OUTPUT="$(
  TONDI_RPC="$TONDI_RPC" \
  FAUCET_P2MR_SECRET_KEY="$FAUCET_P2MR_SECRET_KEY" \
  FAUCET_PREALLOC_ID=1 \
  FAUCET_PREALLOC_AMOUNT="$PREALLOC_AMOUNT" \
  FUND_TARGETS="$ALICE_FUND_ADDR=$FUNDING_AMOUNT,$ALICE_ISSUE_FUND_ADDR=$FUNDING_AMOUNT,$BOB_FUND_ADDR=$FUNDING_AMOUNT,$BOB_FUND_ADDR_2=$FUNDING_AMOUNT,$BOB_FUND_ADDR_3=$FUNDING_AMOUNT,$CAROL_FUND_ADDR=$FUNDING_AMOUNT" \
  cargo run --quiet -p bp-tondi --features grpc --example devnet_p2mr_fund
)"
printf '%s\n' "$FUNDING_OUTPUT"
FUNDING_TXID="$(printf '%s\n' "$FUNDING_OUTPUT" | awk -F= '$1 == "funding_txid" { print $2; exit }')"
if [[ -z "$FUNDING_TXID" ]]; then
  echo "Unable to parse funding transaction id" >&2
  exit 1
fi
wait_for_tx_accepted "$FUNDING_TXID" >/dev/null

api_post_empty "$ALICE_API" "/wallet/sync" >/dev/null
api_post_empty "$BOB_API" "/wallet/sync" >/dev/null
api_post_empty "$CAROL_API" "/wallet/sync" >/dev/null

echo "[6/12] Alice issues two assets and exports contract consignments through HTTP"
api_post "$ALICE_API" "/issuers/import" "$(json_field_payload fileName "$ISSUER_FILE")" >/dev/null
ALICE_DASHBOARD="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ISSUE_SEAL="$(p2mr_seal_at "$ALICE_DASHBOARD" 0)"
ISSUE_SEAL_2="$(p2mr_seal_at "$ALICE_DASHBOARD" 1)"
if [[ "$ISSUE_SEAL" == "$ISSUE_SEAL_2" ]]; then
  echo "Alice issue seals are not distinct" >&2
  exit 1
fi
ISSUE_PAYLOAD="$(
  node -e '
    const [issuerFileName, issueSeal] = process.argv.slice(1);
    console.log(JSON.stringify({
      issuerFileName,
      ticker: "TEST",
      name: "Avato Test Asset",
      precision: 0,
      issuedSupply: "1000000",
      issueSeal,
    }));
  ' "$ISSUER_FILE" "$ISSUE_SEAL"
)"
ISSUED_ASSET="$(api_post "$ALICE_API" "/contracts/issue" "$ISSUE_PAYLOAD")"
CONTRACT_ID="$(json_get contractId <<<"$ISSUED_ASSET")"
CONTRACT_REF="$(json_get contractConsignmentRef <<<"$ISSUED_ASSET")"
CONTRACT_PATH="$(abs_ref "$ALICE_DIR" "$CONTRACT_REF")"
echo "  Contract: $CONTRACT_ID"
echo "  Contract consignment: $CONTRACT_PATH"

api_post "$BOB_API" "/contracts/accept" "$(json_field_payload consignmentName "$CONTRACT_PATH")" >/dev/null
api_post "$CAROL_API" "/contracts/accept" "$(json_field_payload consignmentName "$CONTRACT_PATH")" >/dev/null

ISSUE_PAYLOAD_2="$(
  node -e '
    const [issuerFileName, issueSeal] = process.argv.slice(1);
    console.log(JSON.stringify({
      issuerFileName,
      ticker: "CASH",
      name: "Avato Cash Test Asset",
      precision: 0,
      issuedSupply: "500000",
      issueSeal,
    }));
  ' "$ISSUER_FILE" "$ISSUE_SEAL_2"
)"
ISSUED_ASSET_2="$(api_post "$ALICE_API" "/contracts/issue" "$ISSUE_PAYLOAD_2")"
CONTRACT_ID_2="$(json_get contractId <<<"$ISSUED_ASSET_2")"
CONTRACT_REF_2="$(json_get contractConsignmentRef <<<"$ISSUED_ASSET_2")"
CONTRACT_PATH_2="$(abs_ref "$ALICE_DIR" "$CONTRACT_REF_2")"
echo "  Second contract: $CONTRACT_ID_2"
echo "  Second contract consignment: $CONTRACT_PATH_2"

api_post "$CAROL_API" "/contracts/accept" "$(json_field_payload consignmentName "$CONTRACT_PATH_2")" >/dev/null

echo "  Verifying P2MR address and invoice receiver seal uniqueness"
P2MR_FUZZ_ADDRESSES=""
for _ in $(seq 1 20); do
  P2MR_FUZZ_ADDRESS_RESPONSE="$(api_post_empty "$ALICE_API" "/wallet/p2mr-address")"
  P2MR_FUZZ_ADDRESSES+="$(json_get address <<<"$P2MR_FUZZ_ADDRESS_RESPONSE")"$'\n'
done
printf '%s' "$P2MR_FUZZ_ADDRESSES" | assert_unique_lines "Alice derived P2MR addresses" 20

INVOICE_FUZZ_SEALS=""
for index in $(seq 1 10); do
  BOB_FUZZ_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName, amount] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME" "$index")"
  BOB_FUZZ_INVOICE="$(api_post "$BOB_API" "/invoices" "$BOB_FUZZ_INVOICE_PAYLOAD")"
  INVOICE_FUZZ_SEALS+="$(json_get receiverP2mrSeal <<<"$BOB_FUZZ_INVOICE")"$'\n'
done
for index in $(seq 11 20); do
  CAROL_FUZZ_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName, amount] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME" "$index")"
  CAROL_FUZZ_INVOICE="$(api_post "$CAROL_API" "/invoices" "$CAROL_FUZZ_INVOICE_PAYLOAD")"
  INVOICE_FUZZ_SEALS+="$(json_get receiverP2mrSeal <<<"$CAROL_FUZZ_INVOICE")"$'\n'
done
printf '%s' "$INVOICE_FUZZ_SEALS" | assert_unique_lines "HTTP invoice receiver P2MR seals" 20

echo "  Verifying oversized batch payment fails atomically before transfer persistence"
ALICE_PRE_FAILURE_DASHBOARD="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_PRE_FAILURE_TRANSFER_COUNT="$(transfer_count "$ALICE_PRE_FAILURE_DASHBOARD")"
BOB_OVERSIZED_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "2000000" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
CAROL_SMALL_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "1" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
BOB_OVERSIZED_INVOICE="$(api_post "$BOB_API" "/invoices" "$BOB_OVERSIZED_INVOICE_PAYLOAD")"
CAROL_SMALL_INVOICE="$(api_post "$CAROL_API" "/invoices" "$CAROL_SMALL_INVOICE_PAYLOAD")"
BOB_OVERSIZED_INVOICE_TEXT="$(json_get invoice <<<"$BOB_OVERSIZED_INVOICE")"
CAROL_SMALL_INVOICE_TEXT="$(json_get invoice <<<"$CAROL_SMALL_INVOICE")"
OVERSIZED_BATCH_PAYLOAD="$(node -e 'const [badInvoice, goodInvoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoices: [badInvoice, goodInvoice], feeTondi, carrierValueTondi }));' "$BOB_OVERSIZED_INVOICE_TEXT" "$CAROL_SMALL_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
api_post_expect_failure "$ALICE_API" "/transfers/pay-batch" "$OVERSIZED_BATCH_PAYLOAD" "oversized batch payment" >/dev/null
ALICE_POST_FAILURE_DASHBOARD="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_POST_FAILURE_TRANSFER_COUNT="$(transfer_count "$ALICE_POST_FAILURE_DASHBOARD")"
require_eq "$ALICE_POST_FAILURE_TRANSFER_COUNT" "$ALICE_PRE_FAILURE_TRANSFER_COUNT" "Alice transfer count after failed oversized batch"

echo "  Verifying corrupt transfer record makes sync fail explicitly and recover after removal"
ALICE_CORRUPT_TRANSFER_ID="transfer-corrupt-half-written"
ALICE_CORRUPT_RECORD="$ALICE_DIR/transfers/$ALICE_CORRUPT_TRANSFER_ID.json"
mkdir -p "$(dirname "$ALICE_CORRUPT_RECORD")"
node -e '
  const fs = require("fs");
  const [path, id, contractId] = process.argv.slice(1);
  fs.writeFileSync(path, JSON.stringify({
    id,
    contractTransfers: [{
      contractId,
    }],
    invoices: [],
    psttRef: "transfers/corrupt-half-written.pstt",
    state: "consignment-ready",
    validation: "pending",
    updatedAt: new Date().toISOString(),
  }, null, 2));
' "$ALICE_CORRUPT_RECORD" "$ALICE_CORRUPT_TRANSFER_ID" "$CONTRACT_ID"
CORRUPT_SYNC_RESPONSE="$DATA_ROOT/alice-corrupt-sync.response"
curl -sS -w $'\n%{http_code}' -X POST "$ALICE_API/wallet/sync" >"$CORRUPT_SYNC_RESPONSE"
CORRUPT_SYNC_STATUS="$(response_status_file "$CORRUPT_SYNC_RESPONSE")"
if (( CORRUPT_SYNC_STATUS >= 200 && CORRUPT_SYNC_STATUS < 300 )); then
  echo "corrupt transfer sync: expected HTTP failure, got status $CORRUPT_SYNC_STATUS" >&2
  response_body_file "$CORRUPT_SYNC_RESPONSE" >&2
  exit 1
fi
if [[ "$(response_body_file "$CORRUPT_SYNC_RESPONSE")" != *"transfer record"* || "$(response_body_file "$CORRUPT_SYNC_RESPONSE")" != *"invalid"* ]]; then
  echo "corrupt transfer sync: expected explicit transfer-record validation error, got:" >&2
  response_body_file "$CORRUPT_SYNC_RESPONSE" >&2
  exit 1
fi
rm -f "$ALICE_CORRUPT_RECORD"
ALICE_POST_CORRUPT_RECOVERY="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_POST_CORRUPT_TRANSFER_COUNT="$(transfer_count "$ALICE_POST_CORRUPT_RECOVERY")"
require_eq "$ALICE_POST_CORRUPT_TRANSFER_COUNT" "$ALICE_PRE_FAILURE_TRANSFER_COUNT" "Alice transfer count after corrupt record removal"
sync_all_and_assert_invariants "post failed-batch and corrupt-record recovery invariants"

echo "[7/12] Alice pays Bob through HTTP, Bob accepts, Alice finalises and broadcasts"
BOB_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "100" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
BOB_INVOICE="$(api_post "$BOB_API" "/invoices" "$BOB_INVOICE_PAYLOAD")"
BOB_INVOICE_TEXT="$(json_get invoice <<<"$BOB_INVOICE")"
PAY_BOB_PAYLOAD="$(node -e 'const [invoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoice, feeTondi, carrierValueTondi }));' "$BOB_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
ALICE_TO_BOB="$(api_post "$ALICE_API" "/transfers/pay" "$PAY_BOB_PAYLOAD")"
ALICE_TO_BOB_CONSIGNMENT="$(transfer_contract_field "$ALICE_TO_BOB" "$CONTRACT_ID" consignmentRef)"
ALICE_TO_BOB_PSTT="$(json_get psttRef <<<"$ALICE_TO_BOB")"
ALICE_TO_BOB_CONSIGNMENT_PATH="$(abs_ref "$ALICE_DIR" "$ALICE_TO_BOB_CONSIGNMENT")"
ALICE_TO_BOB_PSTT_PATH="$(abs_ref "$ALICE_DIR" "$ALICE_TO_BOB_PSTT")"
require_file_exists "$ALICE_TO_BOB_CONSIGNMENT_PATH" "Alice prepared transfer consignment before restart"
require_file_exists "$ALICE_TO_BOB_PSTT_PATH" "Alice prepared transfer PSTT before restart"
restart_http_service "Alice after pay before finalise" "$ALICE_API" "$ALICE_DIR" "$ALICE_HTTP_PORT" "$DATA_ROOT/alice-http.restart-pay.log" ALICE_HTTP_PID
ALICE_AFTER_PAY_RESTART="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_AFTER_PAY_TRANSFER_COUNT="$(transfer_count "$ALICE_AFTER_PAY_RESTART")"
require_eq "$ALICE_AFTER_PAY_TRANSFER_COUNT" "$((ALICE_POST_FAILURE_TRANSFER_COUNT + 1))" "Alice transfer count after pay restart"
require_file_exists "$ALICE_TO_BOB_CONSIGNMENT_PATH" "Alice prepared transfer consignment after restart"
require_file_exists "$ALICE_TO_BOB_PSTT_PATH" "Alice prepared transfer PSTT after restart"
PAY_CAROL_WHILE_ALICE_LIVE_PAYLOAD="$(node -e 'const [invoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoice, feeTondi, carrierValueTondi }));' "$CAROL_SMALL_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
echo "  Concurrently rejecting Alice pay/pay/sync while a prepared transfer is live"
IN_FLIGHT_PAY_A="$DATA_ROOT/alice-inflight-pay-a.response"
IN_FLIGHT_PAY_B="$DATA_ROOT/alice-inflight-pay-b.response"
IN_FLIGHT_PAY_SYNC="$DATA_ROOT/alice-inflight-pay-sync.response"
curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$PAY_CAROL_WHILE_ALICE_LIVE_PAYLOAD" "$ALICE_API/transfers/pay" >"$IN_FLIGHT_PAY_A" &
IN_FLIGHT_PAY_A_PID=$!
curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$PAY_CAROL_WHILE_ALICE_LIVE_PAYLOAD" "$ALICE_API/transfers/pay" >"$IN_FLIGHT_PAY_B" &
IN_FLIGHT_PAY_B_PID=$!
curl -sS -w $'\n%{http_code}' -X POST "$ALICE_API/wallet/sync" >"$IN_FLIGHT_PAY_SYNC" &
IN_FLIGHT_PAY_SYNC_PID=$!
wait "$IN_FLIGHT_PAY_A_PID"
wait "$IN_FLIGHT_PAY_B_PID"
wait "$IN_FLIGHT_PAY_SYNC_PID"
IN_FLIGHT_PAY_A_STATUS="$(response_status_file "$IN_FLIGHT_PAY_A")"
IN_FLIGHT_PAY_B_STATUS="$(response_status_file "$IN_FLIGHT_PAY_B")"
if (( IN_FLIGHT_PAY_A_STATUS >= 200 && IN_FLIGHT_PAY_A_STATUS < 300 )); then
  echo "in-flight concurrent pay A: expected HTTP failure, got status $IN_FLIGHT_PAY_A_STATUS" >&2
  response_body_file "$IN_FLIGHT_PAY_A" >&2
  exit 1
fi
if (( IN_FLIGHT_PAY_B_STATUS >= 200 && IN_FLIGHT_PAY_B_STATUS < 300 )); then
  echo "in-flight concurrent pay B: expected HTTP failure, got status $IN_FLIGHT_PAY_B_STATUS" >&2
  response_body_file "$IN_FLIGHT_PAY_B" >&2
  exit 1
fi
if [[ "$(response_body_file "$IN_FLIGHT_PAY_A")" != *"without a broadcastTxid"* || "$(response_body_file "$IN_FLIGHT_PAY_B")" != *"without a broadcastTxid"* ]]; then
  echo "in-flight concurrent pay: expected live outgoing transfer conflicts, got:" >&2
  response_body_file "$IN_FLIGHT_PAY_A" >&2
  response_body_file "$IN_FLIGHT_PAY_B" >&2
  exit 1
fi
require_response_success "$IN_FLIGHT_PAY_SYNC" "in-flight concurrent Alice sync"
ALICE_AFTER_IN_FLIGHT_PAY_FAILURE="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_AFTER_IN_FLIGHT_PAY_TRANSFER_COUNT="$(transfer_count "$ALICE_AFTER_IN_FLIGHT_PAY_FAILURE")"
require_eq "$ALICE_AFTER_IN_FLIGHT_PAY_TRANSFER_COUNT" "$ALICE_AFTER_PAY_TRANSFER_COUNT" "Alice transfer count after in-flight double-pay rejection"
mv "$ALICE_TO_BOB_PSTT_PATH" "$ALICE_TO_BOB_PSTT_PATH.missing"
MISSING_PSTT_ERROR="$(api_post_expect_failure "$ALICE_API" "/transfers/finalise" "$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: false }));' "$ALICE_TO_BOB_PSTT")" "missing PSTT finalise")"
if [[ "$MISSING_PSTT_ERROR" != *"unable to open PSTT"* ]]; then
  echo "missing PSTT finalise: expected explicit PSTT error, got:" >&2
  printf '%s\n' "$MISSING_PSTT_ERROR" >&2
  exit 1
fi
mv "$ALICE_TO_BOB_PSTT_PATH.missing" "$ALICE_TO_BOB_PSTT_PATH"
ALICE_AFTER_MISSING_PSTT_FAILURE="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_AFTER_MISSING_PSTT_TRANSFER_COUNT="$(transfer_count "$ALICE_AFTER_MISSING_PSTT_FAILURE")"
require_eq "$ALICE_AFTER_MISSING_PSTT_TRANSFER_COUNT" "$ALICE_AFTER_PAY_TRANSFER_COUNT" "Alice transfer count after missing PSTT finalise failure"
BOB_ACCEPT_FIRST="$(api_post "$BOB_API" "/transfers/accept" "$(json_field_payload consignmentRef "$(abs_ref "$ALICE_DIR" "$ALICE_TO_BOB_CONSIGNMENT")")")"
BOB_ACCEPT_SECOND="$(api_post "$BOB_API" "/transfers/accept" "$(json_field_payload consignmentRef "$(abs_ref "$ALICE_DIR" "$ALICE_TO_BOB_CONSIGNMENT")")")"
require_eq "$(json_get id <<<"$BOB_ACCEPT_SECOND")" "$(json_get id <<<"$BOB_ACCEPT_FIRST")" "Repeated Bob accept transfer id"
if [[ "$TENTATIVE_CHAIN" == "1" ]]; then
  echo "  Pausing devnet miner before first transfer broadcast to keep it tentative"
  stop_miner
fi
ALICE_TO_BOB_SIGNED="$(api_post "$ALICE_API" "/transfers/finalise" "$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: false }));' "$ALICE_TO_BOB_PSTT")")"
require_eq "$(json_get state <<<"$ALICE_TO_BOB_SIGNED")" "finalised" "Alice local finalise state before restart"
ALICE_TO_BOB_TX_PATH="${ALICE_TO_BOB_PSTT_PATH%.pstt}.tx"
require_file_exists "$ALICE_TO_BOB_TX_PATH" "Alice extracted transaction after local finalise"
restart_http_service "Alice after local finalise before broadcast" "$ALICE_API" "$ALICE_DIR" "$ALICE_HTTP_PORT" "$DATA_ROOT/alice-http.restart-finalise.log" ALICE_HTTP_PID
ALICE_AFTER_FINALISE_RESTART="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_AFTER_FINALISE_TRANSFER_COUNT="$(transfer_count "$ALICE_AFTER_FINALISE_RESTART")"
require_eq "$ALICE_AFTER_FINALISE_TRANSFER_COUNT" "$ALICE_AFTER_PAY_TRANSFER_COUNT" "Alice transfer count after local-finalise restart"
require_file_exists "$ALICE_TO_BOB_TX_PATH" "Alice extracted transaction after local-finalise restart"
echo "  Concurrently syncing Alice and broadcasting the same finalised PSTT twice"
CONCURRENT_FINALISE_PAYLOAD="$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: true }));' "$ALICE_TO_BOB_PSTT")"
CONCURRENT_FINALISE_A="$DATA_ROOT/alice-finalise-concurrent-a.response"
CONCURRENT_FINALISE_B="$DATA_ROOT/alice-finalise-concurrent-b.response"
CONCURRENT_SYNC="$DATA_ROOT/alice-sync-concurrent.response"
curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$CONCURRENT_FINALISE_PAYLOAD" "$ALICE_API/transfers/finalise" >"$CONCURRENT_FINALISE_A" &
FINALISE_A_PID=$!
curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$CONCURRENT_FINALISE_PAYLOAD" "$ALICE_API/transfers/finalise" >"$CONCURRENT_FINALISE_B" &
FINALISE_B_PID=$!
curl -sS -w $'\n%{http_code}' -X POST "$ALICE_API/wallet/sync" >"$CONCURRENT_SYNC" &
SYNC_PID=$!
wait "$FINALISE_A_PID"
wait "$FINALISE_B_PID"
wait "$SYNC_PID"
require_response_success "$CONCURRENT_FINALISE_A" "concurrent Alice finalise A"
require_response_success "$CONCURRENT_FINALISE_B" "concurrent Alice finalise B"
require_response_success "$CONCURRENT_SYNC" "concurrent Alice sync"
ALICE_TO_BOB_FINAL="$(response_body_file "$CONCURRENT_FINALISE_A")"
ALICE_TO_BOB_FINAL_B="$(response_body_file "$CONCURRENT_FINALISE_B")"
require_eq "$(json_get broadcastTxid <<<"$ALICE_TO_BOB_FINAL_B")" "$(json_get broadcastTxid <<<"$ALICE_TO_BOB_FINAL")" "Concurrent Alice finalise broadcast txid"
ALICE_AFTER_CONCURRENT_FINALISE="$(api_post_empty "$ALICE_API" "/wallet/sync")"
ALICE_AFTER_CONCURRENT_FINALISE_TRANSFER_COUNT="$(transfer_count "$ALICE_AFTER_CONCURRENT_FINALISE")"
require_eq "$ALICE_AFTER_CONCURRENT_FINALISE_TRANSFER_COUNT" "$ALICE_AFTER_PAY_TRANSFER_COUNT" "Alice transfer count after concurrent finalise/sync"
ALICE_TO_BOB_TRANSFER_ID="$(json_get id <<<"$ALICE_TO_BOB_FINAL")"
TXID_1="$(json_get broadcastTxid <<<"$ALICE_TO_BOB_FINAL")"
if [[ "$TENTATIVE_CHAIN" == "1" ]]; then
  wait_for_tx_status "$TXID_1" mempool >/dev/null
  echo "  Alice -> Bob txid is tentative in mempool: $TXID_1"
else
  wait_for_tx_accepted "$TXID_1" >/dev/null
  echo "  Alice -> Bob txid accepted: $TXID_1"
fi
ALICE_TO_BOB_RECORD="$ALICE_DIR/transfers/$ALICE_TO_BOB_TRANSFER_ID.json"
node -e '
  const fs = require("fs");
  const path = process.argv[1];
  const record = JSON.parse(fs.readFileSync(path, "utf8"));
  delete record.broadcastTxid;
  record.state = "finalised";
  record.validation = "pending";
  fs.writeFileSync(path, JSON.stringify(record, null, 2));
' "$ALICE_TO_BOB_RECORD"
restart_http_service "Alice after broadcast before record update" "$ALICE_API" "$ALICE_DIR" "$ALICE_HTTP_PORT" "$DATA_ROOT/alice-http.restart-broadcast-record.log" ALICE_HTTP_PID
ALICE_AFTER_BROADCAST_RECORD_RECOVERY="$(api_post_empty "$ALICE_API" "/wallet/sync")"
require_eq "$(dashboard_transfer_field "$ALICE_AFTER_BROADCAST_RECORD_RECOVERY" "$ALICE_TO_BOB_TRANSFER_ID" state)" "broadcast" "Recovered Alice transfer state after lost broadcast record update"
require_eq "$(dashboard_transfer_field "$ALICE_AFTER_BROADCAST_RECORD_RECOVERY" "$ALICE_TO_BOB_TRANSFER_ID" broadcastTxid)" "$TXID_1" "Recovered Alice broadcast txid after lost record update"
ALICE_TO_BOB_FINAL_REPEAT="$(api_post "$ALICE_API" "/transfers/finalise" "$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: true }));' "$ALICE_TO_BOB_PSTT")")"
require_eq "$(json_get broadcastTxid <<<"$ALICE_TO_BOB_FINAL_REPEAT")" "$TXID_1" "Repeated Alice finalise broadcast txid"

api_post_empty "$ALICE_API" "/wallet/sync" >/dev/null
BOB_AFTER_FIRST="$(api_post_empty "$BOB_API" "/wallet/sync")"
BOB_BALANCE_1="$(asset_balance "$BOB_AFTER_FIRST" "$CONTRACT_ID")"
require_eq "$BOB_BALANCE_1" "100" "Bob balance after first transfer"

echo "[8/12] Bob spends received asset onward to Carol through HTTP"
CAROL_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "40" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
CAROL_INVOICE="$(api_post "$CAROL_API" "/invoices" "$CAROL_INVOICE_PAYLOAD")"
CAROL_INVOICE_TEXT="$(json_get invoice <<<"$CAROL_INVOICE")"
PAY_CAROL_PAYLOAD="$(node -e 'const [invoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoice, feeTondi, carrierValueTondi }));' "$CAROL_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
BOB_TO_CAROL="$(api_post "$BOB_API" "/transfers/pay" "$PAY_CAROL_PAYLOAD")"
BOB_TO_CAROL_CONSIGNMENT="$(transfer_contract_field "$BOB_TO_CAROL" "$CONTRACT_ID" consignmentRef)"
BOB_TO_CAROL_PSTT="$(json_get psttRef <<<"$BOB_TO_CAROL")"
api_post "$CAROL_API" "/transfers/accept" "$(json_field_payload consignmentRef "$(abs_ref "$BOB_DIR" "$BOB_TO_CAROL_CONSIGNMENT")")" >/dev/null
BOB_TO_CAROL_FINAL="$(api_post "$BOB_API" "/transfers/finalise" "$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: true }));' "$BOB_TO_CAROL_PSTT")")"
TXID_2="$(json_get broadcastTxid <<<"$BOB_TO_CAROL_FINAL")"
if [[ "$TXID_1" == "$TXID_2" ]]; then
  echo "Second transfer reused the first broadcast txid" >&2
  exit 1
fi
if [[ "$TENTATIVE_CHAIN" == "1" ]]; then
  wait_for_tx_status "$TXID_2" mempool >/dev/null
  echo "  Bob -> Carol txid is tentative in mempool: $TXID_2"
  BOB_BEFORE_TENTATIVE_SPEND_RETRY="$(api_post_empty "$BOB_API" "/wallet/sync")"
  BOB_BEFORE_TENTATIVE_SPEND_RETRY_TRANSFER_COUNT="$(transfer_count "$BOB_BEFORE_TENTATIVE_SPEND_RETRY")"
  ALICE_RETRY_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "1" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
  ALICE_RETRY_INVOICE="$(api_post "$ALICE_API" "/invoices" "$ALICE_RETRY_INVOICE_PAYLOAD")"
  ALICE_RETRY_INVOICE_TEXT="$(json_get invoice <<<"$ALICE_RETRY_INVOICE")"
  BOB_RETRY_WHILE_TENTATIVE_PAYLOAD="$(node -e 'const [invoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoice, feeTondi, carrierValueTondi }));' "$ALICE_RETRY_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
  BOB_TENTATIVE_SPEND_ERROR="$(api_post_expect_failure "$BOB_API" "/transfers/pay" "$BOB_RETRY_WHILE_TENTATIVE_PAYLOAD" "Bob second spend while first onward spend is tentative")"
  if [[ "$BOB_TENTATIVE_SPEND_ERROR" != *"still tentative"* ]]; then
    echo "Bob tentative second spend: expected Tondi tentative conflict, got:" >&2
    printf '%s\n' "$BOB_TENTATIVE_SPEND_ERROR" >&2
    exit 1
  fi
  BOB_AFTER_TENTATIVE_SPEND_RETRY="$(api_post_empty "$BOB_API" "/wallet/sync")"
  BOB_AFTER_TENTATIVE_SPEND_RETRY_TRANSFER_COUNT="$(transfer_count "$BOB_AFTER_TENTATIVE_SPEND_RETRY")"
  require_eq "$BOB_AFTER_TENTATIVE_SPEND_RETRY_TRANSFER_COUNT" "$BOB_BEFORE_TENTATIVE_SPEND_RETRY_TRANSFER_COUNT" "Bob transfer count after rejected tentative double spend"
  echo "  Resuming devnet miner and requiring both tentative-chain transactions to become accepted"
  start_miner
  wait_for_tx_accepted "$TXID_1" >/dev/null
  wait_for_tx_accepted "$TXID_2" >/dev/null
  wait_for_asset_balance "$ALICE_API" "$CONTRACT_ID" "999900" "Alice post-tentative TEST balance" >/dev/null
  wait_for_asset_balance "$BOB_API" "$CONTRACT_ID" "60" "Bob post-tentative TEST balance" >/dev/null
  wait_for_asset_balance "$CAROL_API" "$CONTRACT_ID" "40" "Carol post-tentative TEST balance" >/dev/null
else
  wait_for_tx_accepted "$TXID_2" >/dev/null
fi
echo "  Bob -> Carol txid accepted: $TXID_2"
sync_all_and_assert_invariants "post Bob onward spend invariants"

echo "  Verifying mixed imported/unimported contract batch fails atomically"
BOB_PRE_UNKNOWN_CONTRACT_BATCH="$(api_post_empty "$BOB_API" "/wallet/sync")"
BOB_PRE_UNKNOWN_CONTRACT_BATCH_COUNT="$(transfer_count "$BOB_PRE_UNKNOWN_CONTRACT_BATCH")"
ALICE_UNKNOWN_BATCH_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "1" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
CAROL_UNKNOWN_BATCH_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "1" }));' "$CONTRACT_ID_2" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
ALICE_UNKNOWN_BATCH_INVOICE="$(api_post "$ALICE_API" "/invoices" "$ALICE_UNKNOWN_BATCH_INVOICE_PAYLOAD")"
CAROL_UNKNOWN_BATCH_INVOICE="$(api_post "$CAROL_API" "/invoices" "$CAROL_UNKNOWN_BATCH_INVOICE_PAYLOAD")"
ALICE_UNKNOWN_BATCH_INVOICE_TEXT="$(json_get invoice <<<"$ALICE_UNKNOWN_BATCH_INVOICE")"
CAROL_UNKNOWN_BATCH_INVOICE_TEXT="$(json_get invoice <<<"$CAROL_UNKNOWN_BATCH_INVOICE")"
UNKNOWN_CONTRACT_BATCH_PAYLOAD="$(node -e 'const [testInvoice, cashInvoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoices: [testInvoice, cashInvoice], feeTondi, carrierValueTondi }));' "$ALICE_UNKNOWN_BATCH_INVOICE_TEXT" "$CAROL_UNKNOWN_BATCH_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
UNKNOWN_CONTRACT_BATCH_ERROR="$(api_post_expect_failure "$BOB_API" "/transfers/pay-batch" "$UNKNOWN_CONTRACT_BATCH_PAYLOAD" "batch with unimported CASH contract")"
if [[ "$UNKNOWN_CONTRACT_BATCH_ERROR" != *"unknown RGB contract"* ]]; then
  echo "batch with unimported CASH contract: expected explicit unknown-contract error, got:" >&2
  printf '%s\n' "$UNKNOWN_CONTRACT_BATCH_ERROR" >&2
  exit 1
fi
BOB_POST_UNKNOWN_CONTRACT_BATCH="$(api_post_empty "$BOB_API" "/wallet/sync")"
BOB_POST_UNKNOWN_CONTRACT_BATCH_COUNT="$(transfer_count "$BOB_POST_UNKNOWN_CONTRACT_BATCH")"
require_eq "$BOB_POST_UNKNOWN_CONTRACT_BATCH_COUNT" "$BOB_PRE_UNKNOWN_CONTRACT_BATCH_COUNT" "Bob transfer count after unimported-contract batch failure"
api_post "$BOB_API" "/contracts/accept" "$(json_field_payload consignmentName "$CONTRACT_PATH_2")" >/dev/null

echo "  Verifying unknown and removed Tondi statuses reject stale broadcast records without locking Alice"
ALICE_UNKNOWN_STATUS_TRANSFER_ID="transfer-unknown-status-rollback"
ALICE_UNKNOWN_STATUS_RECORD="$ALICE_DIR/transfers/$ALICE_UNKNOWN_STATUS_TRANSFER_ID.json"
ALICE_UNKNOWN_STATUS_TXID="1111111111111111111111111111111111111111111111111111111111111111"
write_synthetic_broadcast_transfer_record "$ALICE_UNKNOWN_STATUS_RECORD" "$ALICE_UNKNOWN_STATUS_TRANSFER_ID" "$CONTRACT_ID" "$ALICE_TO_BOB_CONSIGNMENT" "$ALICE_UNKNOWN_STATUS_TXID" "transfers/unknown-status-rollback.pstt"
ALICE_UNKNOWN_STATUS_DASHBOARD="$(api_post_empty "$ALICE_API" "/wallet/sync")"
require_eq "$(dashboard_transfer_field "$ALICE_UNKNOWN_STATUS_DASHBOARD" "$ALICE_UNKNOWN_STATUS_TRANSFER_ID" validation)" "rejected" "Unknown-status transfer validation after sync"

ALICE_REMOVED_STATUS_TRANSFER_ID="transfer-removed-status-rollback"
ALICE_REMOVED_STATUS_RECORD="$ALICE_DIR/transfers/$ALICE_REMOVED_STATUS_TRANSFER_ID.json"
ALICE_REMOVED_STATUS_TXID="2222222222222222222222222222222222222222222222222222222222222222"
printf '%s\n' "$ALICE_REMOVED_STATUS_TXID" >>"$TONDI_FORCE_REMOVED_TXIDS_FILE"
ALICE_REMOVED_STATUS_QUERY="$(wait_for_tx_status "$ALICE_REMOVED_STATUS_TXID" "removed")"
require_eq "$(kv_field status <<<"$ALICE_REMOVED_STATUS_QUERY")" "removed" "Forced removed Tondi status"
write_synthetic_broadcast_transfer_record "$ALICE_REMOVED_STATUS_RECORD" "$ALICE_REMOVED_STATUS_TRANSFER_ID" "$CONTRACT_ID" "$ALICE_TO_BOB_CONSIGNMENT" "$ALICE_REMOVED_STATUS_TXID" "transfers/removed-status-rollback.pstt"
ALICE_REMOVED_STATUS_DASHBOARD="$(api_post_empty "$ALICE_API" "/wallet/sync")"
require_eq "$(dashboard_transfer_field "$ALICE_REMOVED_STATUS_DASHBOARD" "$ALICE_REMOVED_STATUS_TRANSFER_ID" validation)" "rejected" "Removed-status transfer validation after sync"
sync_all_and_assert_invariants "post unknown/removed-status rollback recovery invariants"

echo "[9/12] Alice pays Bob and Carol through one same-contract MIMO transfer"
api_post_empty "$ALICE_API" "/wallet/sync" >/dev/null
api_post_empty "$BOB_API" "/wallet/sync" >/dev/null
api_post_empty "$CAROL_API" "/wallet/sync" >/dev/null

BOB_MIMO_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "50" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
CAROL_MIMO_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "50" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
BOB_MIMO_INVOICE="$(api_post "$BOB_API" "/invoices" "$BOB_MIMO_INVOICE_PAYLOAD")"
CAROL_MIMO_INVOICE="$(api_post "$CAROL_API" "/invoices" "$CAROL_MIMO_INVOICE_PAYLOAD")"
BOB_MIMO_INVOICE_TEXT="$(json_get invoice <<<"$BOB_MIMO_INVOICE")"
CAROL_MIMO_INVOICE_TEXT="$(json_get invoice <<<"$CAROL_MIMO_INVOICE")"
PAY_MIMO_PAYLOAD="$(node -e 'const [bobInvoice, carolInvoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoices: [bobInvoice, carolInvoice], feeTondi, carrierValueTondi }));' "$BOB_MIMO_INVOICE_TEXT" "$CAROL_MIMO_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
ALICE_MIMO="$(api_post "$ALICE_API" "/transfers/pay-batch" "$PAY_MIMO_PAYLOAD")"
ALICE_MIMO_AMOUNT="$(transfer_contract_field "$ALICE_MIMO" "$CONTRACT_ID" amount)"
require_eq "$ALICE_MIMO_AMOUNT" "100" "Alice MIMO transfer amount"
rm -f "$ALICE_UNKNOWN_STATUS_RECORD" "$ALICE_REMOVED_STATUS_RECORD"
ALICE_MIMO_CONSIGNMENT="$(transfer_contract_field "$ALICE_MIMO" "$CONTRACT_ID" consignmentRef)"
ALICE_MIMO_PSTT="$(json_get psttRef <<<"$ALICE_MIMO")"
ALICE_MIMO_CONSIGNMENT_PATH="$(abs_ref "$ALICE_DIR" "$ALICE_MIMO_CONSIGNMENT")"
echo "  Broadcasting same-contract MIMO before receiver accept to test out-of-order sync"
stop_miner
ALICE_MIMO_FINAL="$(api_post "$ALICE_API" "/transfers/finalise" "$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: true }));' "$ALICE_MIMO_PSTT")")"
TXID_3="$(json_get broadcastTxid <<<"$ALICE_MIMO_FINAL")"
if [[ "$TXID_3" == "$TXID_1" || "$TXID_3" == "$TXID_2" ]]; then
  echo "MIMO transfer reused an earlier broadcast txid" >&2
  exit 1
fi
wait_for_tx_status "$TXID_3" mempool >/dev/null
api_post_empty "$ALICE_API" "/wallet/sync" >/dev/null
CAROL_MIMO_PRE_ACCEPT_SYNC="$(api_post_empty "$CAROL_API" "/wallet/sync")"
BOB_MIMO_PRE_ACCEPT_SYNC="$(api_post_empty "$BOB_API" "/wallet/sync")"
require_eq "$(asset_balance "$CAROL_MIMO_PRE_ACCEPT_SYNC" "$CONTRACT_ID")" "40" "Carol pre-accept out-of-order MIMO balance"
require_eq "$(asset_balance "$BOB_MIMO_PRE_ACCEPT_SYNC" "$CONTRACT_ID")" "60" "Bob pre-accept out-of-order MIMO balance"
start_miner
wait_for_tx_accepted "$TXID_3" >/dev/null
api_post_empty "$ALICE_API" "/wallet/sync" >/dev/null
echo "  Concurrently accepting the same MIMO consignment on Bob and Carol"
CONCURRENT_MIMO_ACCEPT_PAYLOAD="$(json_field_payload consignmentRef "$ALICE_MIMO_CONSIGNMENT_PATH")"
CONCURRENT_BOB_ACCEPT="$DATA_ROOT/bob-mimo-accept-concurrent.response"
CONCURRENT_CAROL_ACCEPT="$DATA_ROOT/carol-mimo-accept-concurrent.response"
curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$CONCURRENT_MIMO_ACCEPT_PAYLOAD" "$BOB_API/transfers/accept" >"$CONCURRENT_BOB_ACCEPT" &
BOB_MIMO_ACCEPT_PID=$!
curl -sS -w $'\n%{http_code}' -H "content-type: application/json" -d "$CONCURRENT_MIMO_ACCEPT_PAYLOAD" "$CAROL_API/transfers/accept" >"$CONCURRENT_CAROL_ACCEPT" &
CAROL_MIMO_ACCEPT_PID=$!
wait "$BOB_MIMO_ACCEPT_PID"
wait "$CAROL_MIMO_ACCEPT_PID"
require_response_success "$CONCURRENT_BOB_ACCEPT" "concurrent Bob MIMO accept"
require_response_success "$CONCURRENT_CAROL_ACCEPT" "concurrent Carol MIMO accept"
wait_for_asset_balance "$CAROL_API" "$CONTRACT_ID" "90" "Carol post-accept out-of-order MIMO balance" >/dev/null
wait_for_asset_balance "$BOB_API" "$CONTRACT_ID" "110" "Bob post-accept out-of-order MIMO balance" >/dev/null
sync_all_and_assert_invariants "post same-contract MIMO accept invariants"
echo "  Alice -> Bob+Carol MIMO txid accepted: $TXID_3"

echo "[10/12] Alice pays Bob TEST and Carol CASH through one multi-contract MIMO transfer"
api_post_empty "$ALICE_API" "/wallet/sync" >/dev/null
api_post_empty "$BOB_API" "/wallet/sync" >/dev/null
api_post_empty "$CAROL_API" "/wallet/sync" >/dev/null

BOB_MULTI_TEST_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "40" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
CAROL_MULTI_CASH_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "60" }));' "$CONTRACT_ID_2" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
BOB_MULTI_TEST_INVOICE="$(api_post "$BOB_API" "/invoices" "$BOB_MULTI_TEST_INVOICE_PAYLOAD")"
CAROL_MULTI_CASH_INVOICE="$(api_post "$CAROL_API" "/invoices" "$CAROL_MULTI_CASH_INVOICE_PAYLOAD")"
BOB_MULTI_TEST_INVOICE_TEXT="$(json_get invoice <<<"$BOB_MULTI_TEST_INVOICE")"
CAROL_MULTI_CASH_INVOICE_TEXT="$(json_get invoice <<<"$CAROL_MULTI_CASH_INVOICE")"
PAY_MULTI_CONTRACT_PAYLOAD="$(node -e 'const [testInvoice, cashInvoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoices: [testInvoice, cashInvoice], feeTondi, carrierValueTondi }));' "$BOB_MULTI_TEST_INVOICE_TEXT" "$CAROL_MULTI_CASH_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
ALICE_MULTI_CONTRACT="$(api_post "$ALICE_API" "/transfers/pay-batch" "$PAY_MULTI_CONTRACT_PAYLOAD")"
ALICE_MULTI_TEST_AMOUNT="$(transfer_contract_field "$ALICE_MULTI_CONTRACT" "$CONTRACT_ID" amount)"
ALICE_MULTI_CASH_AMOUNT="$(transfer_contract_field "$ALICE_MULTI_CONTRACT" "$CONTRACT_ID_2" amount)"
require_eq "$ALICE_MULTI_TEST_AMOUNT" "40" "Alice multi-contract TEST amount"
require_eq "$ALICE_MULTI_CASH_AMOUNT" "60" "Alice multi-contract CASH amount"
ALICE_MULTI_TEST_CONSIGNMENT="$(transfer_contract_field "$ALICE_MULTI_CONTRACT" "$CONTRACT_ID" consignmentRef)"
ALICE_MULTI_CASH_CONSIGNMENT="$(transfer_contract_field "$ALICE_MULTI_CONTRACT" "$CONTRACT_ID_2" consignmentRef)"
ALICE_MULTI_PSTT="$(json_get psttRef <<<"$ALICE_MULTI_CONTRACT")"
api_post "$BOB_API" "/transfers/accept" "$(json_field_payload consignmentRef "$(abs_ref "$ALICE_DIR" "$ALICE_MULTI_TEST_CONSIGNMENT")")" >/dev/null
BOB_BEFORE_WRONG_RECEIVER_ACCEPT="$(api_post_empty "$BOB_API" "/wallet/sync")"
BOB_BEFORE_WRONG_RECEIVER_TRANSFER_COUNT="$(transfer_count "$BOB_BEFORE_WRONG_RECEIVER_ACCEPT")"
WRONG_RECEIVER_ACCEPT_ERROR="$(api_post_expect_failure "$BOB_API" "/transfers/accept" "$(json_field_payload consignmentRef "$(abs_ref "$ALICE_DIR" "$ALICE_MULTI_CASH_CONSIGNMENT")")" "wrong receiver accept for multi-contract batch")"
if [[ "$WRONG_RECEIVER_ACCEPT_ERROR" != *"did not add wallet-owned RGB state"* ]]; then
  echo "wrong receiver accept: expected explicit non-owned state error, got:" >&2
  printf '%s\n' "$WRONG_RECEIVER_ACCEPT_ERROR" >&2
  exit 1
fi
BOB_AFTER_WRONG_RECEIVER_ACCEPT="$(api_post_empty "$BOB_API" "/wallet/sync")"
BOB_AFTER_WRONG_RECEIVER_TRANSFER_COUNT="$(transfer_count "$BOB_AFTER_WRONG_RECEIVER_ACCEPT")"
require_eq "$BOB_AFTER_WRONG_RECEIVER_TRANSFER_COUNT" "$BOB_BEFORE_WRONG_RECEIVER_TRANSFER_COUNT" "Bob transfer count after wrong receiver accept failure"
api_post "$CAROL_API" "/transfers/accept" "$(json_field_payload consignmentRef "$(abs_ref "$ALICE_DIR" "$ALICE_MULTI_CASH_CONSIGNMENT")")" >/dev/null
ALICE_MULTI_FINAL="$(api_post "$ALICE_API" "/transfers/finalise" "$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: true }));' "$ALICE_MULTI_PSTT")")"
TXID_4="$(json_get broadcastTxid <<<"$ALICE_MULTI_FINAL")"
if [[ "$TXID_4" == "$TXID_1" || "$TXID_4" == "$TXID_2" || "$TXID_4" == "$TXID_3" ]]; then
  echo "Multi-contract MIMO transfer reused an earlier broadcast txid" >&2
  exit 1
fi
wait_for_tx_accepted "$TXID_4" >/dev/null
sync_all_and_assert_invariants "post multi-contract MIMO invariants"
echo "  Alice -> Bob TEST + Carol CASH MIMO txid accepted: $TXID_4"

echo "[11/12] Bob aggregates split TEST states and pays Alice plus Carol through one MIMO transfer"
api_post_empty "$ALICE_API" "/wallet/sync" >/dev/null
api_post_empty "$BOB_API" "/wallet/sync" >/dev/null
api_post_empty "$CAROL_API" "/wallet/sync" >/dev/null
echo "  Bob must aggregate split TEST states of 60 + 50 + 40"

ALICE_MULTI_INPUT_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "70" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
CAROL_MULTI_INPUT_INVOICE_PAYLOAD="$(node -e 'const [contractId, apiName, methodName, stateName] = process.argv.slice(1); console.log(JSON.stringify({ contractId, apiName, methodName, stateName, amount: "65" }));' "$CONTRACT_ID" "$API_NAME" "$TRANSFER_METHOD" "$STATE_NAME")"
ALICE_MULTI_INPUT_INVOICE="$(api_post "$ALICE_API" "/invoices" "$ALICE_MULTI_INPUT_INVOICE_PAYLOAD")"
CAROL_MULTI_INPUT_INVOICE="$(api_post "$CAROL_API" "/invoices" "$CAROL_MULTI_INPUT_INVOICE_PAYLOAD")"
ALICE_MULTI_INPUT_INVOICE_TEXT="$(json_get invoice <<<"$ALICE_MULTI_INPUT_INVOICE")"
CAROL_MULTI_INPUT_INVOICE_TEXT="$(json_get invoice <<<"$CAROL_MULTI_INPUT_INVOICE")"
PAY_MULTI_INPUT_PAYLOAD="$(node -e 'const [aliceInvoice, carolInvoice, feeTondi, carrierValueTondi] = process.argv.slice(1); console.log(JSON.stringify({ invoices: [aliceInvoice, carolInvoice], feeTondi, carrierValueTondi }));' "$ALICE_MULTI_INPUT_INVOICE_TEXT" "$CAROL_MULTI_INPUT_INVOICE_TEXT" "$TRANSFER_FEE_TONDI" "$TRANSFER_CARRIER_TONDI")"
BOB_MULTI_INPUT="$(api_post "$BOB_API" "/transfers/pay-batch" "$PAY_MULTI_INPUT_PAYLOAD")"
BOB_MULTI_INPUT_AMOUNT="$(transfer_contract_field "$BOB_MULTI_INPUT" "$CONTRACT_ID" amount)"
require_eq "$BOB_MULTI_INPUT_AMOUNT" "135" "Bob multi-input TEST amount"
BOB_MULTI_INPUT_CONSIGNMENT="$(transfer_contract_field "$BOB_MULTI_INPUT" "$CONTRACT_ID" consignmentRef)"
BOB_MULTI_INPUT_PSTT="$(json_get psttRef <<<"$BOB_MULTI_INPUT")"
BOB_MULTI_INPUT_CONSIGNMENT_PATH="$(abs_ref "$BOB_DIR" "$BOB_MULTI_INPUT_CONSIGNMENT")"
BOB_MULTI_INPUT_PSTT_PATH="$(abs_ref "$BOB_DIR" "$BOB_MULTI_INPUT_PSTT")"
BOB_MULTI_INPUT_PSTT_INFO="$(cargo run --quiet -p rgb-wallet --example inspect_pstt -- "$BOB_MULTI_INPUT_PSTT_PATH")"
require_eq "$(kv_field input_count <<<"$BOB_MULTI_INPUT_PSTT_INFO")" "3" "Bob multi-input PSTT input count"
require_eq "$(kv_field unique_input_count <<<"$BOB_MULTI_INPUT_PSTT_INFO")" "3" "Bob multi-input PSTT unique input count"
require_eq "$(kv_field change_output_count <<<"$BOB_MULTI_INPUT_PSTT_INFO")" "1" "Bob multi-input PSTT change output count"
BOB_MULTI_INPUT_CHANGE_VOUT="$(kv_field change_vout <<<"$BOB_MULTI_INPUT_PSTT_INFO")"
api_post "$ALICE_API" "/transfers/accept" "$(json_field_payload consignmentRef "$BOB_MULTI_INPUT_CONSIGNMENT_PATH")" >/dev/null
api_post "$CAROL_API" "/transfers/accept" "$(json_field_payload consignmentRef "$BOB_MULTI_INPUT_CONSIGNMENT_PATH")" >/dev/null
BOB_MULTI_INPUT_FINAL="$(api_post "$BOB_API" "/transfers/finalise" "$(node -e 'const [psttRef] = process.argv.slice(1); console.log(JSON.stringify({ psttRef, broadcast: true }));' "$BOB_MULTI_INPUT_PSTT")")"
TXID_5="$(json_get broadcastTxid <<<"$BOB_MULTI_INPUT_FINAL")"
if [[ "$TXID_5" == "$TXID_1" || "$TXID_5" == "$TXID_2" || "$TXID_5" == "$TXID_3" || "$TXID_5" == "$TXID_4" ]]; then
  echo "Multi-input MIMO transfer reused an earlier broadcast txid" >&2
  exit 1
fi
wait_for_tx_accepted "$TXID_5" >/dev/null
BOB_AFTER_MULTI_INPUT="$(api_post_empty "$BOB_API" "/wallet/sync")"
BOB_MULTI_INPUT_CHANGE_OUTPOINT="$TXID_5:$BOB_MULTI_INPUT_CHANGE_VOUT"
require_eq "$(count_p2mr_outpoint "$BOB_AFTER_MULTI_INPUT" "$BOB_MULTI_INPUT_CHANGE_OUTPOINT")" "1" "Bob tracked P2MR change outpoint after multi-input spend"
sync_all_and_assert_invariants "post multi-input MIMO invariants"
echo "  Bob -> Alice+Carol aggregated TEST MIMO txid accepted: $TXID_5"

echo "[12/12] Sync and assert transfer records and balances"
ALICE_FINAL="$(api_post_empty "$ALICE_API" "/wallet/sync")"
BOB_FINAL="$(api_post_empty "$BOB_API" "/wallet/sync")"
CAROL_FINAL="$(api_post_empty "$CAROL_API" "/wallet/sync")"

ALICE_BALANCE_FINAL="$(asset_balance "$ALICE_FINAL" "$CONTRACT_ID")"
BOB_BALANCE_FINAL="$(asset_balance "$BOB_FINAL" "$CONTRACT_ID")"
CAROL_BALANCE_FINAL="$(asset_balance "$CAROL_FINAL" "$CONTRACT_ID")"
ALICE_CASH_BALANCE_FINAL="$(asset_balance "$ALICE_FINAL" "$CONTRACT_ID_2")"
BOB_CASH_BALANCE_FINAL="$(asset_balance "$BOB_FINAL" "$CONTRACT_ID_2")"
CAROL_CASH_BALANCE_FINAL="$(asset_balance "$CAROL_FINAL" "$CONTRACT_ID_2")"
require_eq "$ALICE_BALANCE_FINAL" "999830" "Alice final TEST balance"
require_eq "$BOB_BALANCE_FINAL" "15" "Bob final TEST balance"
require_eq "$CAROL_BALANCE_FINAL" "155" "Carol final balance"
require_eq "$ALICE_CASH_BALANCE_FINAL" "499940" "Alice final CASH balance"
require_eq "$BOB_CASH_BALANCE_FINAL" "0" "Bob final CASH balance"
require_eq "$CAROL_CASH_BALANCE_FINAL" "60" "Carol final CASH balance"
assert_global_invariants \
  "final dashboard invariants" \
  "$ALICE_FINAL" \
  "$BOB_FINAL" \
  "$CAROL_FINAL" \
  "$CONTRACT_ID" \
  "1000000" \
  "$CONTRACT_ID_2" \
  "500000"

ALICE_TRANSFER_COUNT="$(transfer_count "$ALICE_FINAL")"
BOB_TRANSFER_COUNT="$(transfer_count "$BOB_FINAL")"
CAROL_TRANSFER_COUNT="$(transfer_count "$CAROL_FINAL")"
require_eq "$ALICE_TRANSFER_COUNT" "4" "Alice transfer record count"
require_eq "$BOB_TRANSFER_COUNT" "5" "Bob transfer record count"
require_eq "$CAROL_TRANSFER_COUNT" "4" "Carol transfer record count"

echo "HTTP API chained, tentative-chain, same-contract MIMO, multi-contract MIMO, and multi-input MIMO transfer workflow completed."
echo "  Contract: $CONTRACT_ID"
echo "  Second contract: $CONTRACT_ID_2"
echo "  Alice -> Bob txid: $TXID_1"
echo "  Bob -> Carol txid: $TXID_2"
echo "  Alice -> Bob+Carol MIMO txid: $TXID_3"
echo "  Alice -> Bob TEST + Carol CASH MIMO txid: $TXID_4"
echo "  Bob -> Alice+Carol aggregated TEST MIMO txid: $TXID_5"
echo "  Alice final TEST balance: $ALICE_BALANCE_FINAL"
echo "  Bob final TEST balance: $BOB_BALANCE_FINAL"
echo "  Carol final TEST balance: $CAROL_BALANCE_FINAL"
echo "  Alice final CASH balance: $ALICE_CASH_BALANCE_FINAL"
echo "  Bob final CASH balance: $BOB_CASH_BALANCE_FINAL"
echo "  Carol final CASH balance: $CAROL_CASH_BALANCE_FINAL"
echo "  Data root: $DATA_ROOT"
cleanup
trap - EXIT
