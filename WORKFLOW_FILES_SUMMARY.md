# RGB 完整工作流测试文件总结

本文档总结了为完整 RGB 工作流测试创建的所有文件。

## 📁 创建的文件

### 1. `test_full_workflow.sh`
**完整自动化测试脚本**

这是主要的测试脚本，自动执行完整的 RGB 工作流：
- 创建钱包（Alice 和 Bob）
- 获取资金地址
- 发行资产
- 创建并签名 PSTT
- 最终化并广播交易
- Bob 接收并验证资产所有权

**使用方法：**
```bash
# 设置环境变量
export ALICE_XPUB='tpub...'
export BOB_XPUB='tpub...'

# 运行脚本
bash test_full_workflow.sh
```

### 2. `test_workflow_simple.sh`
**简化版测试脚本（命令参考）**

这个脚本不执行实际操作，而是显示所有可用的命令和步骤说明。适合：
- 学习各个命令
- 手动执行步骤
- 调试特定步骤

**使用方法：**
```bash
bash test_workflow_simple.sh
```

### 3. `generate_test_keys.sh`
**测试密钥生成脚本**

生成用于测试的 xpub/tpub 密钥对。

**使用方法：**
```bash
bash generate_test_keys.sh
# 然后从输出中复制环境变量设置命令
```

### 4. `issue_params_template.yaml`
**资产发行参数模板**

RGB20 资产发行的 YAML 参数文件模板。

**使用方法：**
1. 复制模板文件
2. 替换 `<REPLACE_WITH_UTXO>` 为实际 UTXO
3. 根据需要修改其他参数
4. 使用 `rgb issue` 命令发行

### 5. `TEST_WORKFLOW_README.md`
**详细使用文档**

包含：
- 完整的工作流说明
- 每个步骤的详细命令
- 配置选项
- 常见问题解答
- 测试检查清单

### 6. `WORKFLOW_FILES_SUMMARY.md`（本文件）
**文件总结文档**

## 🚀 快速开始

### 方法 1: 完整自动化（推荐）

```bash
# 1. 生成测试密钥
bash generate_test_keys.sh
# 复制输出的环境变量设置命令并执行

# 2. 运行完整测试
bash test_full_workflow.sh
```

### 方法 2: 使用示例数据

```bash
USE_EXAMPLES=true bash test_workflow_simple.sh
```

### 方法 3: 手动执行

参考 `TEST_WORKFLOW_README.md` 中的详细步骤。

## 📋 工作流步骤

1. **初始化** - 创建数据目录
2. **导入发行者** - 导入 RGB20 合约发行者
3. **创建钱包** - 为 Alice 和 Bob 创建钱包
4. **获取地址** - 获取资金地址并充值
5. **同步钱包** - 同步钱包状态
6. **发行资产** - Alice 发行 RGB20 代币
7. **接收合约** - Bob 接收合约
8. **创建发票** - Bob 生成接收发票
9. **支付发票** - Alice 创建 PSTT
10. **接收转账** - Bob 接收转账
11. **广播交易** - 最终化并广播交易
12. **验证状态** - 验证资产所有权

## 🔧 配置

### 环境变量

- `TONDI_RPC`: Tondi RPC 端点（默认: `localhost:17110`）
- `NETWORK`: 网络类型（默认: `testnet`）
- `DATA_DIR_ALICE`: Alice 数据目录（默认: `./rgb-test-alice`）
- `DATA_DIR_BOB`: Bob 数据目录（默认: `./rgb-test-bob`）
- `ALICE_XPUB`: Alice 的扩展公钥（必需）
- `BOB_XPUB`: Bob 的扩展公钥（必需）

### 命令行参数

主要 RGB CLI 参数：
- `--data-dir <DIR>`: 数据目录
- `--network <NETWORK>`: 网络（testnet/mainnet）
- `--tondi=<ENDPOINT>`: Tondi RPC 端点
- `--wallet <NAME>`: 钱包名称

## 📝 文件结构

```
RGB-on-Tondi/
├── test_full_workflow.sh          # 完整自动化测试
├── test_workflow_simple.sh         # 简化版测试（命令参考）
├── generate_test_keys.sh           # 密钥生成脚本
├── issue_params_template.yaml      # 发行参数模板
├── TEST_WORKFLOW_README.md         # 详细使用文档
└── WORKFLOW_FILES_SUMMARY.md       # 本文件
```

## ⚠️ 注意事项

1. **测试环境**: 这些脚本仅用于测试，不要在生产环境使用测试密钥
2. **网络连接**: 确保 Tondi 节点正在运行并可访问
3. **资金要求**: 需要至少 10000 sats 用于交易费用
4. **密钥安全**: 测试密钥不应用于存储真实资产

## 🔗 相关资源

- RGB 文档: https://rgb.tech
- Tondi 文档: [项目文档]
- RGB CLI 帮助: `cargo run --bin rgb -- --help`

## ✅ 测试检查清单

运行测试前，确保：

- [ ] Tondi 节点运行中
- [ ] RGB CLI 已构建（`cargo build --bin rgb`）
- [ ] 已设置 `ALICE_XPUB` 和 `BOB_XPUB`
- [ ] 钱包已充值（至少 10000 sats）
- [ ] RGB20 发行者文件存在（`rgb/examples/RGB20-FNA.issuer`）

## 🐛 故障排除

如果遇到问题，请：

1. 查看 `TEST_WORKFLOW_README.md` 中的常见问题部分
2. 检查 Tondi 节点连接
3. 验证钱包状态（`rgb --wallet <name> seals`）
4. 查看 RGB CLI 的错误输出

---

**创建时间**: 2024-12-18
**版本**: 1.0
**状态**: ✅ 完整实现







