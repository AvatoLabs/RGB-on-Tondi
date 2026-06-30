# Workflow File Summary

This workspace keeps the Tondi RGB workflow files at the repository root so they can be run from a fresh checkout without extra scaffolding.

## Files

- `test_workflow_simple.sh`: local smoke test for CLI, templates, scripts, and disabled legacy entry points.
- `test_full_workflow.sh`: issuer-to-receiver contract workflow using Tondi P2MR wallets and PSTT transfer handling.
- `generate_test_keys.sh`: helper for producing development xpubs when `bdk-cli` is installed.
- `issue_params_template.yaml`: issue parameter template with a Tondi outpoint placeholder.
- `TONDI_CONTRACT_WORKFLOWS.md`: production workflow guide.
- `TEST_WORKFLOW_README.md`: test-focused workflow notes.
- `QUICK_START.md`: shortest build and verification path.

## Recommended Order

```bash
cargo build --bin rgb
./test_workflow_simple.sh
bash scripts/devnet_p2mr_rgb_verify.sh
./test_full_workflow.sh
```

Use the devnet script before the full workflow whenever resolver or P2MR transaction code changes.
