# Native v4 verification — 2026-11-14

Validated against the sibling Tondi native-v4 development source checkout described in the root README.

- `cargo test --workspace --no-fail-fast`: 331 passed, 0 failed, 13 ignored, including documentation tests.
- `cargo check --workspace --all-targets --locked`: passed.
- `cargo test -p bp-core --features all --locked`: passed, including the regenerated type-library ID.
- `cargo test -p rgb-std --features stl --lib --locked`: passed, including the native transaction/anchor type-library ID.
- `cargo test -p rgb-runtime --features serde --locked`: 8 passed, including complete UTXO persistence and funding address derivation.
- `git diff --check`: passed.

Regression coverage includes native wire and strict-frame round trips, full-width `u128` assets and 64-bit Since, unchanged body IDs after signing, preservation of reference inputs and RGB state, immutable prepared transaction context, rejection of old PSTT maps and duplicate metadata, exact public StateNote pins, references contributing no spending value, and unique RGB anchors with independent Auth commitments.

The source type libraries and their generated artifacts were regenerated together. Cargo.lock is tracked for reproducible dependency resolution.

No live-node broadcast, pruning recovery or cross-wallet acceptance result is claimed for this RGB branch. The adjacent node source is a separate development dependency, not included or published by this repository commit.
