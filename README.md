# RGB-X Workspace

This is a unified Cargo workspace containing all components of RGB-X and its related projects. All dependencies are fully privatized, using local paths instead of fetching from crates.io.

这是一个统一的 Cargo 工作区，包含了 RGB-X 及其相关项目的所有组件。所有依赖都已完全私有化，使用本地路径而不是从 crates.io 获取。

## 项目结构

### 核心编码库
- **strict-encoding** - 严格编码库：确定性和受限的二进制序列化
- **strict-types** - 严格类型：受限的广义代数数据类型（GADT）

### AluVM 和零知识
- **aluvm** - 函数式基于寄存器的 RISC 虚拟机
- **zk-aluvm** - AluVM 的伽罗瓦域扩展
- **ultrasonic** - 具有基于能力的内存访问的事务执行层

### 客户端验证
- **client_side_validation** - 客户端验证基础库
  - `commit_verify` - 提交-验证 API
  - `single_use_seals` - 单次使用密封
  - `commit_verify/derive` - 提交编码派生宏

### 比特币协议
- **bp-core** - 比特币协议核心库
  - `consensus` - 比特币协议共识库
  - `dbc` - 确定性比特币承诺库
  - `seals` - 比特币协议单次使用密封库

- **bp-std** - 比特币标准实现
  - `invoice` - 比特币地址和发票
  - `derive` - 地址和发票派生
  - `descriptors` - 比特币描述符库
  - `psbt` - 现代的 PSBT 库

- **bp-esplora-client** - 比特币 Esplora API 客户端库

### RGB 智能合约
- **rgb-core** - RGB 核心库：比特币和闪电网络上的机密和可扩展智能合约（共识层）

- **rgb-std** - RGB 智能合约标准库
  - `cli` - RGB 命令行工具箱
  - `invoice` - RGB 智能合约的发票库
  - `persistence/fs` - RGB 智能合约的文件系统持久化

- **rgb** - RGB 智能合约钱包运行时
  - `psbt` - RGB 协议的部分签名交易支持
  - `descriptors` - RGB 协议的比特币钱包描述符
  - `cli` - RGB 智能合约命令行钱包

### SONARE（当前已禁用）
- **sonare** - 形式化可验证分布式软件的运行时环境
  - 注意：由于 `sonic` 依赖问题，该项目当前已从工作区中禁用

## 构建

```bash
# 检查整个工作区
cargo check --workspace

# 构建整个工作区
cargo build --workspace

# 运行测试
cargo test --workspace

# 构建特定的包
cargo build -p rgb-std
cargo build -p bp-core
```

## 特性

### 完全私有化
所有内部依赖都使用本地路径引用，确保：
- 不依赖外部 crates.io
- 所有组件版本一致
- 完全可离线工作
- 更好的构建性能

### 统一管理
- 单一的根级 `Cargo.toml` 管理所有工作区成员
- 共享的依赖版本定义
- 统一的构建配置
- 单一的 `Cargo.lock` 文件

### 优化配置
```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
overflow-checks = true
```

## 版本信息

- Rust 版本要求: 1.85.0
- Edition: 2021
- License: Apache-2.0

## 注意事项

1. 所有子项目的 `Cargo.lock` 文件已被删除，只保留根目录的 `Cargo.lock`
2. `sonare` 项目由于 `sonic` 依赖的问题暂时被禁用
3. 一些项目可能会显示关于 `default-features` 的警告，这是正常的，不影响功能

## Maintainers / 维护者

- Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
- Avato Labs

## Links / 相关链接

- RGB-X Repository: https://github.com/AvatoLabs/RGB-X
- RGB Official: https://rgb.tech
- LNP-BP Project: https://github.com/LNP-BP
- BP-WG Project: https://github.com/BP-WG
- RGB-WG Project: https://github.com/RGB-WG

