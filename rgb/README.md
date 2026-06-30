# RGB on Tondi

This runtime is the Tondi-native RGB wallet stack. New wallet, resolver, and
transaction paths are P2MR-only:

- Chain outputs commit through Tondi P2MR v4 roots.
- RGB state, consignments, proofs, and sidecars stay off-chain.
- Tondi transaction payloads and BP/RGB bridge witness fields must stay empty.
- P2MR spend witness data is only spend authorization data, not an RGB metadata
  carrier.
- Wallet funding addresses are Tondi P2MR addresses (`tondi*:m...`).

Historical non-P2MR examples have been removed from this fork. Code that needs a
new descriptor must create a `RgbDescr::p2mr_unfunded` descriptor and use the
Tondi resolver.

## CLI

Initialize a data directory:

```console
rgb init
```

Create a Tondi P2MR wallet from an xpub descriptor:

```console
rgb create default '<xpub-derivation>'
```

Get a Tondi P2MR funding address:

```console
rgb --wallet default fund
```

Run a Tondi resolver-backed sync:

```console
rgb --tondi=127.0.0.1:16210 sync
```

## Devnet Verification

The repository includes a policy and liveness probe:

```console
TONDI_DEVNET_RPC=127.0.0.1:16210 bash scripts/devnet_p2mr_rgb_verify.sh
```

The script runs Tondi PSTT P2MR tests, RGB descriptor/runtime checks, native
client checks, and a live devnet probe. It refuses to pass without an explicit
`TONDI_DEVNET_RPC` endpoint.
