# RGB Interfaces

**RGB20/21/25 标准接口实现**

> **注意**: 此代码是从 [RGB-WG/rgb-interfaces](https://github.com/RGB-WG/rgb-interfaces) v0.12.0 fork 而来，  
> 现已完全私有化为 RGB-X 的一部分。不再与上游同步。

---

## 概述

此模块提供标准的 RGB 智能合约接口，包括：

- **RGB20**: 可替代代币标准 (Fungible Tokens)
- **RGB21**: NFT 标准 (Non-Fungible Tokens)
- **RGB25**: 其他标准接口

---

## 模块结构

| 文件 | 说明 |
|-----|------|
| `src/fungible.rs` | RGB20 - Amount, Precision 等 |
| `src/nft.rs` | RGB21 - NFT 相关类型 |
| `src/types.rs` | 通用类型定义 |
| `src/names.rs` | Ticker, AssetName, Details |
| `src/por.rs` | 权益证明等其他标准 |

---

## 使用方式

### 作为 RGB-X workspace 成员

```toml
[dependencies]
rgb-interfaces = { workspace = true }
```

### 代码示例

```rust
use ifaces::{Amount, Precision, Ticker, AssetName, Details};

// 创建金额 (带精度)
let precision = Precision::from(8u8);  // 8位小数
let amount = Amount::with_precision(100_000_000, precision); // 1.00000000

// 分离整数和小数部分
let (integer, fractional) = amount.split(precision);
println!("{}.{:08}", integer, fractional);

// 代币元数据
let ticker = Ticker::from("USDT");
let name = AssetName::from("Tether USD");
let details = Details::from("USD-pegged stablecoin");
```

---

## 核心类型

### Amount

```rust
pub struct Amount(u64);
```

代表代币数量，支持：
- 算术运算 (`+`, `-`, `*`, `/`, `%`)
- 精度转换
- 整数/小数分离
- 四舍五入、向上/向下取整

**方法**:
- `with_precision(amount, precision)` - 创建带精度的金额
- `split(precision)` - 分离整数和小数
- `round(precision)` - 四舍五入
- `ceil(precision)` - 向上取整
- `floor(precision)` - 向下取整

### Precision

```rust
pub struct Precision(u8);  // 0-18
```

代表小数精度（0-18位）

**常用精度**:
- Bitcoin: 8 位 (1 BTC = 100,000,000 satoshi)
- Ethereum: 18 位
- USDT: 6 位

### 元数据类型

| 类型 | 最大长度 | 说明 |
|-----|---------|------|
| `Ticker` | 8 字节 | 代币符号 (如 "BTC") |
| `AssetName` | 40 字节 | 资产名称 (如 "Bitcoin") |
| `Details` | 256 字节 | 详细描述 |

---

## RGB20 完整示例

```rust
use rgb::{CreateParams, CodexId, Assignment};
use ifaces::{Amount, Precision, Ticker, AssetName};

// RGB20 标准 Schema ID
const RGB20_CODEX: &str = "7C15w3W1-L0T~zXw-Aeh5~kV-Zquz729-HXQFKQW-_5lX9O8";

pub fn issue_token(
    runtime: &mut RgbRuntime,
    ticker: &str,
    name: &str,
    precision: u8,
    total_supply: u64,
    initial_owner: Outpoint,
) -> Result<ContractId> {
    let codex_id = CodexId::from_str(RGB20_CODEX)?;
    
    // 创建发行参数
    let mut params = CreateParams::new_bitcoin_testnet(codex_id, ticker.to_string())
        .with_global_verified(vname!("ticker"), ticker)
        .with_global_verified(vname!("name"), name)
        .with_global_verified(vname!("precision"), precision as u16)
        .with_global_verified(vname!("issued"), total_supply);
    
    // 分配初始余额
    params.push_owned_unlocked(
        vname!("balance"),
        Assignment::new_internal(initial_owner, total_supply)
    );
    
    // 发行合约
    runtime.issue(params)
}
```

---

## 特性 (Features)

```toml
[features]
default = []
all = ["stl", "serde"]
stl = ["strict_types/armor"]  # 启用 StrictTypes 库支持
serde = ["dep:serde", ...]    # 启用 Serde 序列化
```

---

## 依赖关系

所有依赖都来自 RGB-X workspace:

- `strict_encoding` - 严格编码
- `strict_types` - 类型系统
- `commit_verify` - 承诺验证
- `bp-consensus` - Bitcoin 共识

---

## Schema 文件

`stl/` 目录包含编译好的 Schema 定义：

```
stl/
├── RGB21@0.12.0.stl          # RGB21 Schema
├── RGB21@0.12.0.sta          # RGB21 抽象语法树
├── RGB21@0.12.0.sty          # RGB21 类型
├── RGBContract@0.12.0.stl    # RGB 合约基础
├── RGBContract@0.12.0.sta
└── RGBContract@0.12.0.sty
```

---

## 开发

### 构建

```bash
cargo build -p rgb-interfaces
```

### 测试

```bash
cargo test -p rgb-interfaces
```

### 生成 Schema

```bash
cargo run --bin rgb-interfaces-stl --features stl
```

---

## 版本历史

- **v0.12.0** - Hard fork from RGB-WG/rgb-interfaces
  - 完全集成到 RGB-X workspace
  - 私有化，不再与上游同步
  - 所有依赖指向本地 workspace

---

## 许可证

Apache-2.0

原始代码来自 RGB Working Group  
Hard fork 维护: Avato Labs

---

## 参考

- **原始仓库**: https://github.com/RGB-WG/rgb-interfaces (已断开)
- **RGB 标准**: https://rgb.tech
- **RGB-X 文档**: `../QUICK_START.md`
