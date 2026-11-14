//! Types for Tondi RPC API
//!
//! These types model Tondi's native RPC surface for RGB-on-Tondi.

use std::str::FromStr;

use amplify::Bytes32;
use bp::{
    LockTime, Outpoint, ScriptPubkey, SeqNo, SigScript, Tx as Transaction, TxIn, TxOut, TxVer,
    Txid, VarIntBytes, Vout, Witness,
};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Tondi-native P2MR script version.
pub use bp::TONDI_P2MR_SCRIPT_VERSION;

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
    /// Whether the transaction is currently known in the mempool or orphan pool.
    pub in_mempool: bool,
    /// Whether the node knows the transaction but it is no longer accepted by the selected chain.
    pub removed: bool,
    /// The accepting block hash if accepted
    pub accepting_block_hash: Option<TondiBlockHash>,
    /// The accepting block DAA score
    pub accepting_block_daa_score: Option<DaaScore>,
    /// Block time
    pub block_time: Option<u64>,
}

/// Accepted transaction ids grouped by their accepting chain block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedTransactionIds {
    /// The chain block which accepted these transactions.
    pub accepting_block_hash: TondiBlockHash,
    /// DAA score of the accepting chain block, when provided by the node.
    pub accepting_block_daa_score: Option<DaaScore>,
    /// Transactions accepted by this chain block.
    pub accepted_transaction_ids: Vec<TondiTxid>,
}

/// Virtual selected-chain delta from a known block to the current virtual chain.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualChain {
    /// Chain blocks removed by the virtual-chain transition, high-to-low.
    pub removed_chain_block_hashes: Vec<TondiBlockHash>,
    /// Chain blocks added by the virtual-chain transition, low-to-high.
    pub added_chain_block_hashes: Vec<TondiBlockHash>,
    /// Accepted transaction ids for the added chain blocks, when requested.
    pub accepted_transaction_ids: Vec<AcceptedTransactionIds>,
}

/// Source used for a virtual-chain replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualChainReplayMode {
    /// Replay started from the caller's persisted checkpoint.
    Checkpoint,
    /// The checkpoint was not retained, so replay restarted from the node's pruning point.
    PruningPointFallback,
}

/// Paginated virtual-chain replay result suitable for wallet reorg recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualChainReplay {
    /// Checkpoint requested by the caller.
    pub requested_checkpoint: TondiBlockHash,
    /// Actual start hash used for replay.
    pub replay_start_hash: TondiBlockHash,
    /// Checkpoint the wallet should persist after applying this replay.
    pub next_checkpoint: TondiBlockHash,
    /// DAG snapshot used as the replay target.
    pub dag_info: DagInfo,
    /// Accumulated virtual-chain delta across all RPC batches.
    pub chain: VirtualChain,
    /// Number of `GetVirtualChainFromBlock` pages consumed.
    pub page_count: u64,
    /// Whether replay used the requested checkpoint or the pruning-point fallback.
    pub mode: VirtualChainReplayMode,
}

impl VirtualChainReplay {
    /// Returns true when the caller must rescan local wallet/RGB state.
    pub fn requires_full_rescan(&self) -> bool {
        self.mode == VirtualChainReplayMode::PruningPointFallback
    }
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
    pub datum: Option<bp::Datum>,
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
        self.version == TONDI_P2MR_SCRIPT_VERSION
            && self.script.len() == 32
            && self.script.iter().any(|byte| *byte != 0)
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
        let txid = bp_txid_from_tondi_txid(outpoint.transaction_id);
        Outpoint::new(txid, Vout::from_u32(outpoint.index))
    }
}

fn bp_txid_from_tondi_txid(txid: TondiTxid) -> Txid {
    Txid::from_str(&txid.to_string()).expect("Tondi txid display must be valid BP txid hex")
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

/// A consuming input or an authenticated, read-only UTXO dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TondiTxInput {
    #[serde(rename = "standard")]
    Standard {
        previous_outpoint: TondiOutpoint,
        #[serde(with = "hex::serde")]
        signature_script: Vec<u8>,
        sequence: u64,
        sig_op_count: u8,
    },
    #[serde(rename = "reference")]
    Reference { previous_outpoint: TondiOutpoint },
}
impl TondiTxInput {
    pub fn previous_outpoint(&self) -> TondiOutpoint {
        match self {
            Self::Standard {
                previous_outpoint, ..
            }
            | Self::Reference { previous_outpoint } => *previous_outpoint,
        }
    }
    pub fn is_reference(&self) -> bool {
        matches!(self, Self::Reference { .. })
    }
}

