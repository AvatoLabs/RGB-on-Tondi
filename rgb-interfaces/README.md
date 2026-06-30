# RGB Interfaces for Tondi

This crate contains the standard RGB contract interface types used by
RGB-on-Tondi.

## Interfaces

- `fungible.rs`: fungible asset values such as `Amount` and `Precision`.
- `nft.rs`: NFT media and ownership types.
- `por.rs`: proof-of-reserves metadata.
- `names.rs`: tickers, asset names, and descriptions.
- `types.rs`: Strict Type library construction.

The interfaces are chain-neutral at the contract layer, but RGB-on-Tondi
workflows bind issuance and transfer publication to Tondi P2MR outpoints.

## Tondi Workflow

All supported contract schemas use the same owner workflow:

1. Import the issuer with `rgb import <issuer>`.
2. Create a P2MR wallet with `rgb create <wallet> <xpub>`.
3. Fund the wallet with Tondi coins at `RGB_WALLET=<wallet> rgb fund`.
4. Use `RGB_WALLET=<wallet> rgb seals` to pick the issue or receive outpoint.
5. Issue or transfer with Tondi-native params and invoices.

For a complete CLI workflow see the repository root
`TONDI_CONTRACT_WORKFLOWS.md` and `test_full_workflow.sh`.

## Example Types

```rust
use ifaces::{Amount, AssetName, Details, Precision, Ticker};

let precision = Precision::CentiMicro;
let amount = Amount::with_precision(1_000_000, precision);
let (integer, fractional) = amount.split(precision);

let ticker = Ticker::from("USDT");
let name = AssetName::from("Tondi USD");
let details = Details::from("USD-denominated asset issued on RGB-on-Tondi");
```

## Validation

```bash
cargo test -p rgb-interfaces --features serde
cargo run -p rgb-interfaces --bin rgb-interfaces-stl --features stl
```
