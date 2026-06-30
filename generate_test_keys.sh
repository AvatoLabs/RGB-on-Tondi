#!/usr/bin/env bash
# Generate development xpubs for the RGB on Tondi workflow scripts.

set -euo pipefail

echo "=== Generate development wallet keys ==="
echo

if ! command -v bdk-cli >/dev/null 2>&1; then
    echo "bdk-cli is required to generate valid BIP32 development xpubs."
    echo "Install bdk-cli, or provide ALICE_XPUB and BOB_XPUB from an existing development wallet."
    exit 2
fi

echo "Generating Alice key material..."
ALICE_MNEMONIC="$(bdk-cli key generate | grep -oP 'mnemonic: \\K[^\"]+')"
ALICE_XPUB="$(bdk-cli key derive --mnemonic "$ALICE_MNEMONIC" --path "m/84'/1'/0'" | grep -oP 'xpub: \\K[^\\s]+')"

echo "Generating Bob key material..."
BOB_MNEMONIC="$(bdk-cli key generate | grep -oP 'mnemonic: \\K[^\"]+')"
BOB_XPUB="$(bdk-cli key derive --mnemonic "$BOB_MNEMONIC" --path "m/84'/1'/0'" | grep -oP 'xpub: \\K[^\\s]+')"

echo
echo "Export these variables for the workflow scripts:"
echo "export ALICE_XPUB='$ALICE_XPUB'"
echo "export BOB_XPUB='$BOB_XPUB'"
echo
echo "Optional helper file:"
echo "printf '%s\\n' 'export ALICE_XPUB=\"$ALICE_XPUB\"' 'export BOB_XPUB=\"$BOB_XPUB\"' > test_keys.env"
echo
echo "Development keys generated. Do not use them for real funds."
