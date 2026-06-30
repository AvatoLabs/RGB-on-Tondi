//! Types for Tondi RPC API
//!
//! These types model Tondi's native RPC surface for RGB-on-Tondi.

use amplify::{ByteArray, Bytes32};
use bp::{
    LockTime, Outpoint, ScriptPubkey, SeqNo, SigScript, Tx as Transaction, TxIn, TxOut, TxVer, Txid, VarIntBytes, Vout, Witness,
};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Tondi-native P2MR script version.
pub const TONDI_P2MR_SCRIPT_VERSION: u16 = 4;

/// Tondi transaction ID (BLAKE3 hash)
pub type TondiTxid = Bytes32;

/// Tondi block hash (BLAKE3 hash)
pub type TondiBlockHash = Bytes32;

/// DAA score (Directed Acyclic Graph Adjusted score)
pub type DaaScore = u64;

/// Blue score for DAG ordering
pub type BlueScore = u64;

/// Network type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Devnet,
    Simnet,
}

impl Default for NetworkType {
    fn default() -> Self {
        NetworkType::Mainnet
    }
}

/// Transaction status in the DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxStatus {
    /// Whether the transaction is accepted (confirmed)
    pub accepted: bool,
    /// The accepting block hash if accepted
    pub accepting_block_hash: Option<TondiBlockHash>,
    /// The accepting block DAA score
    pub accepting_block_daa_score: Option<DaaScore>,
    /// Block time
    pub block_time: Option<u64>,
}

/// UTXO entry from Tondi
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiUtxoEntry {
    /// Amount in saus (smallest unit)
    pub amount: u64,
    /// Script public key
    pub script_public_key: ScriptPublicKey,
    /// Block DAA score when this UTXO was created
    pub block_daa_score: DaaScore,
    /// Whether this is a coinbase output
    pub is_coinbase: bool,
}

/// Script public key type for Tondi
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPublicKey {
    /// Script version
    pub version: u16,
    /// Script bytes
    #[serde(with = "hex::serde")]
    pub script: Vec<u8>,
}

impl ScriptPublicKey {
    /// Returns true when this script is a structurally valid Tondi P2MR root.
    pub fn is_p2mr_root(&self) -> bool {
        self.version == TONDI_P2MR_SCRIPT_VERSION && self.script.len() == 32 && self.script.iter().any(|byte| *byte != 0)
    }

    /// Validates that the script is Tondi-native P2MR.
    pub fn validate_p2mr_root(&self) -> Result<()> {
        if self.version != TONDI_P2MR_SCRIPT_VERSION {
            return Err(Error::InvalidNativeTransaction(format!(
                "RGB-on-Tondi outputs must use P2MR script version {}, got {}",
                TONDI_P2MR_SCRIPT_VERSION, self.version
            )));
        }

        let all_zero = self.script.iter().all(|byte| *byte == 0);
        if self.script.len() != 32 || all_zero {
            return Err(Error::InvalidNativeTransaction(format!(
                "invalid P2MR root: expected non-zero 32-byte script, got {} bytes (all_zero={})",
                self.script.len(),
                all_zero
            )));
        }

        Ok(())
    }
}

/// Transaction outpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiOutpoint {
    /// Transaction ID
    pub transaction_id: TondiTxid,
    /// Output index
    pub index: u32,
}

impl From<TondiOutpoint> for Outpoint {
    fn from(outpoint: TondiOutpoint) -> Self {
        // Convert TondiTxid to bp::Txid
        let txid = Txid::from_byte_array(outpoint.transaction_id.to_byte_array());
        Outpoint::new(txid, Vout::from_u32(outpoint.index))
    }
}

/// UTXO entry with address information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtxosByAddressEntry {
    /// Address (if available)
    pub address: Option<String>,
    /// Outpoint
    pub outpoint: TondiOutpoint,
    /// UTXO entry
    pub utxo_entry: TondiUtxoEntry,
}

/// Balance entry for an address
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceByAddressEntry {
    /// Address
    pub address: String,
    /// Balance in saus
    pub balance: Option<u64>,
}

