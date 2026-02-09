# RGB 完整工作流测试指南

本指南说明如何运行完整的 RGB 工作流测试，包括资产发行、转账、广播和验证。

## 📋 测试流程概述

完整测试流程包括以下步骤：

1. **创建钱包** - 为 Alice 和 Bob 创建钱包
2. **获取资金地址并充值** - 获取 Tondi 测试币地址
3. **发行资产** - Alice 发行 RGB20 代币
4. **创建并签名 PSTT** - Alice 创建转账交易
5. **最终化并广播交易** - 将交易广播到 Tondi 网络
6. **Bob 接收并验证** - Bob 接收资产并验证所有权

## 🚀 快速开始

### 方法 1: 使用完整自动化脚本

```bash
# 1. 生成测试密钥对
bash generate_test_keys.sh

# 2. 设置环境变量（从 generate_test_keys.sh 的输出）
export ALICE_XPUB='tpub...'
export BOB_XPUB='tpub...'

# 3. 运行完整测试
bash test_full_workflow.sh
```

### 方法 2: 使用示例数据

```bash
# 使用现有的示例钱包
USE_EXAMPLES=true bash test_workflow_simple.sh
```

### 方法 3: 手动执行步骤

参考 `test_workflow_simple.sh` 中列出的命令，逐步执行。

## 📝 详细步骤说明

### 步骤 1: 初始化数据目录

```bash
# Alice
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet --tondi=localhost:17110 init

# Bob
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet --tondi=localhost:17110 init
```

### 步骤 2: 导入 RGB20 合约发行者

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet import rgb/examples/RGB20-FNA.issuer
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet import rgb/examples/RGB20-FNA.issuer
```

### 步骤 3: 创建钱包

需要提供扩展公钥（xpub 或 tpub）：

```bash
# 创建 Alice 钱包
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  create --wpkh alice "tpub..."

# 创建 Bob 钱包
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  create --wpkh bob "tpub..."
```

**获取 xpub/tpub 的方法：**

1. **使用 bdk-cli**:
   ```bash
   bdk-cli key generate
   bdk-cli key derive --mnemonic "<mnemonic>" --path "m/84'/1'/0'"
   ```

2. **使用现有钱包**: 从其他钱包软件导出

3. **使用测试脚本**: `bash generate_test_keys.sh`

### 步骤 4: 获取资金地址

```bash
# Alice 的资金地址
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --wallet alice fund

# Bob 的资金地址
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  --wallet bob fund
```

向这些地址充值 Tondi 测试币（至少 10000 sats 用于交易费用）。

### 步骤 5: 同步钱包

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --tondi=localhost:17110 --wallet alice sync

cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  --tondi=localhost:17110 --wallet bob sync
```

### 步骤 6: 检查可用 UTXO

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --wallet alice seals
```

### 步骤 7: 发行资产

创建发行参数文件 `issue_params.yaml`:

```yaml
consensus: tondi-testnet
testnet: true
issuer:
  codexId: 7C15w3W1-L0T~zXw-Aeh5~kV-Zquz729-HXQFKQW-_5lX9O8
  version: 0
  checksum: AYkSrg
name: TestToken
method: issue
timestamp: "2024-12-18T10:32:00Z"

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
    seal: <UTXO_FROM_STEP_6>
    data: 1000000
```

发行资产：

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --tondi=localhost:17110 --wallet alice issue issue_params.yaml
```

备份合约：

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --wallet alice backup -f TestToken contract.rgb
```

### 步骤 8: Bob 接收合约

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  --wallet bob accept -u contract.rgb
```

### 步骤 9: 创建转账发票

Bob 生成接收发票：

```bash
# 获取认证令牌
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  --wallet bob invoice --nonce 0 --seal-only TestToken

# 创建发票（转账 100 个代币）
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  --wallet bob invoice --nonce 0 TestToken balance 100
```

### 步骤 10: Alice 支付发票

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --tondi=localhost:17110 --wallet alice pay \
  --strategy aggregate \
  --fee 1000 \
  --sats 1000 \
  "<INVOICE>" \
  transfer.rgb \
  transfer.pstt
