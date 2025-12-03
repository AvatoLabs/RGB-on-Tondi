# RGB-on-Tondi L1 Migration Adaptation Guide

## 概述

本文档详细描述了将RGB协议栈从Bitcoin L1迁移适配到Tondi L1所需的全部修改。Tondi是一个具有Taproot支持的Kaspa fork，主要使用BLAKE3作为哈希算法。

## Tondi L1 关键技术规格

根据TSP-0001和TSP-0002标准提案：

### 1. 哈希算法 (TSP-0002)

| 用途 | 原Bitcoin | Tondi |
|------|----------|-------|
| 交易ID (Txid) | 双重SHA256 | BLAKE3-256 |
| 区块哈希 | 双重SHA256 | BLAKE3-256 |
| Merkle树 | SHA256 | BLAKE3-256 |
| 脚本哈希 (P2SH) | RIPEMD160(SHA256) | BLAKE3-256 |
| RGB状态承诺 | 标签SHA256 | 域分离BLAKE3 |
| 公钥哈希 | RIPEMD160(SHA256) | BLAKE3-256 (截断到20字节) |

### 2. 域分离规范

Tondi使用BLAKE3的`derive_key`功能进行域分离：

```rust
// 域分离标签格式
blake3::derive_key("tondi.block.hash", header_bytes)
blake3::derive_key("tondi.txid", tx_bytes)
blake3::derive_key("tondi.contract.state", state_bytes)
blake3::derive_key("tondi.rgb.commitment", commitment_bytes)
blake3::derive_key("tondi.merkle.node", node_bytes)
```

### 3. 地址格式 (TSP-0001)

| 网络 | Bitcoin前缀 | Tondi前缀 |
|------|------------|----------|
| 主网 | `bc` | `tondi` |
| 测试网 | `tb` | `tondi0` |
| 模拟网 | `bcrt` | `tondisim` |
| 开发网 | - | `tondidev` |

### 4. Taproot (Copperoot)

Tondi的Taproot实现称为Copperoot，主要区别：

| 特性 | Bitcoin Taproot | Tondi Copperoot |
|------|----------------|-----------------|
| 哈希函数 | SHA256 | BLAKE3-256 |
| 脚本版本 | v1 (88) | CopperootMerkle: v2 (192), CopperootVerkle: v3 (96) |
| 控制块 | BIP341标准 | TLV扩展格式 |
| 签名哈希 | BIP341 TapSighash | CopperootSighash |

---

## 迁移任务清单

### 阶段1: 核心哈希基础设施

#### 1.1 添加BLAKE3依赖

**文件**: `Cargo.toml` (workspace根目录)

```toml
[workspace.dependencies]
blake3 = "1.5"
```

**需要更新的Cargo.toml**:
- `client_side_validation/commit_verify/Cargo.toml`
- `bp-core/consensus/Cargo.toml`
- `bp-core/Cargo.toml`
- `bp-std/Cargo.toml`
- `rgb-core/Cargo.toml`
- `rgb-std/Cargo.toml`

#### 1.2 迁移 `commit_verify/src/digest.rs`

**当前实现**:
```rust
pub use sha2::{Digest, Sha256};

impl DigestExt for Sha256 {
    fn from_tag(tag: impl AsRef<[u8]>) -> Self {
        // BIP-340风格的双重SHA256标签
        let mut tagger = Sha256::default();
        tagger.update(tag);
        let tag = tagger.finalize();
        let mut engine = Sha256::default();
        engine.update(tag);
        engine.update(tag);
        engine
    }
}
```

**迁移后**:
```rust
pub use blake3::Hasher as Blake3;

/// BLAKE3哈希器包装，实现DigestExt trait
pub struct TondiBlake3 {
    hasher: Blake3,
}

impl TondiBlake3 {
    pub fn new() -> Self {
        Self { hasher: Blake3::new() }
    }
    
    /// 使用域分离创建哈希器
    pub fn from_context(context: &str) -> Self {
        Self {
            hasher: Blake3::new_derive_key(context)
        }
    }
}

impl DigestExt for TondiBlake3 {
    fn from_tag(tag: impl AsRef<[u8]>) -> Self {
        // Tondi域分离: 使用derive_key
        let context = format!("tondi.{}", String::from_utf8_lossy(tag.as_ref()));
        Self::from_context(&context)
    }
    
    fn input_raw(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }
    
    fn finish(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

// 保留SHA256用于向后兼容和跨链操作
#[cfg(feature = "sha256-compat")]
pub use sha2::{Digest, Sha256};
```

