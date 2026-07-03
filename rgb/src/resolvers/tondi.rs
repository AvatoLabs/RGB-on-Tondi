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

use bpstd::pstt::Utxo;
use bpstd::{Address, AddressNetwork, Sats, ScriptPubkey, Terminal, Tx, Txid, UnsignedTx, Vout};
use rgb::{Outpoint, WitnessStatus};
use tondi::{Error as TondiError, NetworkType, TondiClient, TondiConfig, TondiOutpoint};

use super::{Resolver, ResolverError};
use crate::tondi_address::encode_p2mr_for_address_network;

/// Tondi async resolver for RGB wallet operations
pub struct TondiResolver {
    endpoint: String,
    network_type: NetworkType,
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
        Ok(Self {
            endpoint: endpoint.to_owned(),
            network_type: NetworkType::Mainnet,
            network: AddressNetwork::TondiMainnet,
            min_blue_score_depth: 10, // Default: 10 confirmations
        })
    }

    /// Creates a new Tondi resolver for mainnet
    #[cfg(feature = "async")]
    pub async fn mainnet(endpoint: &str) -> Result<Self, ResolverError> {
        Ok(Self {
            endpoint: endpoint.to_owned(),
            network_type: NetworkType::Mainnet,
            network: AddressNetwork::TondiMainnet,
            min_blue_score_depth: 10,
        })
    }

    /// Creates a new Tondi resolver for testnet
    #[cfg(feature = "async")]
    pub async fn testnet(endpoint: &str) -> Result<Self, ResolverError> {
        Ok(Self {
            endpoint: endpoint.to_owned(),
            network_type: NetworkType::Testnet,
            network: AddressNetwork::TondiTestnet,
            min_blue_score_depth: 10,
        })
    }

    /// Set minimum Blue Score depth for confirmation
    pub fn with_min_blue_score_depth(mut self, depth: u64) -> Self {
        self.min_blue_score_depth = depth;
        self
    }

    /// Override the address network selected by the CLI.
    pub fn with_network(mut self, network: AddressNetwork) -> Self {
        self.network = network;
        self.network_type = match network {
            AddressNetwork::TondiMainnet => NetworkType::Mainnet,
            AddressNetwork::TondiTestnet => NetworkType::Testnet,
            AddressNetwork::TondiTestnet2 => NetworkType::Testnet2,
            AddressNetwork::TondiSimnet => NetworkType::Simnet,
            AddressNetwork::TondiDevnet => NetworkType::Devnet,
            _ => self.network_type,
        };
        self
    }

    #[cfg(feature = "async")]
    async fn rpc_client(&self) -> Result<TondiClient, ResolverError> {
        TondiClient::connect(
            TondiConfig::new(self.endpoint.clone()).with_network(self.network_type),
        )
        .await
        .map_err(|_| ResolverError::Connectivity)
    }

    /// Get the current network
    pub fn network(&self) -> AddressNetwork { self.network }

    /// Get a full Tondi-native transaction.
    ///
    /// Tondi-native RGB does not use legacy witness or Annex carriers.
    /// Commitments are bound to P2MR outputs, while proof data stays in the RGB
    /// consignment/sidecar layer.
    #[cfg(feature = "async")]
    pub async fn get_full_tx(&self, txid: Txid) -> Result<Option<Tx>, ResolverError> {
        let tondi_txid = tondi_txid_from_bp_txid(txid)?;
        let client = self.rpc_client().await?;

        match client.get_transaction(tondi_txid).await {
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
            transaction_id: tondi_txid_from_bp_txid(outpoint.txid)?,
            index: outpoint.vout.to_u32(),
        };
        let client = self.rpc_client().await?;

        let tx = match client.get_transaction(tondi_outpoint.transaction_id).await {
            Ok(tx) => tx,
            Err(TondiError::TransactionNotFound(_)) => return Ok(None),
            Err(_) => return Err(ResolverError::Connectivity),
        };

        if tondi_outpoint.index as usize >= tx.outputs.len() {
            return Ok(None);
        }

        let status = match client
            .get_transaction_status(tondi_outpoint.transaction_id)
            .await
        {
            Ok(status) => status,
            Err(TondiError::TransactionNotFound(_)) => return Ok(None),
            Err(_) => return Err(ResolverError::Connectivity),
        };
        if !status.accepted {
            return Ok(None);
        }

        let output = &tx.outputs[tondi_outpoint.index as usize];
        let (expected_value, address) = match output {
            tondi::TondiTxOutput::Standard { value, script_public_key } => {
                (*value, tondi_script_public_key_address(self.network, script_public_key)?)
            }
            _ => return Ok(None),
        };

        let live_utxo = client
            .get_utxos_by_address(&address)
            .await
            .map_err(|_| ResolverError::Connectivity)?
            .into_iter()
            .find(|entry| entry.outpoint == tondi_outpoint);
        let Some(live_utxo) = live_utxo else {
            return Ok(None);
        };
        if live_utxo.utxo_entry.amount != expected_value {
            return Err(ResolverError::Protocol);
        }

        Ok(Some(Utxo {
            outpoint,
            value: Sats::from_sats(live_utxo.utxo_entry.amount),
            // Terminal needs to be determined from descriptor context.
            terminal: Terminal::new(0u8, 0u16.into()),
        }))
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
    in_mempool: bool,
    removed: bool,
    daa_score: Option<u64>,
    has_sufficient_depth: bool,
) -> WitnessStatus {
    if removed {
        return WitnessStatus::Archived;
    }
    if !accepted {
        return if in_mempool { WitnessStatus::Tentative } else { WitnessStatus::Archived };
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
        let tondi_txid = tondi_txid_from_bp_txid(txid)?;
        let client = self.rpc_client().await?;

        match client.get_transaction(tondi_txid).await {
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
        let tondi_txid = tondi_txid_from_bp_txid(txid)?;
        let client = self.rpc_client().await?;

        let status = match client.get_transaction_status(tondi_txid).await {
            Ok(status) => {
                // Get current DAA score for Blue Score depth check
                let dag_info = client
                    .get_block_dag_info()
                    .await
                    .map_err(|_| ResolverError::Connectivity)?;
                let current_daa_score = dag_info.virtual_daa_score;

                // Check if transaction has sufficient Blue Score depth
                // This prevents micro-reorganizations in GhostDAG
                let has_sufficient_depth = self
                    .check_blue_score_depth(&status, current_daa_score)
                    .await;

                convert_status(
                    status.accepted,
                    status.in_mempool,
                    status.removed,
                    status.accepting_block_daa_score,
                    has_sufficient_depth,
                )
            }
            Err(TondiError::TransactionNotFound(_)) => WitnessStatus::Archived,
            Err(_) => return Err(ResolverError::Connectivity),
        };
        if !matches!(status, WitnessStatus::Archived) {
            return Ok(status);
        }

        self.resolve_bridge_tx_status_async(&client, txid).await
    }

    async fn resolve_utxos_async(
        &self,
        iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
        let mut utxos = Vec::new();
        let client = match self.rpc_client().await {
            Ok(client) => client,
            Err(err) => return vec![Err(err)].into_iter(),
        };

        for (terminal, spk) in iter {
            let address = match bp_script_pubkey_address(self.network, &spk) {
                Ok(addr) => addr,
                Err(err) => {
                    utxos.push(Err(err));
                    continue;
                }
            };

            match client.get_utxos_by_address(&address).await {
                Err(err) => {
                    let _ = err;
                    utxos.push(Err(ResolverError::Connectivity));
                }
                Ok(list) => {
                    utxos.extend(list.into_iter().map(|utxo_entry| {
                        let txid = bp_txid_from_tondi_txid(utxo_entry.outpoint.transaction_id)?;
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

    async fn check_utxo_async(&self, outpoint: Outpoint) -> Result<Option<Utxo>, ResolverError> {
        self.check_utxo(outpoint).await
    }

    async fn last_block_height_async(&self) -> Result<u64, ResolverError> {
        // In Tondi/Kaspa DAG, we use DAA score instead of block height
        let client = self.rpc_client().await?;
        match client.get_block_dag_info().await {
            Ok(info) => Ok(info.virtual_daa_score),
            Err(_) => Err(ResolverError::Connectivity),
        }
    }

    async fn broadcast_async(&self, tx: &Tx) -> Result<(), ResolverError> {
        // Convert BP transaction to Tondi transaction
        // Note: This is a simplified conversion - full implementation would need proper mapping
        let tondi_tx = convert_bp_to_tondi_tx(tx)?;
        let client = self.rpc_client().await?;

        match client.submit_transaction(tondi_tx, false).await {
            Ok(submitted_txid) => {
                if bp_txid_from_tondi_txid(submitted_txid)? != tx.txid() {
                    return Err(ResolverError::Protocol);
                }
                Ok(())
            }
            Err(err) => Err(err.into()),
        }
    }
}

impl TondiResolver {
    async fn resolve_bridge_tx_status_async(
        &self,
        client: &TondiClient,
        txid: Txid,
    ) -> Result<WitnessStatus, ResolverError> {
        let dag_info = client
            .get_block_dag_info()
            .await
            .map_err(|_| ResolverError::Connectivity)?;
        let replay = client
            .replay_virtual_chain_from_checkpoint(dag_info.pruning_point_hash, true)
            .await
            .map_err(|_| ResolverError::Connectivity)?;

        for accepted in replay.chain.accepted_transaction_ids {
            for accepted_txid in accepted.accepted_transaction_ids {
                let Ok(tondi_tx) = client.get_transaction(accepted_txid).await else {
                    continue;
                };
                let Ok(bp_tx) = tondi_tx.to_bp_tx() else {
                    continue;
                };
                if bp_tx.txid() != txid {
                    continue;
                }

                let accepting_block_daa_score = match accepted.accepting_block_daa_score {
                    Some(daa_score) => daa_score,
                    None => {
                        client
                            .get_block(accepted.accepting_block_hash, false)
                            .await
                            .map_err(|_| ResolverError::Connectivity)?
                            .header
                            .daa_score
                    }
                };
                let accepted_status = tondi::TxStatus {
                    accepted: true,
                    in_mempool: false,
                    removed: false,
                    accepting_block_hash: Some(accepted.accepting_block_hash),
                    accepting_block_daa_score: Some(accepting_block_daa_score),
                    block_time: None,
                };
                let has_sufficient_depth = self
                    .check_blue_score_depth(&accepted_status, replay.dag_info.virtual_daa_score)
                    .await;
                return Ok(convert_status(
                    true,
                    false,
                    false,
                    accepted_status.accepting_block_daa_score,
                    has_sufficient_depth,
                ));
            }
        }

        Ok(WitnessStatus::Archived)
    }
}

/// Convert the RGB runtime transaction bridge into Tondi-native wire format.
///
/// This function rejects legacy compatibility carriers. RGB-on-Tondi
/// must arrive here with Tondi P2MR roots in every output script and final
/// Tondi unlock data in `sig_script`; witness and payload must be empty.
fn convert_bp_to_tondi_tx(tx: &Tx) -> Result<tondi::TondiTransaction, ResolverError> {
    use tondi::{ScriptPublicKey, TondiOutpoint, TondiTransaction, TondiTxInput, TondiTxOutput};

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
                    transaction_id: tondi_txid_from_bp_txid(input.prev_output.txid)?,
                    index: input.prev_output.vout.to_u32(),
                },
                signature_script,
                sequence: tondi_wire_sequence(input.sequence.to_consensus_u32()),
                sig_op_count,
            })
        })
        .collect::<Result<Vec<_>, ResolverError>>()?;

    let outputs: Vec<TondiTxOutput> = tx
        .outputs
        .iter()
        .map(|output| {
            let script_public_key = ScriptPublicKey {
                version: output.script_pubkey_version,
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

fn tondi_wire_sequence(sequence: u32) -> u64 {
    if sequence == u32::MAX {
        u64::MAX
    } else {
        sequence as u64
    }
}

fn tondi_txid_from_bp_txid(txid: Txid) -> Result<amplify::Bytes32, ResolverError> {
    txid.to_string()
        .parse::<amplify::Bytes32>()
        .map_err(|_| ResolverError::Protocol)
}

fn bp_txid_from_tondi_txid(txid: amplify::Bytes32) -> Result<Txid, ResolverError> {
    txid.to_string()
        .parse::<Txid>()
        .map_err(|_| ResolverError::Protocol)
}

fn bp_script_pubkey_address(
    network: AddressNetwork,
    spk: &ScriptPubkey,
) -> Result<String, ResolverError> {
    if spk.as_slice().len() == 32 {
        return encode_p2mr_for_address_network(network, spk.as_slice())
            .map_err(|_| ResolverError::Protocol);
    }
    Address::with(spk, network)
        .map(|addr| addr.to_string())
        .map_err(|_| ResolverError::Protocol)
}

fn tondi_script_public_key_address(
    network: AddressNetwork,
    script_public_key: &tondi::ScriptPublicKey,
) -> Result<String, ResolverError> {
    script_public_key
        .validate_p2mr_root()
        .map_err(|_| ResolverError::Protocol)?;
    encode_p2mr_for_address_network(network, &script_public_key.script)
        .map_err(|_| ResolverError::Protocol)
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
            TondiError::RpcCall(message) | TondiError::InvalidNativeTransaction(message) => {
                ResolverError::ServerSide(message)
            }
            _ => ResolverError::Connectivity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tondi_status_mapping_distinguishes_archived_from_mempool() {
        assert_eq!(convert_status(false, true, false, None, false), WitnessStatus::Tentative);
        assert_eq!(convert_status(false, false, false, None, false), WitnessStatus::Archived);
        assert_eq!(convert_status(false, false, true, None, false), WitnessStatus::Archived);
    }

    #[test]
    fn tondi_status_mapping_requires_sufficient_depth_for_mined() {
        assert_eq!(convert_status(true, false, false, Some(42), false), WitnessStatus::Tentative);
        assert_eq!(
            convert_status(true, false, false, Some(42), true),
            WitnessStatus::Mined(NonZeroU64::new(42).unwrap())
        );
    }

    #[test]
    fn p2mr_address_helpers_match_for_bp_and_tondi_scripts() {
        let root = [0x11u8; 32];
        let bp_script = ScriptPubkey::from_checked(root.to_vec());
        let tondi_script = tondi::ScriptPublicKey {
            version: tondi::TONDI_P2MR_SCRIPT_VERSION,
            script: root.to_vec(),
        };

        let bp_address = bp_script_pubkey_address(AddressNetwork::TondiDevnet, &bp_script).unwrap();
        let tondi_address =
            tondi_script_public_key_address(AddressNetwork::TondiDevnet, &tondi_script).unwrap();

        assert_eq!(bp_address, tondi_address);
        assert!(bp_address.starts_with("tondidev:"));
    }

    #[test]
    fn p2mr_address_helpers_reject_zero_roots() {
        let zero_root = [0u8; 32];
        let bp_script = ScriptPubkey::from_checked(zero_root.to_vec());
        let tondi_script = tondi::ScriptPublicKey {
            version: tondi::TONDI_P2MR_SCRIPT_VERSION,
            script: zero_root.to_vec(),
        };

        assert!(bp_script_pubkey_address(AddressNetwork::TondiDevnet, &bp_script).is_err());
        assert!(
            tondi_script_public_key_address(AddressNetwork::TondiDevnet, &tondi_script).is_err()
        );
    }
}
