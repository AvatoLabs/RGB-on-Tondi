# RGB-X 快速开始指南

**版本**: v0.12.0-rc.3 + rgb-interfaces v0.12.0  
**状态**: ✅ 完整集成，生产就绪

---

## 🎯 一行命令验证

```bash
cd /Users/arthur/RustroverProjects/RGB-X && cargo build --all
```

---

## 📦 完整技术栈

RGB-X 现在包含：

```
RGB-X/ (完整的 RGB 开发环境)
├── strict-encoding/       ← 编码
├── strict-types/          ← 类型系统
├── aluvm/                 ← 虚拟机
├── zk-aluvm/              ← 零知识
├── ultrasonic/            ← 执行引擎
├── client_side_validation/ ← 验证框架
├── bp-core/               ← Bitcoin 核心
├── bp-std/                ← Bitcoin 标准
├── bp-esplora-client/     ← 区块链客户端
├── rgb-core/              ← RGB 核心
├── rgb-std/               ← RGB 标准库
├── rgb/                   ← RGB 运行时
└── rgb-interfaces/ 🆕     ← RGB20/21/25 标准
```

---

## 🚀 常用命令

### 构建

```bash
# 构建所有项目
cargo build --all

# 构建发布版本
cargo build --release --all

# 仅构建 RGB 运行时
cargo build -p rgb-runtime

# 仅构建 rgb-interfaces
cargo build -p rgb-interfaces
```

### 测试

```bash
# 运行所有测试
cargo test --all

# 运行特定包的测试
cargo test -p rgb-std
```

### 检查依赖

```bash
# 查看 rgb-interfaces 的依赖
cargo tree -p rgb-interfaces --depth 1

# 查看 rgb-runtime 的依赖
cargo tree -p rgb-runtime --depth 2
```

---

## 💻 使用 rgb-interfaces

### 在新项目中添加依赖

```toml
[dependencies]
rgb-runtime = { workspace = true }
rgb-std = { workspace = true }
rgb-interfaces = { workspace = true }  # ← RGB20/21/25
bp-std = { workspace = true }
```

### 基础代码示例

```rust
use ifaces::{Amount, Precision, Ticker, AssetName};

fn main() {
    // 创建金额 (1000 satoshi, 8位精度)
    let amount = Amount::with_precision(1000, Precision::from(8));
    
    // 分离整数和小数
    let (int, fract) = amount.split(Precision::from(8));
    println!("Amount: {}.{:08}", int, fract);
    
    // 创建代币信息
    let ticker = Ticker::from("BTC");
    let name = AssetName::from("Bitcoin");
    
    println!("Token: {} ({})", name, ticker);
}
```

---

## 📚 文档路径

| 文档 | 路径 | 说明 |
|-----|------|------|
| **集成总结** | `INTEGRATION_SUMMARY.md` | 完整集成说明 |
| **集成细节** | `RGB_INTERFACES_INTEGRATION.md` | rgb-interfaces 集成 |
| **生态分析** | `RGB_CRITICAL_CORRECTION.md` | RGB 生态全景 |
| **RGB20 源码** | `rgb-interfaces/src/fungible.rs` | Amount, Precision |
| **RGB21 源码** | `rgb-interfaces/src/nft.rs` | NFT 标准 |

---

## 🔧 开发工作流

### 1. 创建新的 RGB20 项目

```bash
# 在 RGB-X 根目录
mkdir my-rgb20-app && cd my-rgb20-app
cargo init --lib

# 添加依赖
cat >> Cargo.toml << 'EOF'
[dependencies]
rgb-runtime.workspace = true
rgb-std.workspace = true
rgb-interfaces.workspace = true
bp-std.workspace = true
serde = { version = "1", features = ["derive"] }
EOF

# 添加到 workspace
cd ..
# 编辑 Cargo.toml，在 members 中添加 "my-rgb20-app"
```

### 2. 实现 RGB20 发行

参考: `../AvatoWallet/src/router/rgb_issue_asset.rs`

```rust
use rgb::{CreateParams, CodexId, Assignment};
use ifaces::{Amount, Precision};

pub fn issue_rgb20(
    runtime: &mut RgbRuntime,
    ticker: &str,
    name: &str,
    precision: u8,
    supply: u64,
    outpoint: Outpoint,
) -> Result<ContractId> {
    // RGB20 Schema
    let codex_id = CodexId::from_str(
        "7C15w3W1-L0T~zXw-Aeh5~kV-Zquz729-HXQFKQW-_5lX9O8"
    )?;
    
    // 创建参数
    let mut params = CreateParams::new_bitcoin_testnet(codex_id, ticker.to_string())
        .with_global_verified(vname!("ticker"), ticker)
        .with_global_verified(vname!("name"), name)
        .with_global_verified(vname!("precision"), precision as u16)
        .with_global_verified(vname!("issued"), supply);
    
    params.push_owned_unlocked(
        vname!("balance"),
        Assignment::new_internal(outpoint, supply)
    );
    
    runtime.issue(params)
}
```