#### 1.3 迁移 `commit_verify/src/merkle.rs`

**修改点**:
```rust
// 修改前
use sha2::Sha256;
impl From<Sha256> for MerkleHash {
    fn from(hash: Sha256) -> Self { hash.finish().into() }
}

// 修改后
use crate::digest::TondiBlake3;
impl From<TondiBlake3> for MerkleHash {
    fn from(hash: TondiBlake3) -> Self { hash.finish().into() }
}
```

**更新MerkleHash标签**:
```rust
impl CommitmentId for MerkleHash {
    // 修改前
    const TAG: &'static str = "urn:ubideco:merkle:node#2024-01-31";
    // 修改后
    const TAG: &'static str = "urn:tondi:merkle:node#2025-01-01";
}
```

#### 1.4 迁移 `commit_verify/src/id.rs`

**修改CommitEngine**:
```rust
// 修改前
pub struct CommitEngine {
    finished: bool,
    hasher: Sha256,
    layout: TinyVec<CommitStep>,
}

// 修改后
pub struct CommitEngine {
    finished: bool,
    hasher: TondiBlake3,
    layout: TinyVec<CommitStep>,
}
```

**更新StrictHash标签**:
```rust
impl CommitmentId for StrictHash {
    // 修改前
    const TAG: &'static str = "urn:ubideco:strict-types:value-hash#2024-02-10";
    // 修改后  
    const TAG: &'static str = "urn:tondi:strict-types:value-hash#2025-01-01";
}
```

---

### 阶段2: Bitcoin协议层适配

#### 2.1 迁移 `bp-core/consensus/src/tx.rs`

**Txid计算**:
```rust
// 修改前
pub fn txid(&self) -> Txid {
    let mut enc = Sha256::default();
    self.version.consensus_encode(&mut enc).expect("engines don't error");
    self.inputs.consensus_encode(&mut enc).expect("engines don't error");
    self.outputs.consensus_encode(&mut enc).expect("engines don't error");
    self.lock_time.consensus_encode(&mut enc).expect("engines don't error");
    let mut double = Sha256::default();
    double.input_raw(&enc.finish());
    Txid::from_byte_array(double.finish())
}

// 修改后 (Tondi使用单次BLAKE3)
pub fn txid(&self) -> Txid {
    let mut hasher = TondiBlake3::from_context("tondi.txid");
    self.version.consensus_encode(&mut hasher).expect("engines don't error");
    self.inputs.consensus_encode(&mut hasher).expect("engines don't error");
    self.outputs.consensus_encode(&mut hasher).expect("engines don't error");
    self.lock_time.consensus_encode(&mut hasher).expect("engines don't error");
    Txid::from_byte_array(hasher.finish())
}
```

#### 2.2 迁移 `bp-core/consensus/src/block.rs`

**BlockHash计算**:
```rust
// 修改后
impl BlockHeader {
    pub fn block_hash(&self) -> BlockHash {
        let mut hasher = TondiBlake3::from_context("tondi.block.hash");
        self.consensus_encode(&mut hasher).expect("engines don't error");
        BlockHash::from_byte_array(hasher.finish())
    }
}
```

#### 2.3 迁移 `bp-core/consensus/src/hashtypes.rs`

**PubkeyHash计算**:
```rust
// 修改后 (BLAKE3截断到20字节)
impl From<CompressedPk> for PubkeyHash {
    fn from(pk: CompressedPk) -> Self {
        let hash = blake3::hash(&pk.to_byte_array());
        let mut result = [0u8; 20];
        result.copy_from_slice(&hash.as_bytes()[..20]);
        Self(result.into())
    }
}
```

---

### 阶段3: 地址格式适配

#### 3.1 迁移 `bp-std/invoice/src/address.rs`

