//! # Tondi RPC Client for RGB/BP
//!
//! This crate provides a native Tondi RPC client for the RGB protocol stack,
//! replacing the Esplora client for Tondi blockchain integration.
//!
//! Tondi RGB uses native P2MR outputs with BLAKE3 transaction IDs.
//!
//! ## Features
//!
//! - Native Tondi RPC API support
//! - UTXO queries by address
//! - Transaction submission
//! - Block and DAG information queries
//! - Paginated virtual-chain replay for wallet reorg recovery
//!
//! ## Example
//!
//! ```no_run
//! use tondi::{TondiClient, TondiConfig, DEFAULT_GRPC_PORT};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), tondi::Error> {
//!     // For gRPC connection (port 16210)
//!     let config = TondiConfig::new(format!("localhost:{}", DEFAULT_GRPC_PORT));
//!     // Or use a remote node: TondiConfig::new("38.190.226.7:16210")
//!     let client = TondiClient::connect(config).await?;
//!     
//!     // Check connection
//!     client.ping().await?;
//!     
//!     // Get sync status
//!     let is_synced = client.get_sync_status().await?;
//!     println!("Node synced: {}", is_synced);
//!     
//!     Ok(())
//! }
//! ```

#![allow(clippy::result_large_err)]

#[macro_use]
extern crate amplify;

mod api;
mod client;
mod error;
mod public;
mod types;
pub use public::{PublicDataKind, PublicDatum, PublicReference};

pub use api::*;
pub use client::*;
pub use error::*;
pub use types::*;

/// Default Tondi mainnet RPC port (Borsh RPC)
pub const DEFAULT_RPC_PORT: u16 = 17110;

/// Default Tondi testnet RPC port (Borsh RPC)
pub const DEFAULT_TESTNET_RPC_PORT: u16 = 17210;

/// Default Tondi mainnet gRPC port
pub const DEFAULT_GRPC_PORT: u16 = 16210;

/// Default Tondi testnet gRPC port
pub const DEFAULT_TESTNET_GRPC_PORT: u16 = 16220;
