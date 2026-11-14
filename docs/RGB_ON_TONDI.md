# RGB on Tondi v4

## Commitment and seal closure

Each closing transaction carries exactly one recognised RGB StatePayload anchor. Its standard Data Law has the exact `Tondi/v4/RGB/anchor` argument domain. Canonical datum contains the anchor version, MPC commitment and network-bound sidecar ID. A StateAnchorProof identifies the exact output and sidecar ID; a second recognised anchor or a different output, root or sidecar fails verification.

RGB MMB proofs map operations to **consumed** transaction inputs. A read-only reference never closes a seal. Merely paying to an Auth root does not establish the provenance of public data. Chain acceptance, reorganisation status, live UTXO resolution and RGB contract validation remain separate checks.

Auth is independent of the anchor. RGB does not tweak a public/private key or replace the output Auth commitment. The runtime commits the canonical strict-encoded pre-anchor PrefabBundle as the sidecar, which avoids a circular commitment to the final transaction ID. RGB contract state and history remain client-side; the node does not execute RGB AluVM.

## Native transactions and assets

Published transactions retain the complete shared `tondi_vm::Transition`: Actions, Auth proofs, Law invocations, program packages, signatures, declared references, Since and full StateNotes. Byte order and body/witness IDs match the node. Strict storage frames bounded canonical native bytes rather than defining another transaction encoding.

Native multi-asset amounts are `u128` and remain decimal strings in JSON. Both referenced asset definitions and RGB state survive signing and persistence. Native assets and RGB allocations are separate namespaces. The simple RGB funding constructor accepts explicit plain/RGB seal inputs; native asset or arbitrary protocol-state spending requires an explicit native draft or an imported native PSTT, preventing accidental token loss.

## PSTT construction and signing

Construction selects complete resolved Notes and Auth descriptors, chooses the anchor output and fixes its state before preparing mandatory execution bindings. A prepared body is immutable: signing verifies the retained input/output context and delegates to `tondi-wallet-pstt`. Combining signatures also delegates to the node wallet implementation. Only signature bytes are removed by unsigned export; code, Auth proofs and other witness data are retained.

PSTT has one v4 JSON schema with unknown fields and duplicate proprietary keys rejected. Old BIP maps and version switches are not decoded. Prepared transactions preserve arbitrary native Actions and custom Auth/Law programs through import.

Extraction requires an explicit deterministic node clock and runs the native VM, including Auth, Law, assets, capacity, resource and Since checks. Clock validation does not certify UTXO liveness; the accepting node independently resolves all inputs. CLI/HTTP extraction obtains the node's virtual DAA score and consensus median time. Offline signing alone is not chain acceptance.

## Public references and RPC

PublicReference pins the complete StateNote and OutPoint. Its Data Law uses a separate `Tondi/v4/RGB/public` domain. Appending a pin adds a declared read-only dependency before the body is prepared. Historical verification checks the creation transaction and reference kind; current execution requires a still-live UTXO. References supply neither spending value nor mint authority.

The client uses official RPC4 gRPC and Borsh wRPC transports and checks network identity. Complete Notes include birth DAA score and coinbase status. No copied protobuf or lossy bridge is used. The resolver uses accepted-chain transaction status, not mere block inclusion, and closes RPC connections within the Tokio runtime that owns them.

## Development status

The shared node sources must exist at the sibling `../Tondi` paths documented in the root README. This branch is a coordinated development implementation, not a standalone released dependency set. Full node broadcast, pruned-checkpoint replay and cross-wallet integration need dedicated acceptance testing before release. The current replay API propagates unavailable checkpoint errors rather than inferring a successful rescan.
