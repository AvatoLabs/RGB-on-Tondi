# Native Tondi RPC client

Uses the official sibling Tondi RPC4 clients over gRPC or Borsh wRPC. Network checks, complete transactions and StateNotes, public references and native asset APIs share the node model. Call `disconnect()` before ending the Tokio runtime owning a connection.

See the [workspace requirements](../README.md) and [native RGB design](../docs/RGB_ON_TONDI.md).