**添加Tondi网络支持**:
```rust
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
pub enum AddressNetwork {
    /// Bitcoin mainnet
    Mainnet,
    /// Bitcoin testnet and signet
    Testnet,
    /// Bitcoin regtest networks
    Regtest,
    /// Tondi mainnet
    TondiMainnet,
    /// Tondi testnet
    TondiTestnet,
    /// Tondi simnet
    TondiSimnet,
    /// Tondi devnet
    TondiDevnet,
}

impl AddressNetwork {
    pub fn bech32_hrp(self) -> &'static str {
        match self {
            AddressNetwork::Mainnet => "bc",
            AddressNetwork::Testnet => "tb",
            AddressNetwork::Regtest => "bcrt",
            AddressNetwork::TondiMainnet => "tondi",
            AddressNetwork::TondiTestnet => "tondi0",
            AddressNetwork::TondiSimnet => "tondisim",
            AddressNetwork::TondiDevnet => "tondidev",
        }
    }
    
    pub fn is_tondi(self) -> bool {
        matches!(self, 
            AddressNetwork::TondiMainnet | 
            AddressNetwork::TondiTestnet | 
            AddressNetwork::TondiSimnet | 
            AddressNetwork::TondiDevnet
        )
    }
}
```

**添加Copperoot地址类型**:
```rust
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display)]
pub enum AddressType {
    // ... 现有类型 ...
    
    /// Pay-to-Copperoot-Merkle (Tondi)
    #[display("P2CR")]
    P2cr,
    
    /// Pay-to-Copperoot-Verkle (Tondi, 预留)
    #[display("P2CRV")]
    P2crv,
}

pub enum AddressPayload {
    // ... 现有类型 ...
    
    /// P2CR payload (CopperootMerkle)
    Cr(OutputPk),
    
    /// P2CRV payload (CopperootVerkle, 预留)
    Crv(OutputPk),
}
```

---

### 阶段4: Taproot/Copperoot适配

#### 4.1 迁移 `bp-core/consensus/src/taproot.rs`

**签名哈希计算**:
```rust
/// Copperoot签名哈希上下文
pub const COPPEROOT_SIGHASH_TAG: &str = "CopperootSighash";
pub const COPPEROOT_LEAF_TAG: &str = "CopperootLeaf";
pub const COPPEROOT_NODE_TAG: &str = "CopperootNode";
pub const COPPER_TWEAK_TAG: &str = "CopperTweak";

/// 计算Copperoot签名哈希
pub fn copperoot_sighash(
    tx: &Tx,
    input_index: usize,
    prevouts: &[TxOut],
    sighash_type: CopperootSighashType,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(COPPEROOT_SIGHASH_TAG);
    
    // 编码签名哈希类型
    hasher.update(&[sighash_type.to_u8()]);
    
    // 编码交易版本
    hasher.update(&tx.version.to_le_bytes());
    
    // 编码prevouts哈希
    let prevouts_hash = hash_prevouts(prevouts);
    hasher.update(&prevouts_hash);
    
    // ... 其他字段 ...
    
    hasher.finalize().into()
}

/// Copperoot叶子哈希
pub fn copperoot_leaf_hash(version: u8, script: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(COPPEROOT_LEAF_TAG);
    hasher.update(&[version]);
    hasher.update(script);
    hasher.finalize().into()
}

/// Copperoot节点哈希
pub fn copperoot_node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(COPPEROOT_NODE_TAG);
    // 排序以确保确定性
    if left < right {
        hasher.update(left);
        hasher.update(right);
    } else {
        hasher.update(right);
        hasher.update(left);
    }
    hasher.finalize().into()
}

/// Copperoot公钥调整
pub fn copperoot_tweak(internal_key: &[u8; 32], merkle_root: Option<&[u8; 32]>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(COPPER_TWEAK_TAG);
    hasher.update(internal_key);
    if let Some(root) = merkle_root {
        hasher.update(root);
    }
    hasher.finalize().into()
}
```

---

### 阶段5: RGB协议层适配

#### 5.1 更新RGB承诺标签

**文件**: `rgb-core/src/` 相关文件

```rust
// 更新所有RGB承诺标签
impl CommitmentId for SchemaId {
    const TAG: &'static str = "urn:tondi:rgb:schema#2025-01-01";
}

impl CommitmentId for OpId {
    const TAG: &'static str = "urn:tondi:rgb:operation#2025-01-01";
}

impl CommitmentId for BundleId {
    const TAG: &'static str = "urn:tondi:rgb:bundle#2025-01-01";
}

impl CommitmentId for SecretSeal {
    const TAG: &'static str = "urn:tondi:seals:secret#2025-01-01";
}
```

#### 5.2 TLV RGB集成支持

根据Copperoot文档，RGB可以通过Annex TLV进行集成：

