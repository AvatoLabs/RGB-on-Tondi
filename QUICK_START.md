# Quick Start

This guide gives the shortest path to build and smoke-test RGB on Tondi with P2MR anchoring.

## Build

```bash
cargo build --bin rgb
cargo check -p rgb-wallet --features resolver-tondi
cargo test -p rgb-wallet http_contract --bin rgb
cargo test -p rgb-wallet http_server --bin rgb --features resolver-tondi
node scripts/verify_frontend_http_contract.mjs
```

## Frontend HTTP Boundary

Run the backend-shaped service target used by the Avato wallet frontend:

```bash
TONDI_RPC=127.0.0.1:26310 \
cargo run -p rgb-wallet --features resolver-tondi -- \
  --network devnet serve-http --bind 127.0.0.1:8787
```

The service now handles wallet profile creation, lock/unlock, cached dashboard,
sync, P2MR address generation, issuer import, contract issue/accept, invoice
creation, and transfer payment preparation against the local RGB wallet store.
Transfer payment writes the real RGB consignment, PSTT, and operation bundle
under the service data directory. Transfer accept validates a known-contract
transfer consignment and records the wallet-owned RGB state delta. Transfer
finalisation can extract and optionally broadcast a PSTT from the HTTP transfer
record after signing matching Tondi P2MR inputs from the unlocked local signer
session. The service persists only an encrypted wallet seed and clears decrypted
signer material on lock; after a service restart, unlock the wallet before
finalising PSTT inputs.

## Smoke Test

```bash
./test_workflow_simple.sh
```

The smoke test verifies that the CLI builds, the workflow scripts parse, the issue template is present, and runtime code no longer exposes legacy publication entry points.

## Devnet Check

Start a Tondi devnet node with RPC and UTXO indexing enabled, then run:

```bash
TONDI_DEVNET_RPC=127.0.0.1:26310 \
TONDI_DEVNET_ALLOW_UNSYNCED=1 \
bash scripts/devnet_p2mr_rgb_verify.sh
```

## Self-Contained Accepted Devnet Workflow

For the full issuer, invoice, PSTT, broadcast, and accepted-state path, use a
patched sibling Tondi checkout:

```bash
TONDI_REPO=../Tondi scripts/local_devnet_full_workflow.sh
```

The script builds the RGB CLI, starts a local Tondi devnet with `--utxoindex`,
preallocates a faucet P2MR output, sends a real signed P2MR funding transaction
to Alice and Bob, mines the funding transaction, broadcasts the RGB transfer,
and requires Tondi `GetTransactionStatus` to report the final transfer as
`accepted`.

## Full Contract Workflow

Prepare an issuer file and, if you do not want deterministic devnet keys, wallet xpubs:

```bash
export ISSUER_FILE='<path-to-tondi-compatible-issuer.issuer>'
export TONDI_RPC='127.0.0.1:26310'
export NETWORK='devnet'
```

Run:

```bash
AUTO_DEV_KEYS=1 WAIT_FOR_FUNDING=1 ./test_full_workflow.sh
```

`AUTO_DEV_KEYS=1` derives deterministic devnet-only Alice/Bob xpubs and Alice's
P2MR signing key. `WAIT_FOR_FUNDING=1` prints the generated P2MR funding
addresses and polls until both wallets see live indexed UTXOs. For real
broadcast verification, add `AUTO_BROADCAST=1`; the script then checks the
submitted transaction through Tondi `GetTransactionStatus`.

For a local acceptance-gated manual run, set:

```bash
AUTO_BROADCAST=1 REQUIRE_BROADCAST_ACCEPTED=1 MIN_CONFIRMATIONS=1 ./test_full_workflow.sh
```

For a step-by-step explanation, read `TONDI_CONTRACT_WORKFLOWS.md`.

## Key Files

- `README.md`: architecture and repository overview.
- `TONDI_CONTRACT_WORKFLOWS.md`: production workflow guide.
- `test_workflow_simple.sh`: local smoke checks.
- `test_full_workflow.sh`: end-to-end issuer, invoice, transfer, and finalization flow.
- `issue_params_template.yaml`: RGB20 issue parameter template using a Tondi outpoint placeholder.
- `scripts/devnet_p2mr_rgb_verify.sh`: devnet P2MR resolver validation.
- `scripts/local_devnet_full_workflow.sh`: self-contained local devnet proof with accepted broadcast.
- `scripts/verify_frontend_http_contract.mjs`: cross-checks the Rust wallet HTTP boundary against the Avato frontend contract.
- `rgb/cli/src/http_contract.rs`: Rust DTOs and endpoint table for the frontend HTTP boundary.
- `rgb/cli/src/http_server.rs`: Axum service exposing the frontend HTTP boundary.
- `bp-tondi-client/examples/devnet_p2mr_fund.rs`: development-only P2MR faucet funder for isolated local devnet runs.
