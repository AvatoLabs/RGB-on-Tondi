# bp-esplora-client

Bitcoin Esplora API client library not using rust-bitcoin. Supports plaintext, TLS and Onion servers. Blocking or async.

<p>
    <a href="https://crates.io/crates/bp-esplora"><img alt="Crate Info" src="https://img.shields.io/crates/v/bp-esplora.svg"/></a>
    <a href="https://github.com/BP-WG/bp-esplora-client/blob/master/LICENSE"><img alt="MIT Licensed" src="https://img.shields.io/badge/license-MIT-blue.svg"/></a>
    <a href='https://coveralls.io/github/BP-WG/bp-esplora-client?branch=master'><img src='https://coveralls.io/repos/github/BP-WG/bp-esplora-client/badge.svg?branch=master' alt='Coverage Status' /></a>
    <a href="https://docs.rs/bp-esplora"><img alt="API Docs" src="https://img.shields.io/badge/docs.rs-bp--esplora-green"/></a>
    <a href="https://blog.rust-lang.org/2024/03/21/Rust-1.82.0.html"><img alt="Rustc Version 1.82.0+" src="https://img.shields.io/badge/rustc-1.82.0%2B-lightgrey.svg"/></a>
</p>

## Minimum Supported Rust Version (MSRV)

This library should compile with any combination of features with Rust 1.82.0.

## OAuth Authentication for Enterprise API

This library now supports OAuth 2.0 client credentials flow for Esplora Enterprise API. To use OAuth authentication:

### Blocking Client

```rust
use bp_esplora::Builder;

let client = Builder::new("https://enterprise.blockstream.info/testnet/api")
    .with_oauth(
        "your_client_id".to_string(),
        "your_client_secret".to_string(),
    )
    .build_blocking()?;

// The OAuth token will be automatically fetched and cached
// and added to the Authorization header in all requests
let height = client.height()?;
```

### Async Client

```rust
use bp_esplora::Builder;

let client = Builder::new("https://enterprise.blockstream.info/testnet/api")
    .with_oauth(
        "your_client_id".to_string(),
        "your_client_secret".to_string(),
    )
    .build_async()?;

// The OAuth token will be automatically fetched and cached
let height = client.height().await?;
```

### Custom OAuth Configuration

For custom token URLs or scopes:

```rust
use bp_esplora::{Builder, OAuthConfig};

let oauth_config = OAuthConfig::new(
    "your_client_id".to_string(),
    "your_client_secret".to_string(),
)
.with_token_url("https://your-token-url.com/".to_string())
.with_scope("openid your_custom_scope".to_string());

let client = Builder::new("https://enterprise.blockstream.info/testnet/api")
    .with_oauth_config(oauth_config)
    .build_blocking()?;
```

The OAuth manager automatically:
- Fetches access tokens using client credentials flow
- Caches tokens and reuses them until expiration
- Refreshes tokens automatically when they expire
- Adds the `Authorization: Bearer <token>` header to all requests
