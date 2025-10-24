# RGB-Interfaces 集成到 RGB-X

**集成日期**: 2025-10-24  
**版本**: v0.12.0  
**状态**: ✅ 完成

---

## ✅ 集成状态

此 `rgb-interfaces` 仓库已被集成到 **RGB-X workspace** 作为本地成员。

### 与上游仓库的关系

- **上游**: https://github.com/RGB-WG/rgb-interfaces
- **当前**: RGB-X workspace 本地成员
- **版本**: v0.12.0
- **Git**: 保留独立 `.git` (可选)

---

## 🔧 使用方式

### 作为 workspace 依赖

其他 RGB-X 项目可以直接引用：

```toml
[dependencies]
rgb-interfaces = { workspace = true }
```

### 代码中使用

```rust
use ifaces::{Amount, Precision, Ticker, AssetName};

let amount = Amount::from(1000);
let precision = Precision::from(8);
let ticker = Ticker::from("BTC");
```

---

## 📦 内容

| 模块 | 说明 |
|-----|------|
| `fungible.rs` | RGB20 标准 (Amount, Precision) |
| `nft.rs` | RGB21 标准 (NFT) |
| `types.rs` | 通用类型定义 |
| `names.rs` | Ticker, AssetName, Details |
| `por.rs` | 其他标准 |

---

## 🔄 同步上游

如果需要从上游同步更新：

```bash
git remote add upstream https://github.com/RGB-WG/rgb-interfaces.git
git fetch upstream
git merge upstream/master
```

---

## 📚 文档

完整集成文档见:
- `../RGB_INTERFACES_INTEGRATION.md` - 集成详情
- `../INTEGRATION_SUMMARY.md` - 完整总结
- `../QUICK_START.md` - 快速开始

---

**维护**: Avato Labs  
**上游**: RGB Working Group

