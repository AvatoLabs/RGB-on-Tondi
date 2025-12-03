//! # Tondi RPC Client for RGB/BP
//!
//! This crate provides a native Tondi RPC client for the RGB protocol stack,
//! replacing the Esplora client for Tondi blockchain integration.
//!
//! Tondi is a Kaspa fork with Taproot support and BLAKE3 hashing.
//!
//! ## Features
//!
//! - Native Tondi RPC API support
//! - UTXO queries by address
//! - Transaction submission
//! - Block and DAG information queries
//! - Real-time notifications via WebSocket
//!
//! ## Example
//!
//! ```no_run
//! use tondi::{TondiClient, TondiConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), tondi::Error> {
//!     let config = TondiConfig::new("localhost:17110");
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
mod types;

pub use api::*;
pub use client::*;
pub use error::*;
pub use types::*;

/// Default Tondi mainnet RPC port
pub const DEFAULT_RPC_PORT: u16 = 17110;

/// Default Tondi testnet RPC port
pub const DEFAULT_TESTNET_RPC_PORT: u16 = 17210;

