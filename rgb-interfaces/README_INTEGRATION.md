# RGB Interfaces Integration

`rgb-interfaces` is integrated as a local RGB-on-Tondi workspace member.

The crate is maintained together with the Tondi-native runtime, wallet, PSTT,
and resolver crates. New work should target Tondi P2MR publication semantics and
must not add chain-specific commitment carriers outside P2MR output roots.

## Workspace Usage

```toml
[dependencies]
rgb-interfaces = { workspace = true }
```

## Maintenance Rules

- Keep contract interface types independent from transport and resolver code.
- Use Tondi/P2MR workflow examples in repository-root docs and scripts.
- Do not add wallet examples that publish RGB metadata in payloads, witness data,
  signature scripts, or sidecar pointers.
- Regenerate Strict Type artifacts only when the interface type definitions
  change, and verify generated artifacts before committing.
