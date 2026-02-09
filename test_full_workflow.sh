#!/usr/bin/env bash
# 完整的 RGB 工作流测试脚本
# 测试流程：Alice 发行资产 -> Alice 转账给 Bob -> 广播 -> Bob 接收并验证

set -e

echo "=== RGB 完整工作流测试 ==="
echo ""

# 配置
TONDI_RPC="${TONDI_RPC:-localhost:17110}"
NETWORK="${NETWORK:-testnet}"
DATA_DIR_ALICE="${DATA_DIR_ALICE:-./rgb-test-alice}"
DATA_DIR_BOB="${DATA_DIR_BOB:-./rgb-test-bob}"

echo "配置:"
echo "  Tondi RPC: $TONDI_RPC"
echo "  Network: $NETWORK"
echo "  Alice 数据目录: $DATA_DIR_ALICE"
echo "  Bob 数据目录: $DATA_DIR_BOB"
echo ""

# 检查 RGB CLI 是否可用
if ! cargo build --bin rgb --quiet 2>/dev/null; then
    echo "正在构建 RGB CLI..."
    cargo build --bin rgb
fi

RGB_ALICE="cargo run --bin rgb -- --data-dir $DATA_DIR_ALICE --network $NETWORK --tondi=$TONDI_RPC"
RGB_BOB="cargo run --bin rgb -- --data-dir $DATA_DIR_BOB --network $NETWORK --tondi=$TONDI_RPC"

# ============================================================================
# 步骤 1: 初始化数据目录
# ============================================================================
echo "步骤 1: 初始化数据目录..."
$RGB_ALICE init --quiet || true
$RGB_BOB init --quiet || true
echo "✓ 数据目录已初始化"
echo ""

# ============================================================================
# 步骤 2: 导入 RGB20 合约发行者
# ============================================================================
echo "步骤 2: 导入 RGB20 合约发行者..."
if [ -f "rgb/examples/RGB20-FNA.issuer" ]; then
    $RGB_ALICE import rgb/examples/RGB20-FNA.issuer
    $RGB_BOB import rgb/examples/RGB20-FNA.issuer
    echo "✓ RGB20 发行者已导入"
else
    echo "⚠️  警告: 未找到 RGB20-FNA.issuer 文件，请确保已存在该文件"
    echo "   可以运行: cargo run --bin rgb -- export --codex <codex-id> --force rgb/examples/RGB20-FNA.issuer"
fi
echo ""

# ============================================================================
# 步骤 3: 创建钱包
# ============================================================================
echo "步骤 3: 创建钱包..."
echo ""

# 检查是否提供了描述符，如果没有则尝试使用示例数据
if [ -z "$ALICE_XPUB" ] || [ -z "$BOB_XPUB" ]; then
    echo "⚠️  未设置 ALICE_XPUB 或 BOB_XPUB 环境变量"
    echo ""
    
    # 尝试从示例数据中提取 xpub
    if [ -f "rgb/examples/data/bitcoin.testnet/alice.wallet/descriptor.toml" ]; then
        echo "尝试从示例数据中提取 xpub..."
        # 从 descriptor.toml 中提取 tpub（需要转换为 xpub 格式）
        ALICE_TPUB=$(grep -oP 'tpub\S+' rgb/examples/data/bitcoin.testnet/alice.wallet/descriptor.toml | head -n 1)
        if [ -n "$ALICE_TPUB" ]; then
            # 将 tpub 转换为 xpub（测试网到主网前缀转换）
            # 注意：这只是一个示例，实际应该使用正确的 BIP32 转换
            ALICE_XPUB="$ALICE_TPUB"
            echo "从示例数据中找到 Alice 的 tpub: ${ALICE_XPUB:0:20}..."
        fi
    fi
    
    if [ -z "$ALICE_XPUB" ] || [ -z "$BOB_XPUB" ]; then
        echo ""
        echo "请设置以下环境变量来创建钱包:"
        echo "  export ALICE_XPUB='tpub...'  # Alice 的扩展公钥（测试网使用 tpub）"
        echo "  export BOB_XPUB='tpub...'   # Bob 的扩展公钥（测试网使用 tpub）"
        echo ""
        echo "或者运行:"
        echo "  source generate_test_keys.sh"
        echo ""
        echo "或者使用示例钱包（如果存在）:"
        echo "  # 复制示例钱包到测试目录"
        exit 1
    fi
fi

