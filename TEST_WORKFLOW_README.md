# Workflow Testing

The workflow tests exercise RGB contract issuance, receiver acceptance, invoice creation, PSTT payment, transfer acceptance, and Tondi P2MR anchoring.

## Smoke Test

```bash
./test_workflow_simple.sh
```

This is the fast local check. It does not require funded wallets. It validates command availability, script syntax, issue template shape, and absence of disabled publication paths in runtime sources.

## Full Workflow

```bash
export ALICE_XPUB='<alice extended public key>'
export BOB_XPUB='<bob extended public key>'
export ISSUER_FILE='rgb/examples/RGB20-FNA.issuer'
export TONDI_RPC='127.0.0.1:26310'
export NETWORK='devnet'

./test_full_workflow.sh
```

The script uses separate Alice and Bob data directories. Override them when needed:

```bash
export DATA_DIR_ALICE='./rgb-test-alice'
export DATA_DIR_BOB='./rgb-test-bob'
```

## Workflow Steps

1. Initialize Alice and Bob stores.
2. Create Tondi P2MR wallets from xpubs.
3. Import the issuer file into both stores.
4. Print funding addresses and sync wallet state.
5. Select an available Tondi outpoint for issuance.
6. Issue an RGB20 contract with a P2MR-backed seal.
7. Export and accept the contract consignment.
8. Create a receiver invoice.
9. Pay the invoice and create transfer artifacts.
10. Accept the transfer on the receiver side.
11. Finalize and broadcast the Tondi transaction when signatures are available.
12. Sync and show final contract state.

## Devnet Resolver Test

```bash
TONDI_DEVNET_RPC=127.0.0.1:26310 \
TONDI_DEVNET_ALLOW_UNSYNCED=1 \
bash scripts/devnet_p2mr_rgb_verify.sh
```

This check verifies that P2MR commitments can be published to and resolved from a running Tondi devnet node.

## Troubleshooting

- If wallet creation fails, check that the xpubs are valid and that the wallet name is not already present in the data directory.
- If no spendable outpoint is found, fund the printed Tondi address and run `sync`.
- If broadcast fails, confirm the node RPC endpoint, network selection, and UTXO index settings.
- If a consignment is rejected, rerun the smoke test and inspect the contract state with `rgb state`.
