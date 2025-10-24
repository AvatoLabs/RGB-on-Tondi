# Hard Fork 声明

**日期**: 2025-10-24  
**项目**: RGB-X

---

## 🔒 完全私有化的模块

以下模块已从上游仓库 hard fork，完全集成到 RGB-X，不再与上游同步：

### rgb-interfaces (v0.12.0)

- **原始仓库**: https://github.com/RGB-WG/rgb-interfaces
- **Fork 版本**: v0.12.0
- **Fork 日期**: 2025-10-24
- **Git 历史**: 已移除 (`.git` 已删除)
- **状态**: 完全私有，作为 RGB-X workspace 成员

**原因**:
- 实现完全自包含的开发环境
- 不需要与上游同步
- 作为 RGB-X 的核心组件统一维护

**包含内容**:
- RGB20 标准实现 (可替代代币)
- RGB21 标准实现 (NFT)
- RGB25 标准实现
- 核心类型: `Amount`, `Precision`, `Ticker`, `AssetName`

---

## 📝 维护政策

### RGB-X 完全控制

所有 hard fork 的代码现在由 RGB-X/Avato Labs 完全控制和维护：

- ✅ 可以自由修改
- ✅ 可以添加自定义功能
- ✅ 不需要考虑上游兼容性
- ✅ 统一版本管理

### 与上游的关系

- ❌ 不再同步上游更新
- ❌ 不保持 git 历史连接
- ℹ️ 保留原始许可证 (Apache-2.0)
- ℹ️ 保留原始作者版权声明

---

## 🎯 技术影响

### 优点

1. **完全自包含**
   - 所有依赖都在 workspace 内
   - 不依赖外部 crates.io
   - 完全离线开发

2. **统一版本管理**
   - workspace 统一控制版本
   - 不会出现版本冲突
   - 简化依赖管理

3. **自由定制**
   - 可以根据需求修改
   - 可以优化性能
   - 可以添加专有功能

### 注意事项

1. **手动跟踪上游**
   - 如需要上游新功能，需手动迁移
   - 安全更新需手动应用
   - 需要关注上游发布

2. **许可证合规**
   - 保留原始 Apache-2.0 许可证
   - 保留版权声明
   - 声明修改内容

---

## 📊 Hard Fork 清单

| 模块 | 原始仓库 | 版本 | Fork日期 | Git历史 |
|-----|---------|------|---------|---------|
| `rgb-interfaces` | RGB-WG/rgb-interfaces | v0.12.0 | 2025-10-24 | ❌ 已移除 |

---

## 🔄 未来可能的 Hard Fork

以下模块目前仍使用外部依赖，未来可能考虑 hard fork：

- ~~`rgb-schemata`~~ - Schema 工具 (可选)
- ~~其他 RGB-WG 组件~~ (按需)

**当前状态**: rgb-interfaces 是唯一的 hard fork

---

## ✅ 验证

### 检查 rgb-interfaces 状态

```bash
# 1. 确认没有 .git 目录
ls -la rgb-interfaces/.git
# 应该输出: No such file or directory

# 2. 确认在 workspace 中
grep "rgb-interfaces" Cargo.toml

# 3. 确认本地依赖
cargo tree -p rgb-interfaces --depth 1
# 所有依赖应该显示 (本地路径)

# 4. 确认 git 状态
git status rgb-interfaces/
# 应该显示为 RGB-X 的一部分
```

---

## 📚 参考

- **原始许可证**: Apache-2.0
- **原始作者**: RGB Working Group
- **维护者**: Avato Labs
- **相关文档**: 
  - `QUICK_START.md` - 快速开始
  - `rgb-interfaces/README.md` - 模块文档
  - `Cargo.toml` - Workspace 配置

---

**Hard Fork 执行**: 2025-10-24  
**维护者**: Avato Labs  
**状态**: ✅ 完成