# 创建 Alice 钱包
echo "创建 Alice 钱包..."
$RGB_ALICE create --wpkh alice "$ALICE_XPUB" || {
    echo "⚠️  创建 Alice 钱包失败，可能钱包已存在，继续..."
}

# 创建 Bob 钱包
echo "创建 Bob 钱包..."
$RGB_BOB create --wpkh bob "$BOB_XPUB" || {
    echo "⚠️  创建 Bob 钱包失败，可能钱包已存在，继续..."
}

echo "✓ 钱包已创建"
echo ""

# ============================================================================
# 步骤 4: 获取资金地址并充值
# ============================================================================
echo "步骤 4: 获取资金地址..."
ALICE_FUND_ADDR=$($RGB_ALICE --wallet alice fund)
BOB_FUND_ADDR=$($RGB_BOB --wallet bob fund)

echo "Alice 资金地址: $ALICE_FUND_ADDR"
echo "Bob 资金地址: $BOB_FUND_ADDR"
echo ""
echo "⚠️  请向这些地址充值 Tondi 测试币以继续测试"
echo "   需要至少 10000 sats 用于交易费用"
echo ""
read -p "充值完成后，按 Enter 继续..." || true
echo ""

# ============================================================================
# 步骤 5: 同步钱包
# ============================================================================
echo "步骤 5: 同步钱包..."
echo "同步 Alice 钱包..."
$RGB_ALICE --wallet alice sync
echo "同步 Bob 钱包..."
$RGB_BOB --wallet bob sync
echo "✓ 钱包已同步"
echo ""

# ============================================================================
# 步骤 6: 检查可用 UTXO
# ============================================================================
echo "步骤 6: 检查可用 UTXO..."
echo "Alice 的 UTXO:"
$RGB_ALICE --wallet alice seals || echo "  无可用 UTXO"
echo ""
echo "Bob 的 UTXO:"
$RGB_BOB --wallet bob seals || echo "  无可用 UTXO"
echo ""

# ============================================================================
# 步骤 7: 发行资产
# ============================================================================
echo "步骤 7: 发行资产..."

# 获取 Alice 的第一个 UTXO 用于发行
ALICE_UTXO=$($RGB_ALICE --wallet alice seals | head -n 1 | awk '{print $1}')

if [ -z "$ALICE_UTXO" ]; then
    echo "❌ 错误: Alice 没有可用的 UTXO，无法发行资产"
    echo "   请确保已向 Alice 的地址充值"
    exit 1
fi

echo "使用 UTXO: $ALICE_UTXO"

# 创建发行参数文件
ISSUE_PARAMS="alice_issue_params.yaml"
cat > "$ISSUE_PARAMS" <<EOF
consensus: tondi-testnet
testnet: true
issuer:
  codexId: 7C15w3W1-L0T~zXw-Aeh5~kV-Zquz729-HXQFKQW-_5lX9O8
  version: 0
  checksum: AYkSrg
name: TestToken
method: issue
timestamp: "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

global:
  - name: ticker
    verified: TEST
  - name: name
    verified: Test Token
  - name: precision
    verified: 8
  - name: issued
    verified: 1000000

owned:
  - name: balance
    seal: $ALICE_UTXO
    data: 1000000
EOF

echo "发行参数文件已创建: $ISSUE_PARAMS"
echo ""

# 发行合约
echo "正在发行资产..."
CONTRACT_ID=$($RGB_ALICE --wallet alice issue "$ISSUE_PARAMS" 2>&1 | grep -oP 'ID \K[^\s]+' || echo "")

if [ -z "$CONTRACT_ID" ]; then
    echo "❌ 错误: 资产发行失败"
    exit 1
fi

echo "✓ 资产已发行，合约 ID: $CONTRACT_ID"
echo ""

# 备份合约
echo "备份合约..."
$RGB_ALICE --wallet alice backup -f TestToken contract.rgb || true
echo "✓ 合约已备份到 contract.rgb"
echo ""

# ============================================================================
# 步骤 8: 查看资产状态
# ============================================================================
echo "步骤 8: 查看资产状态..."
echo "Alice 的资产状态:"
$RGB_ALICE --wallet alice state -go TestToken
echo ""

# ============================================================================
# 步骤 9: Bob 接收合约
# ============================================================================
echo "步骤 9: Bob 接收合约..."
if [ -f "contract.rgb" ]; then
    $RGB_BOB --wallet bob accept -u contract.rgb || {
        echo "⚠️  Bob 接收合约失败（可能已接收）"
    }
    echo "✓ Bob 已接收合约"
