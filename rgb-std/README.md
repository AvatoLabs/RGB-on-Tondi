# RGB Standard Library for Tondi

This crate is the Tondi-native RGB standard library used by RGB-on-Tondi.

The active proof-of-publication path is P2MR-only:

- chain commitments are Tondi P2MR output roots;
- RGB consignments, proofs, allocations, and sidecar data stay off-chain;
- Tondi transaction payloads, signature scripts, and witness data must not carry RGB metadata;
- wallet workflows use Tondi P2MR funding addresses and Tondi resolvers.

## Workflow

The same contract lifecycle is used by fungible assets, NFTs, proof-of-reserves
contracts, and custom issuers:

1. Import a contract issuer with `rgb import <issuer>`.
2. Create a P2MR wallet with `rgb create <wallet> <xpub>`.
3. Fund the wallet using `RGB_WALLET=<wallet> rgb fund`.
4. Sync and select a Tondi outpoint with `RGB_WALLET=<wallet> rgb seals`.
5. Issue with `RGB_WALLET=<wallet> rgb issue <params.yaml>`.
6. Share contract articles with `rgb backup` and `rgb accept --unknown`.
7. Transfer with `rgb invoice`, `rgb pay`, `rgb accept`, and `rgb finalize`.

See the repository root `TONDI_CONTRACT_WORKFLOWS.md`,
`issue_params_template.yaml`, `test_workflow_simple.sh`, and
`test_full_workflow.sh` for executable workflow guidance.

## Features

- `tondi`: enables the Tondi P2MR proof-of-publication stack.
- `serde`: enables serialization support.
- `fs`: enables filesystem persistence.
- `async`: enables async synchronization paths.
- `stl`: emits Strict Type libraries.

## Validation

Run:

```bash
cargo check -p rgb-wallet --features resolver-tondi
cargo test -p rgb-runtime --features "serde fs resolver-tondi" p2mr_address
./test_workflow_simple.sh
```

Live devnet validation is covered by:

```bash
TONDI_DEVNET_RPC=<grpc-endpoint> bash scripts/devnet_p2mr_rgb_verify.sh
```
