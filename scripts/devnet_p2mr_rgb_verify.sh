#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TONDI_REPO="${TONDI_REPO:-"$ROOT/../Tondi"}"

if [[ -z "${TONDI_DEVNET_RPC:-}" ]]; then
  echo "TONDI_DEVNET_RPC is required, for example: TONDI_DEVNET_RPC=127.0.0.1:16210 bash scripts/devnet_p2mr_rgb_verify.sh" >&2
  echo "For an isolated local devnet only, set TONDI_DEVNET_ALLOW_UNSYNCED=1." >&2
  exit 2
fi

if [[ ! -d "$TONDI_REPO" ]]; then
  echo "TONDI_REPO does not exist: $TONDI_REPO" >&2
  exit 2
fi

echo "[1/9] Tondi PSTT RGB outputs are P2MR-only"
(cd "$TONDI_REPO" && CARGO_INCREMENTAL=0 cargo +1.95.0 test -p tondi-wallet-pstt --test test_unsigned_tx_with_rgb -- --nocapture)

echo "[2/9] Tondi P2MR RGB script verification"
(cd "$TONDI_REPO" && CARGO_INCREMENTAL=0 cargo +1.95.0 test -p tondi-wallet-pstt --test test_rgb_script_verification -- --nocapture)

echo "[3/9] RGB-on-Tondi P2MR anchor proof"
(cd "$ROOT" && cargo test -p bp-seals valid_p2mrs -- --nocapture)

echo "[4/9] RGB-on-Tondi P2MR descriptor test"
(cd "$ROOT" && cargo test -p rgb-descriptors --features serde)

echo "[5/9] RGB-on-Tondi P2MR funding address test"
(cd "$ROOT" && cargo test -p rgb-runtime --features "serde fs resolver-tondi" p2mr_address)

echo "[6/9] RGB-on-Tondi PSTT glue build"
(cd "$ROOT" && cargo check -p rgb-pstt --features bp)

echo "[7/9] RGB-on-Tondi native RPC/client build"
(cd "$ROOT" && cargo check -p bp-tondi --features grpc)

echo "[8/9] RGB-on-Tondi CLI/runtime build"
(cd "$ROOT" && cargo check -p rgb-wallet --features resolver-tondi)

echo "[9/9] Live devnet P2MR native probe"
(cd "$ROOT" && TONDI_DEVNET_RPC="$TONDI_DEVNET_RPC" TONDI_DEVNET_ALLOW_UNSYNCED="${TONDI_DEVNET_ALLOW_UNSYNCED:-}" cargo run -p bp-tondi --features grpc --example devnet_p2mr_native_probe)
