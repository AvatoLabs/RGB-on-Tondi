// RGB Wallet Library for Tondi Blockchain
//
// SPDX-License-Identifier: Apache-2.0
//
// Adapted for Tondi by AvatoLabs
// Based on original RGB wallet by Dr Maxim Orlovsky
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 LNP/BP Laboratories,
//                         Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
// Copyright (C) 2025 RGB Consortium, Switzerland.
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

//! Tondi blockchain resolver for RGB operations
//!
//! This module provides a native Tondi RPC client for resolving transactions,
//! UTXOs, and broadcasting transactions on the Tondi blockchain.

use core::num::NonZeroU64;

use amplify::ByteArray;
use bpstd::pstt::Utxo;
use bpstd::{Sats, ScriptPubkey, Terminal, Tx, Txid, UnsignedTx, Vout};
use rgb::{Outpoint, WitnessStatus};
use bpstd::{Address, AddressNetwork};
use tondi::{TondiClient, TondiConfig, Error as TondiError, NetworkType};

use super::{Resolver, ResolverError};

/// Tondi async resolver for RGB wallet operations
pub struct TondiResolver {
    client: TondiClient,
    network: AddressNetwork,
}

impl TondiResolver {
    /// Creates a new Tondi resolver with the specified endpoint.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The RPC endpoint URL (e.g., "localhost:17110")
    ///
    /// # Errors
    ///
    /// Returns an error if the client fails to connect to the Tondi node.
    #[cfg(feature = "async")]
    pub async fn new(endpoint: &str) -> Result<Self, ResolverError> {
        let config = TondiConfig::new(endpoint);
        let client = TondiClient::connect(config).await
            .map_err(|e| ResolverError::Connectivity)?;
        
        // Determine network from connection
        let network = match client.get_current_network().await {
            Ok(NetworkType::Mainnet) => AddressNetwork::TondiMainnet,
            Ok(NetworkType::Testnet) => AddressNetwork::TondiTestnet,
            Ok(_) => AddressNetwork::TondiMainnet, // Default to mainnet
            Err(_) => AddressNetwork::TondiMainnet,
        };
        
        Ok(Self { client, network })
    }

    /// Creates a new Tondi resolver for mainnet
    #[cfg(feature = "async")]
    pub async fn mainnet(endpoint: &str) -> Result<Self, ResolverError> {
        let config = TondiConfig::mainnet(endpoint);
        let client = TondiClient::connect(config).await
            .map_err(|_| ResolverError::Connectivity)?;
        Ok(Self { client, network: AddressNetwork::TondiMainnet })
    }

    /// Creates a new Tondi resolver for testnet
    #[cfg(feature = "async")]
    pub async fn testnet(endpoint: &str) -> Result<Self, ResolverError> {
        let config = TondiConfig::testnet(endpoint);
        let client = TondiClient::connect(config).await
            .map_err(|_| ResolverError::Connectivity)?;
        Ok(Self { client, network: AddressNetwork::TondiTestnet })
    }

    /// Get the current network
    pub fn network(&self) -> AddressNetwork {
        self.network
    }
}

/// Convert Tondi transaction status to RGB witness status
fn convert_status(accepted: bool, daa_score: Option<u64>) -> WitnessStatus {
    if !accepted {
        return WitnessStatus::Tentative;
    }
    if let Some(score) = daa_score {
        match NonZeroU64::new(score) {
            Some(height) => WitnessStatus::Mined(height),
            None => WitnessStatus::Genesis,
        }
    } else {
        WitnessStatus::Archived
    }
}

#[cfg(feature = "async")]
impl Resolver for TondiResolver {
    async fn resolve_tx_async(&self, txid: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
        let tondi_txid = amplify::Bytes32::from_byte_array(txid.to_byte_array());
        
        match self.client.get_transaction(tondi_txid).await {
            Ok(tondi_tx) => {
                // Convert Tondi transaction to BP transaction
                match tondi_tx.to_bp_tx() {
                    Ok(tx) => Ok(Some(UnsignedTx::with_sigs_removed(tx))),
                    Err(_) => Err(ResolverError::Protocol),
                }
            }
            Err(TondiError::TransactionNotFound(_)) => Ok(None),
            Err(_) => Err(ResolverError::Connectivity),
        }
    }

