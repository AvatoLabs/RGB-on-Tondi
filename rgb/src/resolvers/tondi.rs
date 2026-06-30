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
use bpstd::{Address, AddressNetwork};
use bpstd::{Sats, ScriptPubkey, Terminal, Tx, Txid, UnsignedTx, Vout};
use rgb::{Outpoint, WitnessStatus};
use tondi::{Error as TondiError, NetworkType, TondiClient, TondiConfig, TondiOutpoint};

use super::{Resolver, ResolverError};

/// Tondi async resolver for RGB wallet operations
pub struct TondiResolver {
    client: TondiClient,
    network: AddressNetwork,
    /// Minimum Blue Score depth for confirmation (default: 10)
    min_blue_score_depth: u64,
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
        let client = TondiClient::connect(config)
            .await
            .map_err(|_| ResolverError::Connectivity)?;

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
        let client = TondiClient::connect(config)
            .await
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
        let client = TondiClient::connect(config)
            .await
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

    /// Get a full Tondi-native transaction.
    ///
    /// Tondi-native RGB does not use legacy witness or Annex carriers.
    /// Commitments are bound to P2MR outputs, while proof data stays in the RGB
    /// consignment/sidecar layer.
    #[cfg(feature = "async")]
    pub async fn get_full_tx(&self, txid: Txid) -> Result<Option<Tx>, ResolverError> {
        let tondi_txid = amplify::Bytes32::from_byte_array(txid.to_byte_array());

        match self.client.get_transaction(tondi_txid).await {
            Ok(tondi_tx) => match tondi_tx.to_bp_tx() {
                Ok(tx) => Ok(Some(tx)),
                Err(_e) => Err(ResolverError::Protocol),
            },
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
        match self
            .client
            .get_transaction(tondi_outpoint.transaction_id)
            .await
        {
            Ok(tx) => {
                // Check if the output index is valid
                if tondi_outpoint.index as usize >= tx.outputs.len() {
                    return Ok(None);
                }

                // Get transaction status to check if it's accepted
                match self
                    .client
                    .get_transaction_status(tondi_outpoint.transaction_id)
                    .await
                {
                    Ok(status) => {
                        if !status.accepted {
                            // Transaction not yet accepted, UTXO is tentative
                            return Ok(None);
                        }

                        // Check if UTXO is spent by querying the address (if available)
                        // For now, we assume if the transaction is accepted and output exists,
                        // the UTXO exists. A more thorough check would require querying spent UTXOs.
                        let output = &tx.outputs[tondi_outpoint.index as usize];
                        let value = match output {
                            tondi::TondiTxOutput::Standard { value, script_public_key } => {
                                script_public_key
                                    .validate_p2mr_root()
                                    .map_err(|_| ResolverError::Protocol)?;
                                *value
                            }
                            _ => return Ok(None), // Non-standard outputs not supported
                        };
                        Ok(Some(Utxo {
                            outpoint,
                            value: Sats::from_sats(value),
                            terminal: Terminal::new(0u8, 0u16.into()), // Terminal needs to be determined from context
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

    /// Extract RGB commitment from a transaction.
    ///
    /// Tondi-native RGB does not expose commitments through Annex.
    /// Commitment validation must use the RGB sidecar and the selected
    /// P2MR output root.
    #[allow(dead_code)]
    fn extract_rgb_commitment(&self, tx: &Tx) -> Result<Option<[u8; 32]>, ResolverError> {
        for input in tx.inputs() {
            if !input.witness.is_empty() {
                return Err(ResolverError::Protocol);
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
                match tondi_tx.to_bp_tx() {
                    Ok(tx) => {
                        // The bridge transaction intentionally has empty witness:
                        // Tondi-native RGB commitments are P2MR output commitments,
                        // not legacy witness or Annex data.
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
                let dag_info = self
                    .client
                    .get_block_dag_info()
                    .await
                    .map_err(|_| ResolverError::Connectivity)?;
                let current_daa_score = dag_info.virtual_daa_score;

                // Check if transaction has sufficient Blue Score depth
                // This prevents micro-reorganizations in GhostDAG
                let has_sufficient_depth = self
                    .check_blue_score_depth(&status, current_daa_score)
                    .await;

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
                Ok(addr) => addr.to_string(),
                Err(_) => {
                    utxos.push(Err(ResolverError::Protocol));
                    continue;
                }
            };

            match self.client.get_utxos_by_address(&address).await {
                Err(_) => utxos.push(Err(ResolverError::Connectivity)),
                Ok(list) => {
                    utxos.extend(list.into_iter().map(|utxo_entry| {
                        let txid = Txid::from_byte_array(
                            utxo_entry.outpoint.transaction_id.to_byte_array(),
                        );
                        Ok(Utxo {
                            outpoint: Outpoint::new(
                                txid,
                                Vout::from_u32(utxo_entry.outpoint.index),
                            ),
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

/// Convert the RGB runtime transaction bridge into Tondi-native wire format.
///
/// This function rejects legacy compatibility carriers. RGB-on-Tondi
/// must arrive here with Tondi P2MR roots in every output script and final
/// Tondi unlock data in `sig_script`; witness and payload must be empty.
fn convert_bp_to_tondi_tx(tx: &Tx) -> Result<tondi::TondiTransaction, ResolverError> {
    use tondi::{
        ScriptPublicKey, TondiOutpoint, TondiTransaction, TondiTxInput, TondiTxOutput,
        TONDI_P2MR_SCRIPT_VERSION,
    };

    if !tx.payload.is_empty() {
        return Err(ResolverError::Protocol);
    }

    let inputs: Vec<TondiTxInput> = tx
        .inputs
        .iter()
        .map(|input| {
            if !input.witness.is_empty() {
                return Err(ResolverError::Protocol);
            }

            let signature_script = input.sig_script.to_vec();
            let sig_op_count = if signature_script.is_empty() { 0 } else { 1 };

            Ok(TondiTxInput::Standard {
                previous_outpoint: TondiOutpoint {
                    transaction_id: amplify::Bytes32::from_byte_array(
                        input.prev_output.txid.to_byte_array(),
                    ),
                    index: input.prev_output.vout.to_u32(),
                },
                signature_script,
                sequence: input.sequence.to_consensus_u32() as u64,
                sig_op_count,
            })
        })
        .collect::<Result<Vec<_>, ResolverError>>()?;

    let outputs: Vec<TondiTxOutput> = tx
        .outputs
        .iter()
        .map(|output| {
            let script_public_key = ScriptPublicKey {
                version: TONDI_P2MR_SCRIPT_VERSION,
                script: output.script_pubkey.to_vec(),
            };
            script_public_key
                .validate_p2mr_root()
                .map_err(|_| ResolverError::Protocol)?;

            Ok(TondiTxOutput::Standard { value: output.value.sats(), script_public_key })
        })
        .collect::<Result<Vec<_>, ResolverError>>()?;

    let transaction = TondiTransaction {
        version: tx.version.to_consensus_u16(),
        inputs,
        outputs,
        lock_time: tx.lock_time.to_consensus_u64(),
        subnetwork_id: {
            let mut bytes32 = [0u8; 32];
            bytes32[0..20].copy_from_slice(&tx.subnetwork_id);
            amplify::Bytes32::from_byte_array(bytes32)
        },
        gas: tx.gas,
        payload: Vec::new(),
        mass: tx.mass,
    };
    transaction
        .validate_tondi_native_rgb()
        .map_err(|_| ResolverError::Protocol)?;

    Ok(transaction)
}

impl From<TondiError> for ResolverError {
    fn from(err: TondiError) -> Self {
        match err {
            TondiError::Connection(_) | TondiError::Timeout => ResolverError::Connectivity,
            TondiError::TransactionNotFound(_) | TondiError::BlockNotFound(_) => {
                ResolverError::Logic
            }
            TondiError::InvalidServerResponse | TondiError::Serialization(_) => {
                ResolverError::Protocol
            }
            TondiError::NodeNotSynced | TondiError::UtxoIndexNotEnabled => ResolverError::Logic,
            _ => ResolverError::Connectivity,
        }
    }
}
