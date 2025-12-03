//! Types for Tondi RPC API
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

/// Transaction input
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiTxInput {
    /// Previous outpoint
    pub previous_outpoint: TondiOutpoint,
    /// Signature script
    #[serde(with = "hex::serde")]
    pub signature_script: Vec<u8>,
    /// Sequence number
    pub sequence: u64,
    /// Signature operation count
    pub sig_op_count: u8,
}

/// Transaction output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiTxOutput {
    /// Value in saus
    pub value: u64,
    /// Script public key
    pub script_public_key: ScriptPublicKey,
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
    pub fn to_bp_tx(&self) -> Result<Transaction, confinement::Error> {
        let inputs = self.inputs.iter().map(|input| TxIn {
            prev_output: input.previous_outpoint.into(),
            sig_script: SigScript::from_checked(input.signature_script.clone()),
            sequence: SeqNo::from_consensus_u32(input.sequence as u32),
            witness: Witness::new(), // Tondi uses different witness structure
        });

        let outputs = self.outputs.iter().map(|output| TxOut {
            value: output.value.into(),
            script_pubkey: ScriptPubkey::from_checked(output.script_public_key.script.clone()),
        });

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

