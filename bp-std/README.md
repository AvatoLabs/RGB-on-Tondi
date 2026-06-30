# Tondi Wallet Standard Libraries

This crate group provides wallet-level primitives used by RGB on Tondi: descriptors, derivation, invoice support, and PSTT handling.

## Crates

- `bp-derive`: derivation helpers.
- `descriptors`: descriptor abstractions used by RGB wallet code.
- `bp-invoice`: address and invoice support.
- `pstt`: Partially Signed Tondi Transaction data structures.
- `bp-std`: umbrella crate for wallet-level integrations.

## RGB on Tondi Policy

- New RGB wallet outputs must be P2MR-capable.
- PSTT code must reject non-P2MR RGB publication outputs.
- Resolver integration should use `bp-tondi-client` and Tondi RPC.

## Checks

```bash
cargo check -p rgb-pstt --features bp
cargo test -p rgb-descriptors --features serde
```