```rust
/// RGB TLV类型定义
pub mod rgb_tlv {
    /// RGBX_ROOT: RGB承诺根哈希
    pub const TYPE_RGBX_ROOT: u8 = 0x20;
    /// RGBX_PROOF_REF: Cartridge CID/hash引用
    pub const TYPE_RGBX_PROOF_REF: u8 = 0x21;
    /// StateEpoch: RGB状态纪元标识符
    pub const TYPE_STATE_EPOCH: u8 = 0x22;
    /// Namespace: RGB命名空间标识符
    pub const TYPE_NAMESPACE: u8 = 0x23;
    /// BatchProofRef: 批量证明数据引用
    pub const TYPE_BATCH_PROOF_REF: u8 = 0x24;
    /// DAHash: 数据可用性引用哈希
    pub const TYPE_DA_HASH: u8 = 0x25;
}

/// RGB Annex结构
pub struct RgbAnnex {
    /// RGB承诺根
    pub rgbx_root: [u8; 32],
    /// 状态纪元
    pub state_epoch: Option<u64>,
    /// 命名空间
    pub namespace: Option<Vec<u8>>,
    /// 证明引用
    pub proof_ref: Option<Vec<u8>>,
}

impl RgbAnnex {
    /// 序列化为TLV格式
    pub fn to_tlv(&self) -> Vec<u8> {
        let mut result = vec![0x50]; // Annex标记
        
        // RGBX_ROOT
        result.push(rgb_tlv::TYPE_RGBX_ROOT);
        result.push(32);
        result.extend_from_slice(&self.rgbx_root);
        
        // StateEpoch (可选)
        if let Some(epoch) = self.state_epoch {
            result.push(rgb_tlv::TYPE_STATE_EPOCH);
            result.push(8);
            result.extend_from_slice(&epoch.to_le_bytes());
        }
        
        // ... 其他字段 ...
        
        result
    }
}
```

---

### 阶段6: 特性标志和兼容性

#### 6.1 Cargo.toml特性配置

```toml
[features]
default = ["tondi"]

# Tondi L1支持 (默认)
tondi = ["blake3"]

# Bitcoin L1兼容性 (用于跨链操作)
bitcoin-compat = ["sha2"]

# 双链支持
dual-chain = ["tondi", "bitcoin-compat"]
```

#### 6.2 条件编译

```rust
#[cfg(feature = "tondi")]
use crate::digest::TondiBlake3 as DefaultHasher;

#[cfg(all(feature = "bitcoin-compat", not(feature = "tondi")))]
use sha2::Sha256 as DefaultHasher;
```

---

## 文件修改清单

### 高优先级 (核心哈希基础设施)

| 文件 | 修改类型 | 复杂度 |
|------|---------|--------|
| `Cargo.toml` (workspace) | 添加blake3依赖 | 低 |
| `client_side_validation/commit_verify/src/digest.rs` | 实现TondiBlake3 | 高 |
| `client_side_validation/commit_verify/src/merkle.rs` | 迁移到Blake3 | 中 |
| `client_side_validation/commit_verify/src/id.rs` | 更新CommitEngine | 中 |

### 中优先级 (协议层适配)

| 文件 | 修改类型 | 复杂度 |
|------|---------|--------|
| `bp-core/consensus/src/tx.rs` | Txid使用Blake3 | 中 |
| `bp-core/consensus/src/block.rs` | BlockHash使用Blake3 | 中 |
| `bp-core/consensus/src/hashtypes.rs` | 公钥哈希迁移 | 中 |
| `bp-core/consensus/src/taproot.rs` | Copperoot签名哈希 | 高 |

### 低优先级 (地址和接口)

| 文件 | 修改类型 | 复杂度 |
|------|---------|--------|
| `bp-std/invoice/src/address.rs` | 添加Tondi网络 | 中 |
| `rgb-core/src/*.rs` | 更新承诺标签 | 低 |
| `rgb-std/src/*.rs` | RGB TLV支持 | 中 |

---

## 测试策略

### 单元测试

1. **哈希一致性测试**: 验证BLAKE3输出与预期值匹配
2. **域分离测试**: 验证不同上下文产生不同哈希
3. **Merkle树测试**: 验证merklization过程正确

### 集成测试

1. **交易签名**: 验证Copperoot签名哈希计算
2. **地址编码/解码**: 验证Tondi地址格式
3. **RGB承诺**: 验证RGB状态承诺

### 兼容性测试

1. **跨链测试**: 验证Bitcoin兼容模式
2. **回归测试**: 确保不破坏现有功能

---

