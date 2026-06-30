# Tondi Core Protocol Libraries

This crate group provides Tondi-adapted consensus, deterministic commitment, and single-use-seal primitives used by RGB on Tondi.

## Crates

- `bp-consensus`: Tondi transaction, script, key, hash, and strict-type support.
- `bp-dbc`: deterministic commitment helpers retained for compatibility and P2MR transition work.
- `bp-seals`: Tondi UTXO seal definitions and P2MR commitment verification.
- `bp-core`: umbrella crate re-exporting the protocol pieces required by RGB.

## RGB on Tondi Policy

- New RGB publication code must use Tondi P2MR outputs.
- Legacy publication transports are not production paths.
- Strict Type artifacts should be regenerated from the Tondi-adapted generators, not hand-edited.

## Checks

```bash
cargo test -p bp-seals valid_p2mrs -- --nocapture
cargo test -p bp-consensus --features stl lib_id -- --nocapture
cargo test -p bp-core --features stl lib_id -- --nocapture
```