    async fn resolve_tx_status_async(&self, txid: Txid) -> Result<WitnessStatus, ResolverError> {
        let tondi_txid = amplify::Bytes32::from_byte_array(txid.to_byte_array());
        
        match self.client.get_transaction_status(tondi_txid).await {
            Ok(status) => Ok(convert_status(status.accepted, status.accepting_block_daa_score)),
            Err(_) => Err(ResolverError::Connectivity),
        }
    }

    async fn resolve_utxos_async(
        &self,
        iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
        let mut utxos = Vec::new();
        
        for (terminal, spk) in iter {
            let address = match Address::with(&spk, self.network) {
                Some(addr) => addr.to_string(),
                None => {
                    utxos.push(Err(ResolverError::Protocol));
                    continue;
                }
            };
            
            match self.client.get_utxos_by_address(&address).await {
                Err(_) => utxos.push(Err(ResolverError::Connectivity)),
                Ok(list) => {
                    utxos.extend(list.into_iter().map(|utxo_entry| {
                        let txid = Txid::from_byte_array(
                            utxo_entry.outpoint.transaction_id.into_inner()
                        );
                        Ok(Utxo {
                            outpoint: Outpoint::new(txid, Vout::from_u32(utxo_entry.outpoint.index)),
                            value: Sats::from_sats(utxo_entry.utxo_entry.amount),
                            terminal,
                        })
                    }));
                }
            }
        }
        
        utxos.into_iter()
    }

    async fn last_block_height_async(&self) -> Result<u64, ResolverError> {
        // In Tondi/Kaspa DAG, we use DAA score instead of block height
        match self.client.get_block_dag_info().await {
            Ok(info) => Ok(info.virtual_daa_score),
            Err(_) => Err(ResolverError::Connectivity),
        }
    }

    async fn broadcast_async(&self, tx: &Tx) -> Result<(), ResolverError> {
        // Convert BP transaction to Tondi transaction
        // Note: This is a simplified conversion - full implementation would need proper mapping
        let tondi_tx = convert_bp_to_tondi_tx(tx)?;
        
        match self.client.submit_transaction(tondi_tx, false).await {
            Ok(_) => Ok(()),
            Err(_) => Err(ResolverError::Connectivity),
        }
    }
}

/// Convert a BP transaction to Tondi transaction format
fn convert_bp_to_tondi_tx(tx: &Tx) -> Result<tondi::TondiTransaction, ResolverError> {
    use tondi::{TondiTransaction, TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey};
    
    let inputs: Vec<TondiTxInput> = tx.inputs.iter().map(|input| {
        TondiTxInput {
            previous_outpoint: TondiOutpoint {
                transaction_id: amplify::Bytes32::from_byte_array(
                    input.prev_output.txid.to_byte_array()
                ),
                index: input.prev_output.vout.to_u32(),
            },
            signature_script: input.sig_script.to_vec(),
            sequence: input.sequence.to_consensus_u32() as u64,
            sig_op_count: 1, // Default, actual value depends on script
        }
    }).collect();
    
    let outputs: Vec<TondiTxOutput> = tx.outputs.iter().map(|output| {
        TondiTxOutput {
            value: output.value.sats(),
            script_public_key: ScriptPublicKey {
                version: 0, // P2PK version
                script: output.script_pubkey.to_vec(),
            },
        }
    }).collect();
    
    Ok(TondiTransaction {
        version: tx.version.to_consensus_i32() as u16,
        inputs,
        outputs,
        lock_time: tx.lock_time.to_consensus_u32() as u64,
        subnetwork_id: amplify::Bytes32::default(), // Default subnetwork
        gas: 0,
        payload: vec![],
        mass: 0, // Will be calculated by node
    })
}

impl From<TondiError> for ResolverError {
    fn from(err: TondiError) -> Self {
        match err {
            TondiError::Connection(_) | TondiError::Timeout => ResolverError::Connectivity,
            TondiError::TransactionNotFound(_) | TondiError::BlockNotFound(_) => ResolverError::Logic,
            TondiError::InvalidServerResponse | TondiError::Serialization(_) => ResolverError::Protocol,
            TondiError::NodeNotSynced | TondiError::UtxoIndexNotEnabled => ResolverError::Logic,
            _ => ResolverError::Connectivity,
        }
    }
}

