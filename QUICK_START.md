# Quick Start

This guide gives the shortest path to build and smoke-test RGB on Tondi with P2MR anchoring.

## Build

```bash
cargo build --bin rgb
cargo check -p rgb-wallet --features resolver-tondi
```

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

## Full Contract Workflow

Prepare wallet xpubs and an issuer file:

```bash
export ALICE_XPUB='<alice extended public key>'
export BOB_XPUB='<bob extended public key>'
export ISSUER_FILE='rgb/examples/RGB20-FNA.issuer'
export TONDI_RPC='127.0.0.1:26310'
export NETWORK='devnet'
```

Run:

```bash
./test_full_workflow.sh
```

For a step-by-step explanation, read `TONDI_CONTRACT_WORKFLOWS.md`.

## Key Files

- `README.md`: architecture and repository overview.
- `TONDI_CONTRACT_WORKFLOWS.md`: production workflow guide.
- `test_workflow_simple.sh`: local smoke checks.
- `test_full_workflow.sh`: end-to-end issuer, invoice, transfer, and finalization flow.
- `issue_params_template.yaml`: RGB20 issue parameter template using a Tondi outpoint placeholder.
- `scripts/devnet_p2mr_rgb_verify.sh`: devnet P2MR resolver validation.
