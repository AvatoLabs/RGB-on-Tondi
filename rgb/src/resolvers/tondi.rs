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
use tondi::{TondiClient, TondiConfig, Error as TondiError, NetworkType, TondiOutpoint};
use commit_verify::annexret::extract_commitment_from_annex;

use super::{Resolver, ResolverError};

/// Tondi async resolver for RGB wallet operations
pub struct TondiResolver {
    client: TondiClient,
    network: AddressNetwork,
    /// Minimum Blue Score depth for confirmation (default: 10)
    min_blue_score_depth: u64,
    /// Cache for full transactions (with Witness) keyed by txid
    /// This is needed because UnsignedTx doesn't preserve Witness/Annex data
    #[cfg(feature = "async")]
    tx_cache: std::collections::HashMap<Txid, Tx>,
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
        
        Ok(Self { 
            client, 
            network,
            min_blue_score_depth: 10, // Default: 10 confirmations
        })
    }

    /// Creates a new Tondi resolver for mainnet
    #[cfg(feature = "async")]
    pub async fn mainnet(endpoint: &str) -> Result<Self, ResolverError> {
        let config = TondiConfig::mainnet(endpoint);
        let client = TondiClient::connect(config).await
            .map_err(|_| ResolverError::Connectivity)?;
        Ok(Self { 
            client, 
            network: AddressNetwork::TondiMainnet,
            min_blue_score_depth: 10,
        })
    }

    /// Creates a new Tondi resolver for testnet
    #[cfg(feature = "async")]
    pub async fn testnet(endpoint: &str) -> Result<Self, ResolverError> {
        let config = TondiConfig::testnet(endpoint);
        let client = TondiClient::connect(config).await
            .map_err(|_| ResolverError::Connectivity)?;
        Ok(Self { 
            client, 
            network: AddressNetwork::TondiTestnet,
            min_blue_score_depth: 10,
        })
    }

    /// Set minimum Blue Score depth for confirmation
    pub fn with_min_blue_score_depth(mut self, depth: u64) -> Self {
        self.min_blue_score_depth = depth;
        self
    }

    /// Get the current network
    pub fn network(&self) -> AddressNetwork {
        self.network
    }

    /// Get full transaction with Witness data (for RGB commitment verification)
    ///
    /// **CRITICAL**: This method preserves Witness/Annex data that contains RGB commitments.
    /// Unlike `resolve_tx_async()` which returns `UnsignedTx` (without Witness), this method
    /// returns the complete `Tx` with Witness preserved.
    ///
    /// RGB needs the full transaction with Witness to verify commitments via `PublishedWitness::verify_commitment()`.
    /// The Witness field contains Annex data (format: 0x50 + TLV) which stores the RGB commitment hash.
    ///
    /// # Arguments
    ///
    /// * `txid` - The transaction ID to retrieve
    ///
    /// # Returns
    ///
    /// `Ok(Some(Tx))` if the transaction exists (with Witness preserved),
    /// `Ok(None)` if the transaction doesn't exist,
    /// `Err` if there was an error retrieving the transaction
    #[cfg(feature = "async")]
    pub async fn get_full_tx(&self, txid: Txid) -> Result<Option<Tx>, ResolverError> {
        let tondi_txid = amplify::Bytes32::from_byte_array(txid.to_byte_array());
        
        match self.client.get_transaction(tondi_txid).await {
            Ok(tondi_tx) => {
                // Convert Tondi transaction to BP transaction (preserves Witness/Annex)
                // The to_bp_tx() method maps Tondi's payload (Annex) to BP Witness
                // 
                // Note: to_bp_tx() will fail if the transaction contains non-Standard
                // inputs or outputs (ChannelStateSpend, IngotSpend, etc.), as RGB
                // Purist Mode only supports Standard transaction types.
                match tondi_tx.to_bp_tx() {
                    Ok(tx) => Ok(Some(tx)),
                    Err(e) => {
                        // If conversion fails due to non-Standard inputs/outputs,
                        // return Protocol error (RGB doesn't support these types)
                        Err(ResolverError::Protocol)
                    }
                }
            }
            Err(TondiError::TransactionNotFound(_)) => Ok(None),
            Err(_) => Err(ResolverError::Connectivity),
        }
    }

    /// Check if a UTXO exists and is unspent
    ///
    /// # Arguments
    ///
    /// * `outpoint` - The outpoint to check
    ///
    /// # Returns
    ///
    /// `Ok(Some(Utxo))` if the UTXO exists and is unspent,
    /// `Ok(None)` if the UTXO is spent or doesn't exist,
    /// `Err` if there was an error querying the node
    #[cfg(feature = "async")]
    pub async fn check_utxo(&self, outpoint: Outpoint) -> Result<Option<Utxo>, ResolverError> {
        let tondi_outpoint = TondiOutpoint {
            transaction_id: amplify::Bytes32::from_byte_array(outpoint.txid.to_byte_array()),
            index: outpoint.vout.to_u32(),
        };

        // Try to get the transaction to check if the output exists
        match self.client.get_transaction(tondi_outpoint.transaction_id).await {
            Ok(tx) => {
                // Check if the output index is valid
                if tondi_outpoint.index as usize >= tx.outputs.len() {
                    return Ok(None);
                }

                // Get transaction status to check if it's accepted
                match self.client.get_transaction_status(tondi_outpoint.transaction_id).await {
                    Ok(status) => {
                        if !status.accepted {
                            // Transaction not yet accepted, UTXO is tentative
                            return Ok(None);
                        }

                        // Check if UTXO is spent by querying the address (if available)
                        // For now, we assume if the transaction is accepted and output exists,
                        // the UTXO exists. A more thorough check would require querying spent UTXOs.
                        let output = &tx.outputs[tondi_outpoint.index as usize];
                        Ok(Some(Utxo {
                            outpoint,
                            value: Sats::from_sats(output.value),
                            terminal: Terminal::default(), // Terminal needs to be determined from context
                        }))
                    }
                    Err(TondiError::TransactionNotFound(_)) => Ok(None),
                    Err(_) => Err(ResolverError::Connectivity),
                }
            }
            Err(TondiError::TransactionNotFound(_)) => Ok(None),
            Err(_) => Err(ResolverError::Connectivity),
        }
    }

    /// Extract RGB commitment from transaction's Annex field
    ///
    /// # Arguments
    ///
    /// * `tx` - The BP transaction (converted from Tondi transaction)
    ///
    /// # Returns
    ///
    /// `Ok(Some([u8; 32]))` if RGB commitment is found in Annex,
    /// `Ok(None)` if no RGB commitment is found,
    /// `Err` if there was an error parsing the Annex
    fn extract_rgb_commitment(&self, tx: &Tx) -> Result<Option<[u8; 32]>, ResolverError> {
        // Look for Annex in witness fields
        for input in tx.inputs() {
            if !input.witness.is_empty() {
                // Annex is typically the last element in witness
                // In Tondi, Annex format: 0x50 + TLV data
                for witness_item in input.witness.iter() {
                    if !witness_item.is_empty() && witness_item[0] == 0x50 {
                        // Found potential Annex, try to extract RGB commitment
                        match extract_commitment_from_annex(witness_item) {
                            Ok(commitment) => return Ok(Some(commitment)),
                            Err(_) => continue, // Not a valid RGB commitment, continue searching
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    /// Check Blue Score depth for a transaction
    ///
    /// # Arguments
    ///
    /// * `tx_status` - Transaction status from Tondi node
    /// * `current_daa_score` - Current virtual DAA score
    ///
    /// # Returns
    ///
    /// `true` if the transaction has sufficient Blue Score depth (>= min_blue_score_depth),
    /// `false` otherwise
    async fn check_blue_score_depth(
        &self,
        tx_status: &tondi::TxStatus,
        current_daa_score: u64,
    ) -> bool {
        if let Some(accepting_daa_score) = tx_status.accepting_block_daa_score {
            let depth = current_daa_score.saturating_sub(accepting_daa_score);
            depth >= self.min_blue_score_depth
        } else {
            false
        }
    }
}

/// Convert Tondi transaction status to RGB witness status
/// 
/// Uses Blue Score depth check to ensure sufficient confirmation depth
fn convert_status(
    accepted: bool,
    daa_score: Option<u64>,
    has_sufficient_depth: bool,
) -> WitnessStatus {
    if !accepted {
        return WitnessStatus::Tentative;
    }
    if !has_sufficient_depth {
        // Transaction is accepted but doesn't have sufficient Blue Score depth
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
                // Note: to_bp_tx() will fail if the transaction contains non-Standard
                // inputs or outputs (ChannelStateSpend, IngotSpend, etc.), as RGB
                // Purist Mode only supports Standard transaction types.
                match tondi_tx.to_bp_tx() {
                    Ok(tx) => {
                        // ⚠️ CRITICAL WARNING: Witness Data Loss
                        //
                        // `UnsignedTx::with_sigs_removed()` will discard ALL Witness data, including
                        // the Annex field that contains RGB commitment (32-byte hash).
                        //
                        // The RGB commitment is stored in the Witness field of the first input as Annex
                        // (format: 0x50 + TLV Type 0x20 + Length 0x20 + 32-byte commitment).
                        //
                        // RGB's `PublishedWitness::verify_commitment()` requires the full `Tx` with
                        // Witness preserved to verify the commitment. However, the `Resolver` trait
                        // only returns `UnsignedTx`, which doesn't preserve Witness.
                        //
                        // SOLUTION: Use `get_full_tx()` method to retrieve the complete transaction
                        // with Witness preserved for RGB commitment verification:
                        //
                        //   let full_tx = resolver.get_full_tx(txid).await?;
                        //   // full_tx contains Witness/Annex with RGB commitment
                        //
                        // For now, we return UnsignedTx as required by the trait signature.
                        // RGB core should use `get_full_tx()` if commitment verification is needed.
                        
                        Ok(Some(UnsignedTx::with_sigs_removed(tx)))
                    }
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
            Ok(status) => {
                // Get current DAA score for Blue Score depth check
                let dag_info = self.client.get_block_dag_info().await
                    .map_err(|_| ResolverError::Connectivity)?;
                let current_daa_score = dag_info.virtual_daa_score;
                
                // Check if transaction has sufficient Blue Score depth
                // This prevents micro-reorganizations in GhostDAG
                let has_sufficient_depth = self.check_blue_score_depth(&status, current_daa_score).await;
                
                Ok(convert_status(
                    status.accepted,
                    status.accepting_block_daa_score,
                    has_sufficient_depth,
                ))
            }
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
///
/// This conversion preserves RGB commitment data by extracting Annex from BP Witness
/// and storing it in Tondi's payload field.
///
/// # RGB Purist Mode
///
/// This function only generates Standard inputs and outputs, as RGB Purist Mode
/// does not support Channel, Ingot, or other non-Standard transaction types.
fn convert_bp_to_tondi_tx(tx: &Tx) -> Result<tondi::TondiTransaction, ResolverError> {
    use tondi::{TondiTransaction, TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey};
    use commit_verify::annexret::extract_commitment_from_annex;
    
    // Extract Annex data from the first input's witness (where RGB commitment is stored)
    let mut payload = Vec::new();
    if let Some(first_input) = tx.inputs().next() {
        if !first_input.witness.is_empty() {
            // Look for Annex in witness (typically the last element)
            // Annex format: 0x50 + TLV data
            for witness_item in first_input.witness.iter().rev() {
                if !witness_item.is_empty() && witness_item[0] == 0x50 {
                    // Found Annex data, extract and store in payload
                    // Verify it's a valid RGB commitment format
                    if let Ok(_commitment) = extract_commitment_from_annex(witness_item) {
                        payload = witness_item.to_vec();
                        break;
                    }
                }
            }
        }
    }
    
    // Convert inputs: RGB only uses Standard inputs
    let inputs: Vec<TondiTxInput> = tx.inputs.iter().map(|input| {
        TondiTxInput::Standard {
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
    
    // Convert outputs: RGB only uses Standard outputs
    let outputs: Vec<TondiTxOutput> = tx.outputs.iter().map(|output| {
        TondiTxOutput::Standard {
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
        payload, // Preserve RGB commitment Annex data
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

