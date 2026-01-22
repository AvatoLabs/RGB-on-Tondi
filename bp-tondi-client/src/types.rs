git fetch//! Types for Tondi RPC API
//!
//! These types provide compatibility between Tondi's native RPC types
//! and the BP/RGB stack types.

use amplify::confinement::Confined;
use amplify::{confinement, Bytes32, ByteArray};
use bp::{
    LockTime, Outpoint, ScriptPubkey, SeqNo, SigScript, Tx as Transaction, TxIn, TxOut, TxVer,
    Txid, Vout, Witness,
};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TondiTxInput {
    /// Standard Bitcoin-style input (RGB must use this)
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
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
    /// Convert to BP Transaction type
    ///
    /// This conversion preserves RGB commitment data stored in Tondi's payload field
    /// (which contains Annex data) by mapping it to BP Witness structure.
    /// 
    /// According to AnnexRet protocol, Annex data is stored in Tondi's payload field
    /// and must be mapped to the Witness field of the first input in BP format.
    ///
    /// # RGB Purist Mode
    ///
    /// Only Standard inputs and outputs are supported. Non-Standard inputs/outputs
    /// (ChannelStateSpend, ChannelFundSpend, IngotSpend, IngotRef, ChannelFund, ChannelState, Ingot)
    /// will cause this conversion to fail, as RGB Purist Mode does not support these types.
    pub fn to_bp_tx(&self) -> Result<Transaction, confinement::Error> {
        use amplify::confinement::Confined;
        
        // Extract Annex data from payload if present
        // Tondi stores Annex in payload field, format: 0x50 + TLV data (RGB commitment)
        let has_annex = !self.payload.is_empty() && self.payload[0] == 0x50;
        
        // Convert inputs: only Standard inputs are supported in RGB Purist Mode
        let inputs: Vec<TxIn> = self.inputs.iter().enumerate().map(|(idx, input)| {
            // Extract Standard input fields
            let (previous_outpoint, signature_script, sequence) = match input {
                TondiTxInput::Standard {
                    previous_outpoint,
                    signature_script,
                    sequence,
                    ..
                } => (previous_outpoint, signature_script, sequence),
                // RGB Purist Mode does not support non-Standard inputs
                _ => {
                    return Err(confinement::Error::InvalidValue(
                        format!("RGB on Tondi only supports Standard inputs, found: {:?}", input)
                    ));
                }
            };
            
            let mut witness = Witness::new();
            
            // Put Annex data in the witness of the first input
            // This follows Bitcoin's Annex convention (last element in witness stack)
            if idx == 0 && has_annex {
                // Create witness stack with Annex as the last element
                let mut witness_items = Vec::new();
                witness_items.push(Vec::new()); // Placeholder for signature
                witness_items.push(self.payload.clone()); // Annex data
                witness = Witness::from_consensus_stack(witness_items);
            }
            
            Ok(TxIn {
                prev_output: (*previous_outpoint).into(),
                sig_script: SigScript::from_checked(signature_script.clone()),
                sequence: SeqNo::from_consensus_u32(*sequence as u32),
                witness,
            })
        }).collect::<Result<Vec<_>, _>>()?;

        // Convert outputs: only Standard outputs are supported in RGB Purist Mode
        let outputs = self.outputs.iter().map(|output| {
            match output {
                TondiTxOutput::Standard { value, script_public_key } => {
                    Ok(TxOut {
                        value: (*value).into(),
                        script_pubkey: ScriptPubkey::from_checked(script_public_key.script.clone()),
                    })
                }
                // RGB Purist Mode does not support non-Standard outputs
                _ => Err(confinement::Error::InvalidValue(
                    format!("RGB on Tondi only supports Standard outputs, found: {:?}", output)
                )),
            }
        }).collect::<Result<Vec<_>, _>>()?;

        Ok(Transaction {
            version: TxVer::from_consensus_i32(self.version as i32),
            lock_time: LockTime::from_consensus_u32(self.lock_time as u32),
            inputs: Confined::try_from_iter(inputs)?,
            outputs: Confined::try_from_iter(outputs)?,
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

