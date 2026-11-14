# Tondi Contract Workflows

This document is the operational workflow reference for RGB-on-Tondi contracts.
It covers fungible assets, NFTs, proof-of-reserves contracts, and custom RGB
issuers. The contract schema changes the issue parameters and state names; the
publication workflow stays the same.

## Invariants

- Use Tondi P2MR outputs for RGB anchors.
- Keep private RGB state, consignments, proofs, and sidecars off-chain.
- Public Datum and pinned references are explicit opt-in application context; see [the current design](docs/RGB_ON_TONDI.md). A reference never closes a seal or authorizes minting.
- Do not place RGB commitments, contract ids, transition ids, allocation data,
  consignment pointers, or Annex-style metadata in Tondi payloads,
  signature scripts, or witness data.
- Use P2MR for ordinary wallet funding as well, so RGB transfers do not stand
  out simply because they use P2MR.

## One-Time Setup

```bash
cargo build --bin rgb
rgb --data-dir <store> --network testnet --tondi=<grpc-endpoint> init
rgb --data-dir <store> --network testnet --tondi=<grpc-endpoint> create <wallet> <xpub>
RGB_WALLET=<wallet> rgb --data-dir <store> --network testnet --tondi=<grpc-endpoint> fund
```

Fund the printed Tondi P2MR address, then sync:

```bash
RGB_WALLET=<wallet> rgb --data-dir <store> --network testnet --tondi=<grpc-endpoint> sync
RGB_WALLET=<wallet> rgb --data-dir <store> --network testnet --tondi=<grpc-endpoint> seals
```

## Issue

1. Import the issuer on the issuer wallet:

   ```bash
   rgb --data-dir <issuer-store> --network testnet --tondi=<grpc-endpoint> import <schema.issuer>
   ```

2. Create an issue params file. Use `issue_params_template.yaml` for RGB20-like
   fungible assets. NFT and proof-of-reserves issuers use the same `consensus`,
   `testnet`, `issuer`, `name`, `method`, and `owned.seal` structure with their
   own state names and values.

3. Issue:

   ```bash
   RGB_WALLET=<issuer-wallet> rgb --data-dir <issuer-store> --network testnet --tondi=<grpc-endpoint> issue <params.yaml>
   ```

4. Backup contract articles for receivers:

   ```bash
   RGB_WALLET=<issuer-wallet> rgb --data-dir <issuer-store> --network testnet --tondi=<grpc-endpoint> backup --force <contract> <contract.rgb>
   ```

## Receiver Accepts Contract

```bash
rgb --data-dir <receiver-store> --network testnet --tondi=<grpc-endpoint> import <schema.issuer>
RGB_WALLET=<receiver-wallet> rgb --data-dir <receiver-store> --network testnet --tondi=<grpc-endpoint> accept --unknown <contract.rgb>
```

## Transfer

Receiver creates an invoice using an unspent P2MR authority token:

```bash
RGB_WALLET=<receiver-wallet> rgb --data-dir <receiver-store> --network testnet --tondi=<grpc-endpoint> invoice --nonce 0 --api <api-name> --method transfer --state <state-name> <contract> <amount>
```

Sender pays the invoice:

```bash
RGB_WALLET=<sender-wallet> rgb --data-dir <sender-store> --network testnet --tondi=<grpc-endpoint> pay --strategy aggregate --fee 1000 --sats 1000 --force <invoice> <transfer.rgb> <transfer.pstt>
```

Receiver accepts the transfer consignment:

```bash
RGB_WALLET=<receiver-wallet> rgb --data-dir <receiver-store> --network testnet --tondi=<grpc-endpoint> accept <transfer.rgb>
```

Finalize and optionally broadcast the signed PSTT:

```bash
RGB_WALLET=<sender-wallet> rgb --data-dir <sender-store> --network testnet --tondi=<grpc-endpoint> finalize <transfer.pstt> <transfer.tx>
RGB_WALLET=<sender-wallet> rgb --data-dir <sender-store> --network testnet --tondi=<grpc-endpoint> finalize --broadcast <transfer.pstt>
```

## Multi-Contract Payment Scripts

For batched or multi-contract transfers:

```bash
RGB_WALLET=<sender-wallet> rgb --data-dir <sender-store> --network testnet --tondi=<grpc-endpoint> script <invoice> <payment.yaml>
RGB_WALLET=<sender-wallet> rgb --data-dir <sender-store> --network testnet --tondi=<grpc-endpoint> exec <payment.yaml> <bundle.rgb> 1000 <bundle.pstt>
RGB_WALLET=<sender-wallet> rgb --data-dir <sender-store> --network testnet --tondi=<grpc-endpoint> complete <bundle.rgb> <bundle.pstt>
```

## Verification

Run the non-interactive workflow smoke check:

```bash
./test_workflow_simple.sh
```

Run full local/devnet validation:

```bash
TONDI_DEVNET_RPC=<grpc-endpoint> bash scripts/devnet_p2mr_rgb_verify.sh
```

Run the self-contained local proof with a patched sibling Tondi checkout:

```bash
TONDI_REPO=../Tondi scripts/local_devnet_full_workflow.sh
```

This starts a local Tondi devnet with `--utxoindex`, funds Alice and Bob through
a real signed P2MR faucet transaction, mines the funding transaction, broadcasts
the RGB transfer, and requires the final broadcast transaction to reach
`accepted` through Tondi `GetTransactionStatus`.

For a manual acceptance-gated run against an already running local devnet:

```bash
AUTO_BROADCAST=1 \
REQUIRE_BROADCAST_ACCEPTED=1 \
MIN_CONFIRMATIONS=1 \
./test_full_workflow.sh
```

## Production Chain-State Recovery

The resolver treats accepted-with-depth transactions as mined and mempool or
orphan transactions as tentative. Retained known transactions reported as
`removed`, plus absent or pruned transactions, are archived.

Wallet services must persist the last applied Tondi sink hash. On restart or
disconnect recovery, call `TondiClient::replay_virtual_chain_from_checkpoint`
with accepted transaction IDs enabled, apply removed-chain and added-chain
deltas in order, then persist `next_checkpoint`. The helper paginates
`GetVirtualChainFromBlock`, so long-running devnet and production scans do not
silently stop at the first RPC batch.

If the checkpoint has fallen outside node retention, replay restarts from the
current pruning point and returns `PruningPointFallback`. Treat that as a
mandatory full local wallet/RGB rescan. This keeps reorg handling in the
general Tondi chain-state/RPC layer and avoids adding RGB-specific consensus
logic.