### 阶段7: RPC客户端迁移

#### 7.1 替换bp-esplora-client为bp-tondi-client

由于Tondi使用原生RPC API而非Esplora API，我们创建了新的`bp-tondi-client`包来替代`bp-esplora-client`。

**新增文件**:
- `bp-tondi-client/Cargo.toml`
- `bp-tondi-client/src/lib.rs`
- `bp-tondi-client/src/client.rs`
- `bp-tondi-client/src/api.rs`
- `bp-tondi-client/src/types.rs`
- `bp-tondi-client/src/error.rs`

**修改的文件**:
- `Cargo.toml` (workspace): 添加bp-tondi-client，移除bp-esplora-client
- `rgb/Cargo.toml`: 使用bp-tondi替代bp-esplora
- `rgb/cli/Cargo.toml`: 使用bp-tondi替代bp-esplora
- `rgb/src/resolvers/mod.rs`: 添加Tondi resolver
- `rgb/src/resolvers/tondi.rs`: 新增Tondi resolver实现
- `rgb/cli/src/opts.rs`: 添加--tondi命令行选项
- `rgb/cli/src/args.rs`: 使用Tondi resolver

**Tondi RPC API支持**:

| API | 功能 |
|-----|------|
| `ping` | 连接检测 |
| `get_server_info` | 获取服务器信息 |
| `get_sync_status` | 获取同步状态 |
| `submit_transaction` | 提交交易 |
| `get_transaction` | 获取交易信息 |
| `get_transaction_status` | 获取交易状态 |
| `get_block` | 获取区块信息 |
| `get_block_dag_info` | 获取DAG信息 |
| `get_balance_by_address` | 获取地址余额 |
| `get_utxos_by_address` | 获取地址UTXO |
| `get_fee_estimate` | 获取费率估算 |

**使用示例**:
```rust
use tondi::{TondiClient, TondiConfig};

#[tokio::main]
async fn main() -> Result<(), tondi::Error> {
    let config = TondiConfig::mainnet("localhost:17110");
    let client = TondiClient::connect(config).await?;
    
    // 检查连接
    client.ping().await?;
    
    // 获取同步状态
    let is_synced = client.get_sync_status().await?;
    println!("Node synced: {}", is_synced);
    
    // 获取UTXO
    let utxos = client.get_utxos_by_address("tondi1qp...").await?;
    for utxo in utxos {
        println!("UTXO: {:?} - {} saus", utxo.outpoint, utxo.utxo_entry.amount);
    }
    
    Ok(())
}
```

---

## 参考文档

- [TSP-0001: Taproot支持](../Tondi-Standard-Proposal/TSP-0001.md)
- [TSP-0002: BLAKE3哈希算法](../Tondi-Standard-Proposal/TSP-0002.md)
- [Copperoot规范](../Tondi/docs/copperoot.md)
- [Tondi白皮书](../Tondi/TONDI_WHITEPAPER.md)
- [Tondi RPC Client README](./RPC_CLIENT_README.md)

---

---

## 当前迁移状态 (2025-12-03)

### 已完成的适配

| 模块 | 状态 | 说明 |
|------|------|------|
| `commit_verify` | ✅ 完成 | TondiBlake3 替换 Sha256, 域分离哈希 |
| `bp-consensus` | ✅ 完成 | Copperoot 签名哈希, BLAKE3 Txid |
| `bp-dbc` | ✅ 完成 | DBC 使用 BLAKE3 |
| `bp-seals` | ✅ 完成 | 单用封条使用 BLAKE3, Method::Blake3t |
| `bp-invoice` | ✅ 完成 | Base58check 使用 BLAKE3 校验和 |
| `bp-derive` | ✅ 完成 | Tondi seed, coin type 7890 |
| `descriptors` | ✅ 完成 | 描述符ID使用 BLAKE3 |
| `pstt` | ✅ 完成 | MPC 使用 Blake3t |
| `aluvm` | ✅ 完成 | LibId 使用 TondiBlake3 |
| `ultrasonic` | ✅ 完成 | Codex/Contract/Operation ID 使用 BLAKE3 |
| `rgb-core` | ✅ 完成 | RGB 核心协议适配 |
| `rgb-interfaces` | ✅ 完成 | RGB 接口适配 |

### Tondi 特定参数