```

### 步骤 11: Bob 接收转账

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  --wallet bob accept -u transfer.rgb
```

### 步骤 12: 最终化并广播交易

```bash
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --tondi=localhost:17110 --wallet alice finalize --broadcast transfer.pstt
```

### 步骤 13: 验证最终状态

```bash
# Alice 的状态
cargo run --bin rgb -- --data-dir ./rgb-test-alice --network testnet \
  --wallet alice state -go TestToken

# Bob 的状态
cargo run --bin rgb -- --data-dir ./rgb-test-bob --network testnet \
  --wallet bob state -go TestToken
```

## 🔧 配置选项

### 环境变量

- `TONDI_RPC`: Tondi 节点 RPC 地址（默认: `localhost:17110`）
- `NETWORK`: 网络类型（默认: `testnet`）
- `DATA_DIR_ALICE`: Alice 的数据目录（默认: `./rgb-test-alice`）
- `DATA_DIR_BOB`: Bob 的数据目录（默认: `./rgb-test-bob`）
- `ALICE_XPUB`: Alice 的扩展公钥
- `BOB_XPUB`: Bob 的扩展公钥

### 命令行参数

RGB CLI 的主要参数：

- `--data-dir <DIR>`: 数据目录
- `--network <NETWORK>`: 网络（testnet/mainnet）
- `--tondi=<ENDPOINT>`: Tondi RPC 端点
- `--wallet <NAME>`: 钱包名称

## 📊 预期输出

### 成功发行资产

```
A new contract issued with ID <contract-id>
```

### 成功创建 PSTT

```
PSTT 已创建: transfer.pstt
Consignment 已创建: transfer.rgb
```

### 成功广播交易

```
交易已最终化并广播
```

### 验证资产所有权

Bob 的状态应显示：

```
Owned:  balance    <confirmation>    100    <rgb-output>    <outpoint>
```

## ⚠️ 常见问题

### 1. 钱包创建失败

**问题**: `unable to create wallet`

**解决**: 
- 确保提供了有效的 xpub/tpub
- 检查钱包名称是否已存在
- 使用 `--force` 标志覆盖现有钱包（谨慎使用）

### 2. 没有可用 UTXO

**问题**: `No UTXOs available`

**解决**:
- 确保已向资金地址充值
- 运行 `sync` 命令同步钱包
- 检查 Tondi 节点连接

### 3. 资产发行失败

**问题**: `Unable to issue contract`

**解决**:
- 检查参数文件格式是否正确
- 确保 UTXO 存在且可用
- 验证合约发行者已导入

### 4. PSTT 创建失败

**问题**: `Unable to create PSTT`

**解决**:
- 确保有足够的 UTXO 用于交易
- 检查发票格式是否正确
- 验证钱包已同步

### 5. 广播失败

**问题**: `Unable to broadcast transaction`

**解决**:
- 检查 Tondi 节点连接
- 确保交易已完全签名
- 验证网络参数匹配

## 📚 相关文件

- `test_full_workflow.sh`: 完整自动化测试脚本
- `test_workflow_simple.sh`: 简化版测试脚本（显示命令）
- `generate_test_keys.sh`: 生成测试密钥对
- `rgb/examples/DemoToken.yaml`: 示例发行参数文件
- `rgb/examples/demo.sh`: 原始示例脚本

## 🔗 更多资源

- RGB 文档: https://rgb.tech
- Tondi 文档: [项目文档]
- RGB CLI 帮助: `cargo run --bin rgb -- --help`

## ✅ 测试检查清单

- [ ] Tondi 节点运行中
- [ ] RGB CLI 已构建
- [ ] 钱包已创建
- [ ] 钱包已充值
- [ ] 合约发行者已导入
- [ ] 资产已发行
- [ ] PSTT 已创建
- [ ] 交易已广播
- [ ] 资产所有权已验证

---

**注意**: 本测试脚本仅用于开发和测试目的。在生产环境中使用前，请确保充分理解所有步骤和风险。