/// Transaction input (enum-based, following Tondi's UTXO-Native architecture)
///
/// RGB on Tondi only supports Standard inputs. Other input types (ChannelStateSpend,
/// ChannelFundSpend, IngotSpend, IngotRef) are not supported in RGB Purist Mode.
///
/// # Serialization Format
///
/// Uses externally tagged format (default) to match Tondi node's JSON format:
/// ```json
/// {"standard": {"previousOutpoint": {...}, "signatureScript": "...", ...}}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TondiTxInput {
    /// Tondi Standard input (RGB must use this)
    #[serde(rename = "standard")]
    Standard {
        /// Previous outpoint
        previous_outpoint: TondiOutpoint,
        /// Signature script (note: no witness field in Standard input)
        #[serde(with = "hex::serde")]
        signature_script: Vec<u8>,
        /// Sequence number
        sequence: u64,
        /// Signature operation count
        sig_op_count: u8,
    },
    /// Channel state spend input (not supported in RGB Purist Mode)
    #[serde(rename = "channelStateSpend")]
    ChannelStateSpend {
        /// Previous outpoint
        previous_outpoint: TondiOutpoint,
        /// Additional channel-specific fields
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Channel fund spend input (not supported in RGB Purist Mode)
    #[serde(rename = "channelFundSpend")]
    ChannelFundSpend {
        /// Previous outpoint
        previous_outpoint: TondiOutpoint,
        /// Additional channel-specific fields
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Ingot (native asset) spend input (not supported in RGB Purist Mode)
    #[serde(rename = "ingotSpend")]
    IngotSpend {
        /// Previous outpoint
        previous_outpoint: TondiOutpoint,
        /// Additional ingot-specific fields
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Ingot reference input (read-only reference, not supported in RGB Purist Mode)
    #[serde(rename = "ingotRef")]
    IngotRef {
        /// Previous outpoint
        previous_outpoint: TondiOutpoint,
        /// Additional ingot-specific fields
        #[serde(flatten)]
        extra: serde_json::Value,
    },
}

/// Transaction output (enum-based, following Tondi's UTXO-Native architecture)
///
/// RGB on Tondi only supports Standard outputs. Other output types (ChannelFund,
/// ChannelState, Ingot) are not supported in RGB Purist Mode.
///
/// # Serialization Format
///
/// Uses externally tagged format (default) to match Tondi node's JSON format:
/// ```json
/// {"standard": {"value": 1000, "scriptPublicKey": {...}}}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TondiTxOutput {
    /// Standard output (RGB must use this)
    #[serde(rename = "standard")]
    Standard {
        /// Value in saus
        value: u64,
        /// Script public key
        script_public_key: ScriptPublicKey,
    },
    /// Channel fund output (not supported in RGB Purist Mode)
    #[serde(rename = "channelFund")]
    ChannelFund {
        /// Additional channel-specific fields
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Channel state output (not supported in RGB Purist Mode)
    #[serde(rename = "channelState")]
    ChannelState {
        /// Additional channel-specific fields
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Ingot (native asset) output (not supported in RGB Purist Mode)
    #[serde(rename = "ingot")]
    Ingot {
        /// Additional ingot-specific fields
        #[serde(flatten)]
        extra: serde_json::Value,
    },
}

/// Tondi transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiTransaction {
    /// Transaction version
    pub version: u16,
    /// Inputs
    pub inputs: Vec<TondiTxInput>,
    /// Outputs
    pub outputs: Vec<TondiTxOutput>,
    /// Lock time
    pub lock_time: u64,
    /// Subnetwork ID
    pub subnetwork_id: Bytes32,
    /// Gas
    pub gas: u64,
    /// Payload
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
    /// Transaction mass
    pub mass: u64,
}

impl TondiTransaction {
    /// Validates the Tondi-native RGB wire contract.
    ///
    /// RGB commitments must be carried by P2MR outputs. Legacy compatibility
    /// carriers such as payload-encoded Annex data are rejected.
    pub fn validate_tondi_native_rgb(&self) -> Result<()> {
        if !self.payload.is_empty() {
            return Err(Error::InvalidNativeTransaction(
                "payload/Annex commitment carriers are forbidden; RGB commitments must be embedded in P2MR outputs".to_string(),
            ));
        }

        for (index, input) in self.inputs.iter().enumerate() {
            if !matches!(input, TondiTxInput::Standard { .. }) {
                return Err(Error::InvalidNativeTransaction(format!(
                    "input {index} is not a Tondi Standard input; RGB-on-Tondi does not accept Channel/Ingot compatibility inputs"
                )));
            }
        }

        for (index, output) in self.outputs.iter().enumerate() {
            match output {
                TondiTxOutput::Standard { script_public_key, .. } => {
                    script_public_key
                        .validate_p2mr_root()
                        .map_err(|err| Error::InvalidNativeTransaction(format!("output {index} is not Tondi-native P2MR: {err}")))?;
                }
                _ => {
                    return Err(Error::InvalidNativeTransaction(format!(
                        "output {index} is not a Tondi Standard output; RGB-on-Tondi does not accept Channel/Ingot compatibility outputs"
                    )));
                }
            }
        }

        Ok(())
    }

    /// Convert to BP Transaction type
    ///
    /// This is only an in-memory bridge for the RGB runtime. The Tondi wire
    /// contract is validated first and remains Tondi-native: Standard inputs,
    /// P2MR v4 outputs, and no payload/Annex/witness commitment carrier.
    pub fn to_bp_tx(&self) -> Result<Transaction> {
        use amplify::confinement::Confined;

        self.validate_tondi_native_rgb()?;

        // Convert inputs: only Standard inputs are supported in Tondi-native mode.
        let inputs: Vec<TxIn> = self
            .inputs
            .iter()
            .map(|input| {
                let (previous_outpoint, signature_script, sequence) = match input {
                    TondiTxInput::Standard { previous_outpoint, signature_script, sequence, .. } => {
                        (previous_outpoint, signature_script, sequence)
                    }
                    _ => unreachable!("validate_tondi_native_rgb rejects non-standard inputs"),
                };

                Ok(TxIn {
                    prev_output: (*previous_outpoint).into(),
                    sig_script: SigScript::from_checked(signature_script.clone()),
                    sequence: SeqNo::from_consensus_u32(*sequence as u32),
                    witness: Witness::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Convert outputs: only Standard P2MR roots are supported.
        let outputs = self
            .outputs
            .iter()
            .map(|output| match output {
                TondiTxOutput::Standard { value, script_public_key } => {
                    script_public_key.validate_p2mr_root()?;
                    Ok(TxOut { value: (*value).into(), script_pubkey: ScriptPubkey::from_checked(script_public_key.script.clone()) })
                }
                _ => unreachable!("validate_tondi_native_rgb rejects non-standard outputs"),
            })
            .collect::<Result<Vec<TxOut>>>()?;

        let payload = VarIntBytes::try_from(Vec::new())
            .map_err(|err| Error::Serialization(format!("failed to create empty Tondi payload bridge: {err}")))?;

        Ok(Transaction {
            version: TxVer::from_consensus_u16(self.version),
            lock_time: LockTime::from_consensus_u64(self.lock_time),
            inputs: Confined::try_from_iter(inputs)
                .map_err(|err| Error::Serialization(format!("too many transaction inputs for RGB bridge: {err}")))?,
            outputs: Confined::try_from_iter(outputs)
                .map_err(|err| Error::Serialization(format!("too many transaction outputs for RGB bridge: {err}")))?,
            // Tondi-specific fields (required for Tondi consensus)
            // Tondi uses 20 bytes for subnetwork_id, but RPC may return Bytes32 (32 bytes)
            // Take only the first 20 bytes
            subnetwork_id: {
                let bytes32 = self.subnetwork_id.to_byte_array();
                let mut subnetwork_id_20 = [0u8; 20];
                subnetwork_id_20.copy_from_slice(&bytes32[0..20]);
                subnetwork_id_20
            },
            gas: self.gas,
            payload,
            mass: self.mass, // Copy mass field from Tondi transaction
        })
    }
}

/// Block information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiBlock {
    /// Block header
    pub header: TondiBlockHeader,
    /// Transactions in this block
    pub transactions: Vec<TondiTransaction>,
    /// Verbose data
    pub verbose_data: Option<BlockVerboseData>,
}

/// Block header
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiBlockHeader {
    /// Block version
    pub version: u32,
    /// Parent block hashes (DAG structure)
    pub parents: Vec<Vec<TondiBlockHash>>,
    /// Hash merkle root
    pub hash_merkle_root: Bytes32,
    /// Accepted ID merkle root
    pub accepted_id_merkle_root: Bytes32,
    /// UTXO commitment
    pub utxo_commitment: Bytes32,
    /// Timestamp
    pub timestamp: u64,
    /// Bits (difficulty target)
    pub bits: u32,
    /// Nonce
    pub nonce: u64,
    /// DAA score
    pub daa_score: DaaScore,
    /// Blue work
    pub blue_work: String,
    /// Blue score
    pub blue_score: BlueScore,
    /// Pruning point
    pub pruning_point: TondiBlockHash,
}

/// Block verbose data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockVerboseData {
    /// Block hash
    pub hash: TondiBlockHash,
    /// Difficulty
    pub difficulty: f64,
    /// Selected parent hash
    pub selected_parent_hash: TondiBlockHash,
    /// Transaction IDs
    pub transaction_ids: Vec<TondiTxid>,
    /// Is this block in the chain of the virtual selected parent?
    pub is_chain_block: bool,
    /// Merge set blues hashes
    pub merge_set_blues_hashes: Vec<TondiBlockHash>,
    /// Merge set reds hashes
    pub merge_set_reds_hashes: Vec<TondiBlockHash>,
    /// Children hashes
    pub children_hashes: Vec<TondiBlockHash>,
}

/// DAG info response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DagInfo {
    /// Network name
    pub network_name: String,
    /// Current block count
    pub block_count: u64,
    /// Current header count
    pub header_count: u64,
    /// Tip hashes
    pub tip_hashes: Vec<TondiBlockHash>,
    /// Difficulty
    pub difficulty: f64,
    /// Past median time
    pub past_median_time: u64,
    /// Virtual parent hashes
    pub virtual_parent_hashes: Vec<TondiBlockHash>,
    /// Pruning point hash
    pub pruning_point_hash: TondiBlockHash,
    /// Virtual DAA score
    pub virtual_daa_score: DaaScore,
    /// Sink (virtual selected parent)
    pub sink: TondiBlockHash,
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::GetBlockDagInfoResponseMessage> for DagInfo {
    fn from(info: crate::client::proto::GetBlockDagInfoResponseMessage) -> Self {
        DagInfo {
            network_name: info.network_name,
            block_count: info.block_count,
            header_count: info.header_count,
            tip_hashes: info
                .tip_hashes
                .iter()
                .map(|hash_str| {
                    hash_str
                        .trim_start_matches("0x")
                        .parse::<TondiBlockHash>()
                        .unwrap_or_else(|_| TondiBlockHash::from_byte_array([0u8; 32]))
                })
                .collect(),
            difficulty: info.difficulty,
            past_median_time: info.past_median_time as u64,
            virtual_parent_hashes: info
                .virtual_parent_hashes
                .iter()
                .map(|hash_str| {
                    hash_str
                        .trim_start_matches("0x")
                        .parse::<TondiBlockHash>()
                        .unwrap_or_else(|_| TondiBlockHash::from_byte_array([0u8; 32]))
                })
                .collect(),
            pruning_point_hash: info
                .pruning_point_hash
                .trim_start_matches("0x")
                .parse::<TondiBlockHash>()
                .unwrap_or_else(|_| TondiBlockHash::from_byte_array([0u8; 32])),
            virtual_daa_score: info.virtual_daa_score,
            sink: info
                .sink
                .trim_start_matches("0x")
                .parse::<TondiBlockHash>()
                .unwrap_or_else(|_| TondiBlockHash::from_byte_array([0u8; 32])),
        }
    }
}

/// Server info response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// RPC API version
    pub rpc_api_version: u32,
    /// RPC API revision
    pub rpc_api_revision: u32,
    /// Server version
    pub server_version: String,
    /// Network ID
    pub network_id: String,
    /// Whether the node has a UTXO index
    pub has_utxo_index: bool,
    /// Whether the node is synced
    pub is_synced: bool,
    /// Virtual DAA score
    pub virtual_daa_score: DaaScore,
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::GetInfoResponseMessage> for ServerInfo {
    fn from(info: crate::client::proto::GetInfoResponseMessage) -> Self {
        ServerInfo {
            rpc_api_version: 0,
            rpc_api_revision: 0,
            server_version: info.server_version,
            network_id: String::new(),
            has_utxo_index: info.is_utxo_indexed,
            is_synced: info.is_synced,
            virtual_daa_score: 0,
        }
    }
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::GetServerInfoResponseMessage> for ServerInfo {
    fn from(info: crate::client::proto::GetServerInfoResponseMessage) -> Self {
        ServerInfo {
            rpc_api_version: info.rpc_api_version as u32,
            rpc_api_revision: info.rpc_api_revision as u32,
            server_version: info.server_version,
            network_id: info.network_id,
            has_utxo_index: info.has_utxo_index,
            is_synced: info.is_synced,
            virtual_daa_score: info.virtual_daa_score,
        }
    }
}

/// Fee estimate
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeEstimate {
    /// Priority bucket (lowest fee for high priority)
    pub priority_bucket: FeeBucket,
    /// Normal buckets
    pub normal_buckets: Vec<FeeBucket>,
    /// Low buckets
    pub low_buckets: Vec<FeeBucket>,
}

/// Fee bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeBucket {
    /// Fee rate in saus per gram
    pub feerate: f64,
    /// Estimated seconds until confirmation
    pub estimated_seconds: f64,
}

/// Mempool entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MempoolEntry {
    /// Fee
    pub fee: u64,
    /// Transaction
    pub transaction: TondiTransaction,
    /// Is orphan
    pub is_orphan: bool,
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::RpcUtxosByAddressesEntry> for UtxosByAddressEntry {
    fn from(entry: crate::client::proto::RpcUtxosByAddressesEntry) -> Self {
        let outpoint = entry.outpoint.as_ref();
        UtxosByAddressEntry {
            address: if entry.address.is_empty() { None } else { Some(entry.address) },
            outpoint: TondiOutpoint {
                transaction_id: outpoint
                    .and_then(|op| op.transaction_id.trim_start_matches("0x").parse::<TondiBlockHash>().ok())
                    .unwrap_or_else(|| TondiBlockHash::from_byte_array([0u8; 32])),
                index: outpoint.map(|op| op.index).unwrap_or(0),
            },
            utxo_entry: entry.utxo_entry.map(TondiUtxoEntry::from).unwrap_or_else(|| TondiUtxoEntry {
                amount: 0,
                script_public_key: ScriptPublicKey { version: 0, script: Vec::new() },
                block_daa_score: 0,
                is_coinbase: false,
            }),
        }
    }
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::RpcUtxoEntry> for TondiUtxoEntry {
    fn from(entry: crate::client::proto::RpcUtxoEntry) -> Self {
        TondiUtxoEntry {
            amount: entry.amount,
            script_public_key: entry
                .script_public_key
                .map(|spk| ScriptPublicKey {
                    version: spk.version as u16,
                    script: hex::decode(spk.script_public_key.trim_start_matches("0x")).unwrap_or_else(|_| Vec::new()),
                })
                .unwrap_or_else(|| ScriptPublicKey { version: 0, script: Vec::new() }),
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
        }
    }
}