/// Native output record. Datum is public; RGB private state stays in consignments.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TondiTxOutput {
    pub value: u64,
    pub script_public_key: ScriptPublicKey,
    pub datum: Option<bp::Datum>,
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
    /// Preserve current native fields when crossing the RGB transaction bridge.
    pub fn from_bp_tx(tx: &Transaction) -> Result<Self> {
        let inputs = tx
            .inputs
            .iter()
            .map(|input| {
                if !input.witness.is_empty() {
                    return Err(Error::InvalidNativeTransaction(
                        "P2MR unlock data must be in signature_script".into(),
                    ));
                }
                let point = TondiOutpoint {
                    transaction_id: input
                        .prev_output
                        .txid
                        .to_string()
                        .parse()
                        .map_err(|_| Error::InvalidServerResponse)?,
                    index: input.prev_output.vout.to_u32(),
                };
                if input.is_reference {
                    if !input.sig_script.is_empty()
                        || input.sig_op_count != 0
                        || input.sequence.to_consensus_u32() != 0
                    {
                        return Err(Error::InvalidNativeTransaction(
                            "reference carries spending fields".into(),
                        ));
                    }
                    Ok(TondiTxInput::Reference {
                        previous_outpoint: point,
                    })
                } else {
                    Ok(TondiTxInput::Standard {
                        previous_outpoint: point,
                        signature_script: input.sig_script.to_vec(),
                        sequence: if input.sequence.to_consensus_u32() == u32::MAX {
                            u64::MAX
                        } else {
                            input.sequence.to_consensus_u32() as u64
                        },
                        sig_op_count: input.sig_op_count,
                    })
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let outputs = tx
            .outputs
            .iter()
            .map(|output| TondiTxOutput {
                value: output.value.sats(),
                script_public_key: ScriptPublicKey {
                    version: output.script_pubkey_version,
                    script: output.script_pubkey.to_vec(),
                },
                datum: output.datum.clone(),
            })
            .collect();
        let mut subnetwork = [0; 32];
        subnetwork[..20].copy_from_slice(&tx.subnetwork_id);
        let result = Self {
            version: tx.version.to_consensus_u16(),
            inputs,
            outputs,
            lock_time: tx.lock_time.to_consensus_u64(),
            subnetwork_id: Bytes32::from_byte_array(subnetwork),
            gas: tx.gas,
            payload: tx.payload.to_vec(),
            mass: tx.mass,
        };
        result.validate_tondi_native_rgb()?;
        Ok(result)
    }

    /// Validates the Tondi-native RGB wire contract.
    ///
    /// RGB commitments use P2MR outputs; private state stays in consignments.
    pub fn validate_tondi_native_rgb(&self) -> Result<()> {
        if self.version != bp::TONDI_TX_VERSION {
            return Err(Error::InvalidNativeTransaction(format!(
                "unsupported transaction version {}",
                self.version
            )));
        }
        if !self.payload.is_empty() {
            return Err(Error::InvalidNativeTransaction(
                "RGB private state belongs in consignments, not transaction payload".into(),
            ));
        }
        if self.subnetwork_id.to_byte_array() != [0; 32] || self.gas != 0 {
            return Err(Error::InvalidNativeTransaction(
                "RGB anchors require the native subnetwork and zero gas".into(),
            ));
        }
        let mut points = std::collections::HashSet::new();
        let mut references = 0;
        for input in &self.inputs {
            let point = input.previous_outpoint();
            if !points.insert((point.transaction_id.to_byte_array(), point.index)) {
                return Err(Error::InvalidNativeTransaction(
                    "duplicate input or reference/spend overlap".into(),
                ));
            }
            references += usize::from(input.is_reference());
        }
        if references > 64 {
            return Err(Error::InvalidNativeTransaction(
                "more than 64 reference inputs".into(),
            ));
        }
        for output in &self.outputs {
            output.script_public_key.validate_p2mr_root()?;
        }

        Ok(())
    }

    /// Convert to BP Transaction type
    ///
    /// This is only an in-memory bridge for the RGB runtime. The Tondi wire
    /// contract is validated first: consuming/reference inputs, P2MR outputs
    /// and an empty private-state payload.
    pub fn to_bp_tx(&self) -> Result<Transaction> {
        use amplify::confinement::Confined;

        self.validate_tondi_native_rgb()?;

        let inputs: Vec<TxIn> = self
            .inputs
            .iter()
            .map(|input| match input {
                TondiTxInput::Standard {
                    previous_outpoint,
                    signature_script,
                    sequence,
                    sig_op_count,
                } => Ok(TxIn {
                    prev_output: (*previous_outpoint).into(),
                    sig_script: SigScript::from_checked(signature_script.clone()),
                    sequence: SeqNo::from_consensus_u32(bp_sequence_from_tondi(*sequence)?),
                    witness: Witness::new(),
                    is_reference: false,
                    sig_op_count: *sig_op_count,
                }),
                TondiTxInput::Reference { previous_outpoint } => Ok(TxIn {
                    prev_output: (*previous_outpoint).into(),
                    sig_script: SigScript::new(),
                    sequence: SeqNo::from_consensus_u32(0),
                    witness: Witness::new(),
                    is_reference: true,
                    sig_op_count: 0,
                }),
            })
            .collect::<Result<_>>()?;
        let outputs: Vec<TxOut> = self
            .outputs
            .iter()
            .map(|output| TxOut {
                value: output.value.into(),
                script_pubkey_version: output.script_public_key.version,
                script_pubkey: ScriptPubkey::from_checked(output.script_public_key.script.clone()),
                datum: output.datum.clone(),
            })
            .collect();

        let payload = VarIntBytes::try_from(Vec::new()).map_err(|err| {
            Error::Serialization(format!(
                "failed to create empty Tondi payload bridge: {err}"
            ))
        })?;

        Ok(Transaction {
            version: TxVer::from_consensus_u16(self.version),
            lock_time: LockTime::from_consensus_u64(self.lock_time),
            inputs: Confined::try_from_iter(inputs).map_err(|err| {
                Error::Serialization(format!("too many transaction inputs for RGB bridge: {err}"))
            })?,
            outputs: Confined::try_from_iter(outputs).map_err(|err| {
                Error::Serialization(format!(
                    "too many transaction outputs for RGB bridge: {err}"
                ))
            })?,
            // The adapter's hash-sized wrapper is zero-padded; the native ID is 20 bytes.
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

fn bp_sequence_from_tondi(sequence: u64) -> Result<u32> {
    if sequence == u64::MAX {
        Ok(u32::MAX)
    } else if sequence >= u32::MAX as u64 {
        Err(Error::Serialization(format!(
            "Tondi sequence {sequence} cannot be represented losslessly by the BP bridge"
        )))
    } else {
        Ok(sequence as u32)
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
fn parse_hash_or_zero(hash: &str) -> TondiBlockHash {
    hash.trim_start_matches("0x")
        .parse::<TondiBlockHash>()
        .unwrap_or_else(|_| TondiBlockHash::from_byte_array([0u8; 32]))
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::RpcAcceptedTransactionIds> for AcceptedTransactionIds {
    fn from(ids: crate::client::proto::RpcAcceptedTransactionIds) -> Self {
        AcceptedTransactionIds {
            accepting_block_hash: parse_hash_or_zero(&ids.accepting_block_hash),
            accepting_block_daa_score: ids.accepting_block_daa_score,
            accepted_transaction_ids: ids
                .accepted_transaction_ids
                .iter()
                .map(|txid| parse_hash_or_zero(txid))
                .collect(),
        }
    }
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::GetVirtualChainFromBlockResponseMessage> for VirtualChain {
    fn from(response: crate::client::proto::GetVirtualChainFromBlockResponseMessage) -> Self {
        VirtualChain {
            removed_chain_block_hashes: response
                .removed_chain_block_hashes
                .iter()
                .map(|hash| parse_hash_or_zero(hash))
                .collect(),
            added_chain_block_hashes: response
                .added_chain_block_hashes
                .iter()
                .map(|hash| parse_hash_or_zero(hash))
                .collect(),
            accepted_transaction_ids: response
                .accepted_transaction_ids
                .into_iter()
                .map(AcceptedTransactionIds::from)
                .collect(),
        }
    }
}

#[cfg(feature = "grpc")]
impl TryFrom<crate::client::proto::RpcMempoolEntry> for MempoolEntry {
    type Error = Error;

    fn try_from(entry: crate::client::proto::RpcMempoolEntry) -> Result<Self> {
        let transaction = entry
            .transaction
            .as_ref()
            .ok_or_else(|| Error::InvalidServerResponse)
            .and_then(proto_rpc_tx_to_tondi_tx)?;

        Ok(MempoolEntry {
            fee: entry.fee,
            transaction,
            is_orphan: entry.is_orphan,
        })
    }
}

#[cfg(feature = "grpc")]
impl TryFrom<crate::client::proto::RpcBlock> for TondiBlock {
    type Error = Error;

    fn try_from(block: crate::client::proto::RpcBlock) -> Result<Self> {
        let header = block.header.ok_or(Error::InvalidServerResponse)?;
        let transactions = block
            .transactions
            .iter()
            .map(proto_rpc_tx_to_tondi_tx)
            .collect::<Result<Vec<_>>>()?;

        Ok(TondiBlock {
            header: TondiBlockHeader::from(header),
            transactions,
            verbose_data: block.verbose_data.map(BlockVerboseData::from),
        })
    }
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::RpcBlockHeader> for TondiBlockHeader {
    fn from(header: crate::client::proto::RpcBlockHeader) -> Self {
        TondiBlockHeader {
            version: header.version,
            parents: header
                .parents
                .iter()
                .map(|level| {
                    level
                        .parent_hashes
                        .iter()
                        .map(|hash| parse_hash_or_zero(hash))
                        .collect()
                })
                .collect(),
            hash_merkle_root: parse_hash_or_zero(&header.hash_merkle_root),
            accepted_id_merkle_root: parse_hash_or_zero(&header.accepted_id_merkle_root),
            utxo_commitment: parse_hash_or_zero(&header.utxo_commitment),
            timestamp: header.timestamp as u64,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: parse_hash_or_zero(&header.pruning_point),
        }
    }
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::RpcBlockVerboseData> for BlockVerboseData {
    fn from(data: crate::client::proto::RpcBlockVerboseData) -> Self {
        BlockVerboseData {
            hash: parse_hash_or_zero(&data.hash),
            difficulty: data.difficulty,
            selected_parent_hash: parse_hash_or_zero(&data.selected_parent_hash),
            transaction_ids: data
                .transaction_ids
                .iter()
                .map(|txid| parse_hash_or_zero(txid))
                .collect(),
            is_chain_block: data.is_chain_block,
            merge_set_blues_hashes: data
                .merge_set_blues_hashes
                .iter()
                .map(|hash| parse_hash_or_zero(hash))
                .collect(),
            merge_set_reds_hashes: data
                .merge_set_reds_hashes
                .iter()
                .map(|hash| parse_hash_or_zero(hash))
                .collect(),
            children_hashes: data
                .children_hashes
                .iter()
                .map(|hash| parse_hash_or_zero(hash))
                .collect(),
        }
    }
}

#[cfg(feature = "grpc")]
pub(crate) fn proto_rpc_tx_to_tondi_tx(
    rpc_tx: &crate::client::proto::RpcTransaction,
) -> Result<TondiTransaction> {
    use crate::client::proto::rpc_transaction_input::Input;
    let inputs = rpc_tx
        .inputs
        .iter()
        .map(|input| {
            let point = input
                .previous_outpoint
                .as_ref()
                .ok_or(Error::InvalidServerResponse)?;
            let previous_outpoint = TondiOutpoint {
                transaction_id: point
                    .transaction_id
                    .parse()
                    .map_err(|_| Error::InvalidServerResponse)?,
                index: point.index,
            };
            Ok(
                match input.input.as_ref().ok_or(Error::InvalidServerResponse)? {
                    Input::Standard(s) => TondiTxInput::Standard {
                        previous_outpoint,
                        signature_script: decode_hex_field(
                            &s.signature_script,
                            "signature_script",
                        )?,
                        sequence: s.sequence,
                        sig_op_count: s
                            .sig_op_count
                            .try_into()
                            .map_err(|_| Error::InvalidServerResponse)?,
                    },
                    Input::Reference(_) => TondiTxInput::Reference { previous_outpoint },
                },
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let outputs = rpc_tx
        .outputs
        .iter()
        .map(|output| {
            let spk = output
                .script_public_key
                .as_ref()
                .ok_or(Error::InvalidServerResponse)?;
            Ok(TondiTxOutput {
                value: output.amount,
                script_public_key: ScriptPublicKey {
                    version: spk
                        .version
                        .try_into()
                        .map_err(|_| Error::InvalidServerResponse)?,
                    script: decode_hex_field(&spk.script_public_key, "script_public_key")?,
                },
                datum: output
                    .datum
                    .clone()
                    .map(bp::Datum::try_from)
                    .transpose()
                    .map_err(|_| Error::InvalidServerResponse)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let subnet = decode_hex_field(&rpc_tx.subnetwork_id, "subnetwork_id")?;
    if subnet.len() != 20 {
        return Err(Error::InvalidServerResponse);
    }
    let mut subnetwork = [0; 32];
    subnetwork[..20].copy_from_slice(&subnet);
    Ok(TondiTransaction {
        version: rpc_tx
            .version
            .try_into()
            .map_err(|_| Error::InvalidServerResponse)?,
        inputs,
        outputs,
        lock_time: rpc_tx.lock_time,
        subnetwork_id: Bytes32::from_byte_array(subnetwork),
        gas: rpc_tx.gas,
        payload: decode_hex_field(&rpc_tx.payload, "payload")?,
        mass: rpc_tx.mass,
    })
}

#[cfg(feature = "grpc")]
fn decode_hex_field(value: &str, field: &str) -> Result<Vec<u8>> {
    hex::decode(value.trim_start_matches("0x"))
        .map_err(|err| Error::Serialization(format!("failed to decode {field}: {err}")))
}

#[cfg(feature = "grpc")]
impl TryFrom<crate::client::proto::RpcUtxosByAddressesEntry> for UtxosByAddressEntry {
    type Error = Error;
    fn try_from(entry: crate::client::proto::RpcUtxosByAddressesEntry) -> Result<Self> {
        let point = entry.outpoint.ok_or(Error::InvalidServerResponse)?;
        Ok(Self {
            address: (!entry.address.is_empty()).then_some(entry.address),
            outpoint: TondiOutpoint {
                transaction_id: point
                    .transaction_id
                    .parse()
                    .map_err(|_| Error::InvalidServerResponse)?,
                index: point.index,
            },
            utxo_entry: entry
                .utxo_entry
                .ok_or(Error::InvalidServerResponse)?
                .try_into()?,
        })
    }
}

#[cfg(feature = "grpc")]
impl From<crate::client::proto::RpcBalancesByAddressesEntry> for BalanceByAddressEntry {
    fn from(entry: crate::client::proto::RpcBalancesByAddressesEntry) -> Self {
        BalanceByAddressEntry {
            address: entry.address,
            balance: if entry.error.is_some() {
                None
            } else {
                Some(entry.balance)
            },
        }
    }
}

#[cfg(feature = "grpc")]
impl TryFrom<crate::client::proto::RpcUtxoEntry> for TondiUtxoEntry {
    type Error = Error;
    fn try_from(entry: crate::client::proto::RpcUtxoEntry) -> Result<Self> {
        let spk = entry
            .script_public_key
            .ok_or(Error::InvalidServerResponse)?;
        Ok(Self {
            amount: entry.amount,
            script_public_key: ScriptPublicKey {
                version: spk
                    .version
                    .try_into()
                    .map_err(|_| Error::InvalidServerResponse)?,
                script: decode_hex_field(&spk.script_public_key, "script_public_key")?,
            },
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
            datum: entry
                .datum
                .map(bp::Datum::try_from)
                .transpose()
                .map_err(|_| Error::InvalidServerResponse)?,
        })
    }
}
