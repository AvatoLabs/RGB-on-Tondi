#!/usr/bin/env bash
# 简化版 RGB 工作流测试脚本
# 使用示例数据或现有钱包进行测试

set -e

echo "=== RGB 简化工作流测试 ==="
echo ""

# 配置
TONDI_RPC="${TONDI_RPC:-localhost:17110}"
NETWORK="${NETWORK:-testnet}"
USE_EXAMPLES="${USE_EXAMPLES:-false}"

if [ "$USE_EXAMPLES" = "true" ]; then
    DATA_DIR_ALICE="./rgb/examples/data"
    DATA_DIR_BOB="./rgb/examples/data2"
    WALLET_ALICE="alice"
    WALLET_BOB="bob"
else
    DATA_DIR_ALICE="${DATA_DIR_ALICE:-./rgb-test-alice}"
    DATA_DIR_BOB="${DATA_DIR_BOB:-./rgb-test-bob}"
    WALLET_ALICE="${WALLET_ALICE:-alice}"
    WALLET_BOB="${WALLET_BOB:-bob}"
fi

echo "配置:"
echo "  Tondi RPC: $TONDI_RPC"
echo "  Network: $NETWORK"
echo "  Alice 数据目录: $DATA_DIR_ALICE"
echo "  Bob 数据目录: $DATA_DIR_BOB"
echo "  使用示例数据: $USE_EXAMPLES"
echo ""

# 检查 RGB CLI
if ! cargo build --bin rgb --quiet 2>/dev/null; then
    echo "正在构建 RGB CLI..."
    cargo build --bin rgb
fi

RGB_ALICE="cargo run --bin rgb -- --data-dir $DATA_DIR_ALICE --network $NETWORK --tondi=$TONDI_RPC"
RGB_BOB="cargo run --bin rgb -- --data-dir $DATA_DIR_BOB --network $NETWORK --tondi=$TONDI_RPC"

# 初始化（如果需要）
if [ "$USE_EXAMPLES" != "true" ]; then
    echo "初始化数据目录..."
    $RGB_ALICE init --quiet || true
    $RGB_BOB init --quiet || true
fi

# 导入 RGB20 发行者
echo "导入 RGB20 合约发行者..."
if [ -f "rgb/examples/RGB20-FNA.issuer" ]; then
    $RGB_ALICE import rgb/examples/RGB20-FNA.issuer || true
    $RGB_BOB import rgb/examples/RGB20-FNA.issuer || true
fi

# 显示可用命令
echo ""
echo "=== 可用命令 ==="
echo ""
echo "1. 查看钱包列表:"
echo "   $RGB_ALICE wallets"
echo "   $RGB_BOB wallets"
echo ""
echo "2. 获取资金地址:"
echo "   $RGB_ALICE --wallet $WALLET_ALICE fund"
echo "   $RGB_BOB --wallet $WALLET_BOB fund"
echo ""
echo "3. 同步钱包:"
echo "   $RGB_ALICE --wallet $WALLET_ALICE sync"
echo "   $RGB_BOB --wallet $WALLET_BOB sync"
echo ""
echo "4. 查看 UTXO:"
echo "   $RGB_ALICE --wallet $WALLET_ALICE seals"
echo "   $RGB_BOB --wallet $WALLET_BOB seals"
echo ""
echo "5. 发行资产（需要创建参数文件）:"
echo "   $RGB_ALICE --wallet $WALLET_ALICE issue <params.yaml>"
echo ""
echo "6. 查看合约:"
echo "   $RGB_ALICE contracts"
echo ""
echo "7. 查看资产状态:"
echo "   $RGB_ALICE --wallet $WALLET_ALICE state -go <contract>"
echo ""
echo "8. 生成发票:"
echo "   $RGB_BOB --wallet $WALLET_BOB invoice --nonce 0 <contract> <state> <value>"
echo ""
echo "9. 支付发票:"
echo "   $RGB_ALICE --wallet $WALLET_ALICE pay --strategy aggregate --fee 1000 --sats 1000 <invoice> <consignment.rgb> <pstt.pstt>"
echo ""
echo "10. 接收转账:"
echo "    $RGB_BOB --wallet $WALLET_BOB accept -u <consignment.rgb>"
echo ""
echo "11. 最终化并广播:"
echo "    $RGB_ALICE --wallet $WALLET_ALICE finalize --broadcast <pstt.pstt>"
echo ""
echo "=== 完整工作流示例 ==="
echo ""
echo "要运行完整工作流，请:"
echo "1. 确保已设置 ALICE_XPUB 和 BOB_XPUB 环境变量"
echo "2. 运行: bash test_full_workflow.sh"
echo ""
echo "或使用示例数据:"
echo "  USE_EXAMPLES=true bash test_workflow_simple.sh"
echo ""