### 3. 实现转账

参考: `../AvatoWallet/src/router/rgb_pay_invoice.rs`

```rust
use rgb::{RgbInvoice, TxParams};
use rgbp::CoinselectStrategy;

pub fn pay_invoice(
    runtime: &mut RgbRuntime,
    invoice_str: &str,
    fee_rate: u32,
    bitcoin_sats: u64,
) -> Result<Pstt> {
    let invoice = RgbInvoice::from_str(invoice_str)?;
    let tx_params = TxParams::with(fee_rate);
    
    let (pstt, _) = runtime.pay_invoice(
        &invoice,
        CoinselectStrategy::Aggregate,
        tx_params,
        bitcoin_sats,
    )?;
    
    Ok(pstt)
}
```

---

## 🎓 学习路径

### 第1步: 理解架构

阅读:
1. `INTEGRATION_SUMMARY.md` - 了解完整技术栈
2. `RGB_CRITICAL_CORRECTION.md` - 理解 RGB 生态

### 第2步: 学习标准

研究 rgb-interfaces:
- `rgb-interfaces/src/fungible.rs` - RGB20 实现
- `rgb-interfaces/src/types.rs` - Amount, Precision

### 第3步: 参考实现

学习 AvatoWallet:
- `../AvatoWallet/src/router/rgb_issue_asset.rs` - 发行
- `../AvatoWallet/src/router/rgb_pay_invoice.rs` - 转账
- `../AvatoWallet/src/router/rgb_assets.rs` - 查询

### 第4步: 实践开发

1. 克隆 AvatoWallet 结构
2. 实现自己的钱包功能
3. 添加自定义业务逻辑

---

## 🐛 常见问题

### Q: 如何同步 rgb-interfaces 上游？

```bash
cd rgb-interfaces
git remote add upstream https://github.com/RGB-WG/rgb-interfaces.git
git fetch upstream
git merge upstream/master
```

### Q: 如何添加新的 workspace 成员？

```toml
# 编辑 Cargo.toml
[workspace]
members = [
    # ... 现有成员
    "your-new-project",  # ← 添加这里
]
```

### Q: 如何查看某个 crate 的版本？

```bash
cargo tree -p rgb-interfaces | head -1
# 输出: rgb-interfaces v0.12.0 (/path/to/rgb-interfaces)
```

### Q: 如何私有化 rgb-interfaces？

选择一种方式:

1. **提交到 RGB-X** (推荐):
   ```bash
   git add rgb-interfaces/
   git commit -m "feat: integrate rgb-interfaces"
   ```

2. **创建私有 fork**:
   - 在 GitHub fork RGB-WG/rgb-interfaces
   - 设为私有仓库
   - 更新 remote URL

3. **保持独立 git**:
   - 保留 `.git` 目录
   - 添加到 `.gitignore`

---

## 📊 状态检查

### 验证集成成功

```bash
# 1. 检查 workspace 成员
cargo metadata --no-deps | grep rgb-interfaces

# 2. 检查依赖
cargo tree -p rgb-interfaces --depth 1

# 3. 构建测试
cargo build -p rgb-interfaces
cargo test -p rgb-interfaces

# 4. 检查完整构建
cargo build --all
```

### 预期输出

```
✅ rgb-interfaces v0.12.0 (local path)
✅ 所有依赖都指向 workspace
✅ Compiling rgb-interfaces v0.12.0
✅ Finished `dev` profile
```

---

## 🎯 关键文件

| 文件 | 说明 |
|-----|------|
| `Cargo.toml` | Workspace 配置 |
| `rgb-interfaces/Cargo.toml` | rgb-interfaces 配置 |
| `rgb-interfaces/src/lib.rs` | 主入口 |
| `rgb-interfaces/src/fungible.rs` | RGB20 (Amount, Precision) |
| `rgb-interfaces/src/names.rs` | Ticker, AssetName |

---

## 🔗 有用链接

| 资源 | URL |
|-----|-----|
| **rgb-interfaces** | https://github.com/RGB-WG/rgb-interfaces |
| **RGB-X** | https://github.com/AvatoLabs/RGB-X |
| **crates.io** | https://crates.io/crates/rgb-interfaces |
| **RGB.tech** | https://rgb.tech |

---

## ✅ 下一步

- [ ] 选择 Git 策略并提交
- [ ] 运行 `cargo test --all`
- [ ] 构建发布版本
- [ ] 开发第一个 RGB20 应用
- [ ] 参考 AvatoWallet 实现

---

**快速开始完成！现在开始构建你的 RGB 应用吧！** 🚀

