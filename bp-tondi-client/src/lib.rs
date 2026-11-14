//! Native RPC4 client sharing the node's complete transaction and StateNote model.
//!
//! Supports official gRPC and Borsh wRPC transports, network verification,
//! accepted-chain transaction status, public references and native asset queries.
//! See the workspace README for the required sibling Tondi v4 source checkout.

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
