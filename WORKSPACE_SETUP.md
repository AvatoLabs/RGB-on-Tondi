# RGB on Tondi Workspace Setup

This workspace is maintained as a single Tondi-native Cargo tree. All internal crates are wired by local paths through the root `Cargo.toml`, and the production RGB runtime is expected to use Tondi RPC, PSTT, and P2MR commitment publication.

## Active Layout

- Core encoding and VM crates: `strict-encoding`, `strict-types`, `aluvm`, `zk-aluvm`, `ultrasonic`.
- Client-side validation crates: `client_side_validation`, `commit_verify`, `single_use_seals`.
- Tondi protocol support crates: `bp-core`, `bp-std`, and their subcrates, adapted for Tondi semantics.
- Tondi RPC integration: `bp-tondi-client`.
- RGB contract stack: `rgb-core`, `rgb-std`, `rgb`, `rgb-interfaces`.

## Build

```bash
cargo check --workspace
cargo build --workspace
cargo test --workspace
```

For the production RGB path:

```bash
cargo build --bin rgb
cargo check -p rgb-wallet --features resolver-tondi
./test_workflow_simple.sh
```

## Devnet Verification

Run a Tondi devnet node with RPC and the UTXO index enabled, then run:

```bash
TONDI_DEVNET_RPC=127.0.0.1:26310 \
TONDI_DEVNET_ALLOW_UNSYNCED=1 \
bash scripts/devnet_p2mr_rgb_verify.sh
```

## Workspace Rules

- Use local workspace dependencies for internal crates.
- Keep workflow docs and scripts English-only.
- Keep RGB wallet publication paths P2MR-first.
- Do not commit generated node data or ad hoc workflow outputs.
