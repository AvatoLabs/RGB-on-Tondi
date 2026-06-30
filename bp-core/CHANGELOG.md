# Changelog

## Tondi Native

- Rebased protocol metadata and generated type libraries around Tondi naming.
- Uses BLAKE3-oriented transaction identifiers for Tondi L1 publication.
- Keeps P2MR as the production RGB seal workflow.
- Leaves legacy host modules available only as non-production compatibility code
  while runtime RGB-on-Tondi workflows deny those entry points.
