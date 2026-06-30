# Tondi Migration Status

This document records the current production posture of RGB on Tondi. Historical migration notes have been condensed so the repository describes the active design rather than retired paths.

## Active Baseline

- Tondi P2MR is the publication path for RGB commitments.
- Public chain data is limited to Tondi transactions, outpoints, script public keys, and P2MR commitment proofs.
- Contract state, invoices, consignments, and transfer history remain client-side.
- Tondi RPC is used through `bp-tondi-client`.
- Wallet transaction assembly uses PSTT naming and Tondi network selection.

## Required Invariants

- New wallet outputs created for RGB publication must be P2MR-capable.
- Resolver code must reject missing, mismatched, or non-P2MR commitment evidence.
- Contract workflows must not depend on retired publication transports.
- Devnet validation must cover at least one publish and resolve cycle before release.

## Verification

```bash
cargo build --bin rgb
cargo check -p rgb-wallet --features resolver-tondi
cargo test -p rgb-core genesis_with_external_output -- --nocapture
cargo test -p rgb-std --test consignment -- --nocapture
TONDI_DEVNET_RPC=127.0.0.1:26310 TONDI_DEVNET_ALLOW_UNSYNCED=1 bash scripts/devnet_p2mr_rgb_verify.sh
```

## Remaining Release Checks

- Run the full contract workflow against a funded devnet wallet pair.
- Review generated artifacts before committing them.
- Confirm top-level documentation and scripts stay English-only.