| 参数 | 值 | 说明 |
|------|------|------|
| Coin Type (BIP-44) | 7890 | Tondi HD 钱包派生路径 |
| HMAC Seed | "Tondi seed" | HD 密钥派生 |
| Bech32 HRP (主网) | tondi | 地址前缀 |
| Bech32 HRP (测试网) | tondi0 | 测试地址前缀 |
| Consensus 类型 | Tondi/TondiTestnet | RGB 共识层 |
| Transaction Format | PSTT | Partially Signed Tondi Transaction (替代 PSBT) |
| Magic Bytes | `pstt\xFF` | PSTT 文件魔数 |

### 待完成 (需要 Fork 外部 Crates)

以下外部 crates 需要被 fork 并更新以支持 Tondi:

| Crate | 问题 | 解决方案 |
|-------|------|----------|
| `sonic-callreq` | Consensus 枚举缺少 Tondi 变体 | Fork 并添加 Tondi 支持 |
| `sonic-api` | 使用 Sha256 和旧的 CommitmentId 约束 | Fork 并迁移到 TondiBlake3 |
| `hypersonic` | 依赖上述 crates | Fork 并更新依赖 |
| `sonic-persist-fs` | 依赖 hypersonic | Fork 并更新依赖 |

### 向后兼容

为了与未更新的外部 crates 兼容，已保留以下内容:
- `Sha256`, `Ripemd160` 在 `commit_verify` 中重新导出
- `Consensus::Bitcoin`, `Consensus::Liquid` 标记为 deprecated
- `LIB_NAME_BITCOIN` 保留但标记为 deprecated
- 旧的 Bitcoin genesis block hashes 保留但标记为 deprecated

---

## 完整 Tondi 适配清单

### ✅ 已完成

1. **核心哈希基础设施**
   - `TondiBlake3` 替代 `Sha256` 用于所有哈希操作
   - 域分离使用 BLAKE3 的 `derive_key` 函数

2. **HD 钱包参数**
   - Tondi Coin Type: `7890`
   - HMAC Seed: `"Tondi seed"`

3. **PSBT → PSTT 重命名**
   - 目录: `psbt/` → `pstt/`
   - Magic bytes: `pstt\xFF`
   - 所有类型名称更新

4. **网络类型**
   - `Network` enum 更新为 Tondi 网络 (Mainnet, Testnet, Simnet, Devnet)
   - `AddressNetwork` 使用 TondiMainnet, TondiTestnet, TondiSimnet, TondiDevnet

5. **创世区块哈希**
   - `GENESIS_TONDI_MAINNET`: 7ba62b07f55160ff0e737ada5b009bce...
   - `GENESIS_TONDI_TESTNET`: 76080d71779c5da22d2127843c7d656e...
   - `GENESIS_TONDI_SIMNET`: 507e187e838115cada6f2c6d59145b85...
   - `GENESIS_TONDI_DEVNET`: 53218a310490f0f36670baab6ef9b925...

6. **URN 标签更新**
   - `urn:ubideco:*` → `urn:tondi:*`
   - `urn:lnp-bp:*` → `urn:tondi:*`
   - strict-types: `urn:tondi:strict-types:*`
   - aluvm: `urn:tondi:aluvm:*`
   - ultrasonic: `urn:tondi:sonic:*`, `urn:tondi:ultrasonic:*`
   - mmb: `urn:tondi:mmb:*`

7. **库名称常量**
   - `LIB_NAME_TONDI = "Tondi"` (原 `LIB_NAME_BITCOIN`)

8. **外部 Crate Fork** (在 `external-forks/` 目录)
   - `sonic-callreq`: 添加 `Consensus::Tondi` 支持
   - `sonic-api`: 使用 `TondiBlake3`

9. **RGB 合约参数**
   - `CreateParams::new_tondi_mainnet()` - 主网合约创建
   - `CreateParams::new_tondi_testnet()` - 测试网合约创建

---

## 版本历史

| 版本 | 日期 | 描述 |
|------|------|------|
| 0.1.0 | 2025-01-01 | 初始迁移文档 |
| 0.2.0 | 2025-12-02 | 添加bp-tondi-client替换bp-esplora-client |
| 0.3.0 | 2025-12-03 | 完成核心库适配到 BLAKE3/Tondi, 添加 Tondi coin type 7890 |
| 0.4.0 | 2025-12-03 | PSBT 重命名为 PSTT (Partially Signed Tondi Transaction) |
| 0.5.0 | 2025-12-03 | 完成网络类型、创世区块、URN 标签全面适配 |

