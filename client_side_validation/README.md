# Client-side Validation for Tondi

This workspace subtree contains the generic client-side validation primitives
used by RGB-on-Tondi. The crates here define deterministic commitments,
single-use seal traits, and the validation glue required by contract
consignments.

## Components

```mermaid
flowchart TD
    csv["client_side_validation"] --> commit["commit_verify"]
    csv --> seals["single_use_seals"]
    commit --> ids["Commitment IDs"]
    commit --> merkle["Merkle proofs"]
    seals --> p2mr["P2MR seal witnesses"]
    p2mr --> tondi["Tondi L1 publication"]
```

- `commit_verify` provides commitment identifiers, multi-message commitments,
  tagged merkle structures, and deterministic digest helpers.
- `single_use_seals` defines the seal traits used by Tondi transaction outputs.
- `client_side_validation` re-exports the pieces needed by contract validation
  code.

## Production Position

RGB-on-Tondi uses this layer with Tondi-native publication and P2MR seal
witnesses. The runtime and wallet code should not route production transfers
through legacy host schemes or external chain resolvers.

## Usage

Use the workspace crates directly:

```toml
client_side_validation = { workspace = true }
commit_verify = { workspace = true }
single_use_seals = { workspace = true }
```

Feature flags are intentionally narrow:

- `rand` enables random 32-byte values used in tests and protocol helpers.
- `serde` enables structured serialization for application-facing data.

## Testing

From the repository root:

```bash
cargo test -p client_side_validation
cargo test -p commit_verify
cargo test -p single_use_seals
```

Any new seal implementation must include deterministic positive and negative
verification tests, including mismatched message and already-spent cases.
