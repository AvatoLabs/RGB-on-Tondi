// Wallet Library for RGB smart contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Adapted for Tondi by AvatoLabs
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 LNP/BP Laboratories,
//                         Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
// Copyright (C) 2025 RGB Consortium, Switzerland.
// Copyright (C) 2019-2025 Dr Maxim Orlovsky.
// Copyright (C) 2025 AvatoLabs.
// All rights under the above copyrights are reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

#[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async",))]
mod tondi;

use core::iter;
#[cfg(feature = "std")]
use std::process::exit;

use amplify::IoError;
use bpstd::pstt::Utxo;
use bpstd::{ScriptPubkey, Terminal, Tx, Txid, UnsignedTx};
use rgb::WitnessStatus;

#[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
pub use self::tondi::TondiResolver;

#[cfg(not(feature = "async"))]
pub trait Resolver {
    fn resolve_tx(&self, txid: Txid) -> Result<Option<UnsignedTx>, ResolverError>;
    fn resolve_tx_status(&self, txid: Txid) -> Result<WitnessStatus, ResolverError>;

    fn resolve_utxos(
        &self,
        iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>>;

    fn last_block_height(&self) -> Result<u64, ResolverError>;

    fn broadcast(&self, tx: &Tx) -> Result<(), ResolverError>;
}

#[cfg(feature = "async")]
pub trait Resolver {
    async fn resolve_tx_async(&self, txid: Txid) -> Result<Option<UnsignedTx>, ResolverError>;
    async fn resolve_tx_status_async(&self, txid: Txid) -> Result<WitnessStatus, ResolverError>;

    async fn resolve_utxos_async(
        &self,
        iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>>;

    async fn last_block_height_async(&self) -> Result<u64, ResolverError>;

    async fn broadcast_async(&self, tx: &Tx) -> Result<(), ResolverError>;
}

#[derive(Default)]
pub struct MultiResolver {
    // Tondi resolver (async-only for DAG-based blockchain)
    // Note: This field is conditionally compiled based on resolver-tondi features
    #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
    tondi: Option<TondiResolver>,
}

#[derive(Copy, Clone)]
pub struct NoResolver;

impl NoResolver {
    fn call(&self) -> ! {
        #[cfg(feature = "std")]
        {
            eprintln!("Error: no Tondi blockchain indexer specified; use the --tondi argument");
            exit(1);
        }
        #[cfg(not(feature = "std"))]
        panic!(
            "Error: no blockchain indexer, you need to use one of resolver-* features during the \
             compilation"
        );
    }
}

#[cfg(not(feature = "async"))]
impl Resolver for NoResolver {
    fn resolve_tx(&self, _txid: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
        self.call()
    }
    fn resolve_tx_status(&self, _txid: Txid) -> Result<WitnessStatus, ResolverError> {
        self.call()
    }
    fn resolve_utxos(
        &self,
        _iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
        self.call();
        #[allow(unreachable_code)]
        iter::empty()
    }
    fn last_block_height(&self) -> Result<u64, ResolverError> {
        self.call()
    }
    fn broadcast(&self, _tx: &Tx) -> Result<(), ResolverError> {
        self.call()
    }
}

#[cfg(feature = "async")]
impl Resolver for NoResolver {
    async fn resolve_tx_async(&self, _txid: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
        self.call()
    }
    async fn resolve_tx_status_async(&self, _txid: Txid) -> Result<WitnessStatus, ResolverError> {
        self.call()
    }
    async fn resolve_utxos_async(
        &self,
        _iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
        self.call();
        #[allow(unreachable_code)]
        iter::empty()
    }
    async fn last_block_height_async(&self) -> Result<u64, ResolverError> {
        self.call()
    }
    async fn broadcast_async(&self, _tx: &Tx) -> Result<(), ResolverError> {
        self.call()
    }
}

impl MultiResolver {
    /// Create a new Tondi resolver (async)
    #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
    pub async fn new_tondi(endpoint: &str) -> Result<Self, ResolverError> {
        Ok(Self {
            tondi: Some(TondiResolver::new(endpoint).await?),
            ..default!()
        })
    }

    pub fn new_absent() -> Result<Self, ResolverError> {
        Ok(Self::default())
    }
}

#[cfg(not(feature = "async"))]
impl Resolver for MultiResolver {
    fn resolve_tx(&self, txid: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
        let _ = txid;
        NoResolver.call()
    }

    fn resolve_tx_status(&self, txid: Txid) -> Result<WitnessStatus, ResolverError> {
        let _ = txid;
        NoResolver.call()
    }

    #[allow(unreachable_code)]
    fn resolve_utxos(
        &self,
        iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
        let _ = iter;
        NoResolver.call();
        vec![].into_iter()
    }

    fn last_block_height(&self) -> Result<u64, ResolverError> {
        NoResolver.call()
    }

    fn broadcast(&self, tx: &Tx) -> Result<(), ResolverError> {
        #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
        if let Some(resolver) = &self.tondi {
            // Tondi resolver is async-only, so we need to use tokio runtime
            // to run the async broadcast in a sync context
            #[cfg(feature = "tokio")]
            {
                let rt = tokio::runtime::Runtime::new().map_err(|_| ResolverError::Local)?;
                return rt.block_on(async { resolver.broadcast_async(tx).await });
            }
            #[cfg(not(feature = "tokio"))]
            {
                return Err(ResolverError::Local);
            }
        }
        NoResolver.call()
    }
}

#[cfg(feature = "async")]
impl Resolver for MultiResolver {
    async fn resolve_tx_async(&self, txid: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
        #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
        if let Some(resolver) = &self.tondi {
            return resolver.resolve_tx_async(txid).await;
        }
        let _ = txid;
        NoResolver.call()
    }

    async fn resolve_tx_status_async(&self, txid: Txid) -> Result<WitnessStatus, ResolverError> {
        #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
        if let Some(resolver) = &self.tondi {
            return resolver.resolve_tx_status_async(txid).await;
        }
        let _ = txid;
        NoResolver.call()
    }

    #[allow(unreachable_code)]
    async fn resolve_utxos_async(
        &self,
        iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
        #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
        if let Some(resolver) = &self.tondi {
            return resolver
                .resolve_utxos_async(iter)
                .await
                .collect::<Vec<_>>()
                .into_iter();
        }
        let _ = iter;
        NoResolver.call();
        vec![].into_iter()
    }

    async fn last_block_height_async(&self) -> Result<u64, ResolverError> {
        #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
        if let Some(resolver) = &self.tondi {
            return resolver.last_block_height_async().await;
        }
        NoResolver.call()
    }

    async fn broadcast_async(&self, tx: &Tx) -> Result<(), ResolverError> {
        #[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
        if let Some(resolver) = &self.tondi {
            return resolver.broadcast_async(tx).await;
        }
        let _ = tx;
        NoResolver.call()
    }
}

#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum ResolverError {
    Io(IoError),

    /// cannot connect to the indexer server.
    Connectivity,

    /// internal resolver error on the client side.
    Local,

    /// indexer uses invalid protocol.
    Protocol,

    /// invalid caller business logic.
    Logic,

    /// the indexer server has returned an error "{0}"
    ServerSide(String),
}