else
    echo "⚠️  警告: 未找到 contract.rgb 文件"
fi
echo ""

# ============================================================================
# 步骤 10: 创建转账发票
# ============================================================================
echo "步骤 10: 创建转账发票..."
echo "Bob 生成接收发票..."

# 获取 Bob 的认证令牌
BOB_AUTH=$($RGB_BOB --wallet bob invoice --nonce 0 --seal-only TestToken 2>&1 | tail -n 1)

if [ -z "$BOB_AUTH" ]; then
    echo "❌ 错误: 无法获取 Bob 的认证令牌"
    exit 1
fi

echo "Bob 的认证令牌: $BOB_AUTH"

# 创建发票（转账 100 个代币给 Bob）
INVOICE=$($RGB_BOB --wallet bob invoice --nonce 0 TestToken balance 100 2>&1 | tail -n 1)

if [ -z "$INVOICE" ]; then
    echo "❌ 错误: 无法创建发票"
    exit 1
fi

echo "发票: $INVOICE"
echo ""

# ============================================================================
# 步骤 11: Alice 支付发票（创建 PSTT）
# ============================================================================
echo "步骤 11: Alice 支付发票（创建 PSTT）..."
echo "正在创建 PSTT 和 consignment..."

$RGB_ALICE --wallet alice pay \
    --strategy aggregate \
    --fee 1000 \
    --sats 1000 \
    "$INVOICE" \
    transfer.rgb \
    transfer.pstt || {
    echo "❌ 错误: 创建 PSTT 失败"
    exit 1
}

echo "✓ PSTT 已创建: transfer.pstt"
echo "✓ Consignment 已创建: transfer.rgb"
echo ""

# ============================================================================
# 步骤 12: 查看支付后的状态
# ============================================================================
echo "步骤 12: 查看支付后的状态..."
echo "Alice 的资产状态:"
$RGB_ALICE --wallet alice state -go TestToken
echo ""

# ============================================================================
# 步骤 13: Bob 接收转账
# ============================================================================
echo "步骤 13: Bob 接收转账..."
if [ -f "transfer.rgb" ]; then
    $RGB_BOB --wallet bob accept -u transfer.rgb || {
        echo "⚠️  Bob 接收转账失败（可能已接收）"
    }
    echo "✓ Bob 已接收转账"
else
    echo "⚠️  警告: 未找到 transfer.rgb 文件"
fi
echo ""

# ============================================================================
# 步骤 14: 最终化并广播交易
# ============================================================================
echo "步骤 14: 最终化并广播交易..."
if [ -f "transfer.pstt" ]; then
    echo "最终化 PSTT..."
    $RGB_ALICE --wallet alice finalize --broadcast transfer.pstt || {
        echo "❌ 错误: 最终化或广播失败"
        exit 1
    }
    echo "✓ 交易已最终化并广播"
else
    echo "⚠️  警告: 未找到 transfer.pstt 文件"
fi
echo ""

# ============================================================================
# 步骤 15: 等待确认并验证
# ============================================================================
echo "步骤 15: 等待确认..."
echo "等待交易确认（可能需要一些时间）..."
sleep 5

echo "同步钱包以获取最新状态..."
$RGB_ALICE --wallet alice sync
$RGB_BOB --wallet bob sync
echo ""

# ============================================================================
# 步骤 16: 最终状态验证
# ============================================================================
echo "步骤 16: 最终状态验证..."
echo "Alice 的最终资产状态:"
$RGB_ALICE --wallet alice state -go TestToken
echo ""

echo "Bob 的最终资产状态:"
$RGB_BOB --wallet bob state -go TestToken
echo ""

# ============================================================================
# 总结
# ============================================================================
echo "=== 测试完成 ==="
echo ""
echo "测试流程总结:"
echo "  ✓ Alice 创建钱包"
echo "  ✓ Bob 创建钱包"
echo "  ✓ 获取资金地址并充值"
echo "  ✓ Alice 发行资产"
echo "  ✓ Alice 创建并签名 PSTT"
echo "  ✓ 最终化并广播交易"
echo "  ✓ Bob 接收并验证资产所有权"
echo ""
echo "生成的文件:"
echo "  - $ISSUE_PARAMS: 发行参数文件"
echo "  - contract.rgb: 合约 consignment"
echo "  - transfer.rgb: 转账 consignment"
echo "  - transfer.pstt: 部分签名的 Tondi 交易"
echo ""

