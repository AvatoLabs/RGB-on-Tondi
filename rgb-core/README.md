# RGB Core for Tondi

This crate contains the consensus-critical and validation primitives used by
RGB-on-Tondi.

The fork is Tondi-native:

- seal publication is anchored to Tondi UTXO witnesses;
- RGB commitments are bound to P2MR output roots by the wallet/runtime layer;
- client-side validation data stays off-chain;
- transaction payloads, signature scripts, witness data, and Annex-like
  carriers are not used for RGB metadata.

The high-level wallet and workflow surface lives in `rgb`, `rgb-std`,
`rgb-interfaces`, and the repository root workflow scripts.
