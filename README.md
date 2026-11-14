# RGB on native Tondi v4

RGB remains a client-side validated protocol. Tondi commits its MPC root and network-bound sidecar ID in a native StateNote, while P2MR Auth controls spending and declared reference inputs provide public dependencies.

This branch shares the VM, native transaction model, RPC4 transports and signing implementation with the adjacent **Tondi v0.4.2 development checkout**. It is a coordinated source checkout, not a standalone dependency release:

```text
parent/
  Tondi/          # codex/tondi-v0.4.2-vm-native-assets; native v4 source required
  RGB-on-Tondi/   # codex/rgb-native-v4
```

The Tondi source changes are maintained in the node repository and are not included in this repository. Cargo uses explicit sibling paths, including `../Tondi/vm` and `../Tondi/wallet/pstt`; the older node tree cannot build this branch.

```sh
cargo check --workspace --all-targets
cargo test --workspace
```

[Native RGB model and API boundaries](docs/RGB_ON_TONDI.md) describes commitments, signing, full native asset preservation, Since and public references. This branch accepts only the native v4 transaction and PSTT model; old transaction codecs, script signing stacks, tweaked RGB keys and BIP PSTT maps have been removed.

[Verification results](docs/VERIFICATION.md) record the executed checks. Live node broadcast and cross-wallet RGB acceptance require separate integration verification; unit tests do not establish network acceptance.
