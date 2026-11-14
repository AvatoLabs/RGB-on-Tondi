//! Native v4 records; transaction bodies and UTXO Notes are never reconstructed field by field.
use crate::{Error, Result};
use amplify::{ByteArray, Bytes32};
use serde::{Deserialize, Serialize};
pub type TondiTxid = Bytes32;
pub type TondiBlockHash = Bytes32;
pub type DaaScore = u64;
pub type BlueScore = u64;
pub type TondiUtxoEntry = tondi_vm::ResolvedInput;
pub type TondiBlock = tondi_rpc_core::RpcBlock;
pub type TondiBlockHeader = tondi_rpc_core::RpcHeader;
pub type ServerInfo = tondi_rpc_core::GetServerInfoResponse;
pub type FeeEstimate = tondi_rpc_core::RpcFeeEstimate;
pub type FeeBucket = tondi_rpc_core::RpcFeerateBucket;
pub type BalanceByAddressEntry = tondi_rpc_core::RpcBalancesByAddressesEntry;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkType {
    #[default]
    Mainnet,
    Testnet,
    Simnet,
    Devnet,
}
impl NetworkType {
    pub fn id(self) -> tondi_consensus_core::network::NetworkId {
        use tondi_consensus_core::network::{NetworkId, NetworkType as N};
        match self {
            Self::Mainnet => NetworkId::try_new(N::Mainnet).expect("mainnet id"),
            Self::Testnet => NetworkId::with_suffix(N::Testnet, 10),
            Self::Simnet => NetworkId::try_new(N::Simnet).expect("simnet id"),
            Self::Devnet => NetworkId::try_new(N::Devnet).expect("devnet id"),
        }
    }
    pub fn domain(self) -> tondi_vm::Hash32 {
        use tondi_consensus_core::config::genesis::*;
        match self {
            Self::Mainnet => GENESIS,
            Self::Testnet => TESTNET_GENESIS,
            Self::Simnet => SIMNET_GENESIS,
            Self::Devnet => DEVNET_GENESIS,
        }
        .network_domain
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TondiOutpoint {
    pub transaction_id: TondiTxid,
    pub index: u32,
}
impl From<TondiOutpoint> for bp::Outpoint {
    fn from(o: TondiOutpoint) -> Self {
        Self::new(
            bp::Txid::from_byte_array(o.transaction_id.to_byte_array()),
            bp::Vout::from_u32(o.index),
        )
    }
}
impl From<TondiOutpoint> for tondi_vm::OutPoint {
    fn from(o: TondiOutpoint) -> Self {
        Self {
            transaction_id: tondi_vm::Hash32::from_bytes(o.transaction_id.to_byte_array()),
            index: o.index,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UtxosByAddressEntry {
    pub address: Option<String>,
    pub outpoint: TondiOutpoint,
    pub utxo_entry: TondiUtxoEntry,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TondiTransaction(pub tondi_vm::Transition);
impl std::ops::Deref for TondiTransaction {
    type Target = tondi_vm::Transition;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl TondiTransaction {
    pub fn from_bp_tx(tx: &bp::Tx) -> Result<Self> {
        Ok(Self(tx.native().clone()))
    }
    pub fn to_bp_tx(&self) -> Result<bp::Tx> {
        bp::Tx::from_native(self.0.clone())
            .map_err(|e| Error::InvalidNativeTransaction(e.to_string()))
    }
    pub fn validate(&self, network: tondi_vm::Hash32) -> Result<()> {
        tondi_vm::validation::validate_structure(&self.0, network, &tondi_vm::Limits::default())
            .map_err(|e| Error::InvalidNativeTransaction(e.to_string()))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxStatus {
    pub accepted: bool,
    pub in_mempool: bool,
    pub removed: bool,
    pub accepting_block_hash: Option<TondiBlockHash>,
    pub accepting_block_daa_score: Option<u64>,
    pub block_time: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedTransactionIds {
    pub accepting_block_hash: TondiBlockHash,
    pub accepting_block_daa_score: Option<u64>,
    pub accepted_transaction_ids: Vec<TondiTxid>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualChain {
    pub removed_chain_block_hashes: Vec<TondiBlockHash>,
    pub added_chain_block_hashes: Vec<TondiBlockHash>,
    pub accepted_transaction_ids: Vec<AcceptedTransactionIds>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualChainReplayMode {
    Checkpoint,
    PruningPointFallback,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualChainReplay {
    pub requested_checkpoint: TondiBlockHash,
    pub replay_start_hash: TondiBlockHash,
    pub next_checkpoint: TondiBlockHash,
    pub dag_info: DagInfo,
    pub chain: VirtualChain,
    pub page_count: u64,
    pub mode: VirtualChainReplayMode,
}
impl VirtualChainReplay {
    pub fn requires_full_rescan(&self) -> bool {
        self.mode == VirtualChainReplayMode::PruningPointFallback
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DagInfo {
    pub network_name: String,
    pub block_count: u64,
    pub header_count: u64,
    pub tip_hashes: Vec<TondiBlockHash>,
    pub difficulty: f64,
    pub past_median_time: u64,
    pub virtual_parent_hashes: Vec<TondiBlockHash>,
    pub pruning_point_hash: TondiBlockHash,
    pub virtual_daa_score: u64,
    pub sink: TondiBlockHash,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MempoolEntry {
    pub fee: u64,
    pub transaction: TondiTransaction,
    pub is_orphan: bool,
}
pub(crate) fn hash(h: Bytes32) -> tondi_vm::Hash32 {
    tondi_vm::Hash32::from_bytes(h.to_byte_array())
}
pub(crate) fn bytes(h: tondi_vm::Hash32) -> Bytes32 {
    Bytes32::from_byte_array(h.as_bytes())
}
