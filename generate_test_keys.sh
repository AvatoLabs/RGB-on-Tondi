#!/usr/bin/env bash
# 生成测试密钥对用于 RGB 钱包测试

set -e

echo "=== 生成测试密钥对 ==="
echo ""

# 检查是否安装了必要的工具
if ! command -v bdk-cli &> /dev/null; then
    echo "⚠️  未找到 bdk-cli，将使用 Python 脚本生成密钥"
    echo ""
    
    # 使用 Python 生成测试密钥
    python3 << 'PYTHON_SCRIPT'
import secrets
import hashlib
import base58

def generate_test_xpub():
    """生成一个测试用的 xpub（简化版本）"""
    # 注意：这是一个简化的示例，实际应该使用 BIP32 标准
    # 这里仅用于测试目的
    private_key = secrets.token_bytes(32)
    
    # 简化的 xpub 生成（实际应使用 BIP32）
    # 这里我们生成一个看起来像 xpub 的字符串
    prefix = b'\x04\x88\xB2\x1E'  # Mainnet xpub prefix
    depth = b'\x00'
    parent_fingerprint = b'\x00\x00\x00\x00'
    child_number = b'\x00\x00\x00\x00'
    chain_code = secrets.token_bytes(32)
    
    # 简化的公钥（实际应使用椭圆曲线）
    public_key = hashlib.sha256(private_key).digest()[:33]
    public_key = b'\x02' + public_key[1:]  # 压缩公钥格式
    
    # 组合
    extended_key = prefix + depth + parent_fingerprint + child_number + chain_code + public_key
    
    # Base58 编码
    xpub = base58.b58encode_check(extended_key).decode('ascii')
    return xpub

# 生成两个测试密钥
alice_xpub = generate_test_xpub()
bob_xpub = generate_test_xpub()

print(f"export ALICE_XPUB='{alice_xpub}'")
print(f"export BOB_XPUB='{bob_xpub}'")
PYTHON_SCRIPT

    echo ""
    echo "⚠️  注意: 上述密钥仅用于测试，不应用于生产环境"
    echo "   建议使用 bdk-cli 或其他标准工具生成真实的 BIP32 密钥"
else
    echo "使用 bdk-cli 生成密钥..."
    
    # 生成 Alice 的密钥
    echo "生成 Alice 的密钥..."
    ALICE_MNEMONIC=$(bdk-cli key generate | grep -oP 'mnemonic: \K[^"]+')
    ALICE_XPUB=$(bdk-cli key derive --mnemonic "$ALICE_MNEMONIC" --path "m/84'/1'/0'" | grep -oP 'xpub: \K[^\s]+')
    
    # 生成 Bob 的密钥
    echo "生成 Bob 的密钥..."
    BOB_MNEMONIC=$(bdk-cli key generate | grep -oP 'mnemonic: \K[^"]+')
    BOB_XPUB=$(bdk-cli key derive --mnemonic "$BOB_MNEMONIC" --path "m/84'/1'/0'" | grep -oP 'xpub: \K[^\s]+')
    
    echo ""
    echo "导出以下环境变量:"
    echo "export ALICE_XPUB='$ALICE_XPUB'"
    echo "export BOB_XPUB='$BOB_XPUB'"
    echo ""
    echo "或者保存到文件:"
    echo "echo 'export ALICE_XPUB=\"$ALICE_XPUB\"' > test_keys.env"
    echo "echo 'export BOB_XPUB=\"$BOB_XPUB\"' >> test_keys.env"
fi

echo ""
echo "✓ 密钥生成完成"







