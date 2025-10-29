# OAuth Implementation for bp-esplora-client

## 概述

本文档描述了为 bp-esplora-client 添加的 OAuth 2.0 客户端凭证流认证支持，用于访问 Blockstream Enterprise Esplora API。

## 实现功能

### 1. OAuth配置管理 (`OAuthConfig`)

提供了灵活的OAuth配置：

```rust
pub struct OAuthConfig {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
}
```

默认配置：
- Token URL: `https://login.blockstream.com/realms/blockstream-public/protocol/openid-connect/token`
- Scope: `openid`

### 2. Token管理 (`OAuthManager`)

线程安全的Token管理器，提供以下功能：
- **自动获取Token**: 在需要时自动从OAuth服务器获取访问令牌
- **Token缓存**: 缓存令牌避免频繁请求
- **自动刷新**: 在令牌过期前自动刷新
- **线程安全**: 使用Arc和Mutex实现线程安全

### 3. 集成到Client

为BlockingClient和AsyncClient添加了OAuth支持：

```rust
let client = Builder::new("https://enterprise.blockstream.info/testnet/api")
    .with_oauth("client_id".to_string(), "client_secret".to_string())
    .build_blocking()?;
```

## 使用示例

### Blocking Client

```rust
use bp_esplora::Builder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Builder::new("https://enterprise.blockstream.info/testnet/api")
        .with_oauth(
            std::env::var("ESPLORA_CLIENT_ID")?,
            std::env::var("ESPLORA_CLIENT_SECRET")?,
        )
        .build_blocking()?;

    // 正常使用客户端，OAuth会自动处理
    let height = client.height()?;
    println!("Current height: {}", height);
    
    Ok(())
}
```

### Async Client

```rust
use bp_esplora::Builder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Builder::new("https://enterprise.blockstream.info/testnet/api")
        .with_oauth(
            std::env::var("ESPLORA_CLIENT_ID")?,
            std::env::var("ESPLORA_CLIENT_SECRET")?,
        )
        .build_async()?;

    let height = client.height().await?;
    println!("Current height: {}", height);
    
    Ok(())
}
```

### 自定义配置

```rust
use bp_esplora::{Builder, OAuthConfig};

let oauth_config = OAuthConfig::new(
    "client_id".to_string(),
    "client_secret".to_string(),
)
.with_token_url("https://custom-auth-server.com/token".to_string())
.with_scope("openid custom_scope".to_string());

let client = Builder::new("https://api.example.com")
    .with_oauth_config(oauth_config)
    .build_blocking()?;
```

## 技术细节

### 1. Token获取流程

```
1. 检查缓存中是否有有效的token
   - 如果有且未过期，直接返回
   - 如果过期或不存在，继续步骤2

2. 发送POST请求到token URL
   - Content-Type: application/x-www-form-urlencoded
   - Body: client_id, client_secret, grant_type=client_credentials, scope

3. 解析响应
   - 提取access_token
   - 提取expires_in（如果有）
   - 计算过期时间并缓存

4. 将token添加到请求的Authorization header
   - Format: "Bearer <token>"
```

### 2. 错误处理

新的错误类型：
```rust
pub enum OAuthError {
    OAuth(String),           // OAuth认证失败
    Http(minreq::Error),    // HTTP请求错误（blocking）
    Http(reqwest::Error),   // HTTP请求错误（async）
    Deserialize(serde_json::Error),  // JSON反序列化错误
}
```

### 3. 线程安全

使用Arc + Mutex实现线程安全：
- `OAuthManager` 内部使用 `Arc<Mutex<OAuthManagerInner>>`
- 多个线程可以安全共享同一个`OAuthManager`实例
- Token缓存对多线程访问安全

## 依赖添加

添加了以下依赖：
```toml
urlencoding = "3"  # URL编码
serde_json = { workspace = true }  # JSON处理
```

## 文件修改清单

1. **新增文件**:
   - `src/oauth.rs`: OAuth实现模块
   - `examples/oauth_example.rs`: Blocking示例
   - `examples/oauth_async_example.rs`: Async示例

2. **修改文件**:
   - `src/lib.rs`: 添加oauth模块和OAuthError
   - `src/blocking.rs`: 添加OAuth支持
   - `src/async.rs`: 添加OAuth支持
   - `Cargo.toml`: 添加依赖和示例配置
   - `README.md`: 添加使用文档

## 测试

运行示例：
```bash
# Blocking示例
cargo run --example oauth_example --features blocking

# Async示例
cargo run --example oauth_async_example --features "async tokio"
```

## 后续改进

1. 支持refresh token（当前只支持access token）
2. 添加更详细的错误信息
3. 支持不同的认证流程（authorization code flow等）
4. 添加token预加载功能
5. 添加metrics/监控支持

## 总结

本次实现成功为bp-esplora-client添加了OAuth 2.0支持，使得客户端可以：
- 自动获取和刷新访问令牌
- 无缝集成到现有的blocking和async客户端
- 提供灵活的配置选项
- 确保线程安全性

所有修改都保持了向后兼容性，不影响现有功能。

## 修复记录

### 修复的问题
1. **错误类型定义冲突**: 修复了 `Http` 变体在 blocking 和 async 功能中的命名冲突，改为 `HttpBlocking` 和 `HttpAsync`
2. **Debug trait**: 为 `OAuthManager` 和 `OAuthManagerInner` 添加了 `Debug` derive
3. **minreq API**: 修复了 minreq 的 API 使用，使用 `Request::new()` 代替 `Request::post()`
4. **类型不匹配**: 修复了 HashMap 参数类型和错误处理函数签名
5. **代码清理**: 移除了未使用的导入和变量

