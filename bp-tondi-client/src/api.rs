//! API trait definitions for Tondi RPC client
//!
//! This module defines the public API interface for the Tondi client,
//! providing a consistent interface for both sync and async implementations.

use crate::{
    BalanceByAddressEntry, DagInfo, FeeEstimate, MempoolEntry, Result, ServerInfo, TondiBlock, TondiBlockHash, TondiTransaction,
    TondiTxid, TxStatus, UtxosByAddressEntry,
};

#[cfg(feature = "async")]
use async_trait::async_trait;

/// Tondi RPC API trait
///
/// Defines all RPC methods available for interacting with a Tondi node.
/// This trait can be implemented for different transport layers (gRPC, wRPC, etc.)
#[cfg(feature = "async")]
#[async_trait]
pub trait TondiRpcApi: Send + Sync {
    /// Check connection status
    async fn ping(&self) -> Result<()>;

    /// Get server information
    async fn get_server_info(&self) -> Result<ServerInfo>;

    /// Get sync status
    async fn get_sync_status(&self) -> Result<bool>;

    // Transaction operations
    /// Submit a transaction to the mempool
    async fn submit_transaction(&self, transaction: TondiTransaction, allow_orphan: bool) -> Result<TondiTxid>;

    /// Get transaction by ID
    async fn get_transaction(&self, txid: TondiTxid) -> Result<TondiTransaction>;

    /// Get transaction status
    async fn get_transaction_status(&self, txid: TondiTxid) -> Result<TxStatus>;

    /// Get mempool entry
    async fn get_mempool_entry(&self, txid: TondiTxid) -> Result<MempoolEntry>;

    /// Get all mempool entries
    async fn get_mempool_entries(&self) -> Result<Vec<MempoolEntry>>;

    // Block operations
    /// Get block by hash
    async fn get_block(&self, hash: TondiBlockHash, include_transactions: bool) -> Result<TondiBlock>;

    /// Get current DAG sink
    async fn get_sink(&self) -> Result<TondiBlockHash>;

    /// Get DAG information
    async fn get_block_dag_info(&self) -> Result<DagInfo>;

    /// Get block count
    async fn get_block_count(&self) -> Result<u64>;

    // Address/UTXO operations
    /// Get balance for an address
    async fn get_balance_by_address(&self, address: &str) -> Result<u64>;

    /// Get balances for multiple addresses
    async fn get_balances_by_addresses(&self, addresses: &[String]) -> Result<Vec<BalanceByAddressEntry>>;

    /// Get UTXOs for an address
    async fn get_utxos_by_address(&self, address: &str) -> Result<Vec<UtxosByAddressEntry>>;

    /// Get UTXOs for multiple addresses
    async fn get_utxos_by_addresses(&self, addresses: &[String]) -> Result<Vec<UtxosByAddressEntry>>;

    // Fee estimation
    /// Get fee estimate
    async fn get_fee_estimate(&self) -> Result<FeeEstimate>;

    // Coin supply
    /// Get current coin supply (circulating, max)
    async fn get_coin_supply(&self) -> Result<(u64, u64)>;

    // Network info
    /// Estimate network hash rate
    async fn estimate_network_hashes_per_second(&self, window_size: u32, start_hash: Option<TondiBlockHash>) -> Result<u64>;
}

/// Notification scope for subscriptions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationScope {
    /// New block added to DAG
    BlockAdded,
    /// New block template available
    NewBlockTemplate,
    /// UTXO changes for subscribed addresses
    UtxosChanged,
    /// Virtual DAA score changed
    VirtualDaaScoreChanged,
    /// Virtual chain changed
    VirtualChainChanged,
    /// Sink blue score changed
    SinkBlueScoreChanged,
    /// Finality conflict detected
    FinalityConflict,
    /// Finality conflict resolved
    FinalityConflictResolved,
    /// Pruning point UTXO set override
    PruningPointUtxoSetOverride,
}

/// Notification event types
#[derive(Debug, Clone)]
pub enum TondiNotification {
    /// Block added to DAG
    BlockAdded { block_hash: TondiBlockHash, block: TondiBlock },
    /// New block template available
    NewBlockTemplate { block_hash: TondiBlockHash },
    /// UTXOs changed for watched addresses
    UtxosChanged { added: Vec<UtxosByAddressEntry>, removed: Vec<UtxosByAddressEntry> },
    /// Virtual DAA score changed
    VirtualDaaScoreChanged { virtual_daa_score: u64 },
    /// Virtual chain changed
    VirtualChainChanged {
        removed_chain_block_hashes: Vec<TondiBlockHash>,
        added_chain_block_hashes: Vec<TondiBlockHash>,
        accepted_transaction_ids: Vec<TondiTxid>,
    },
    /// Sink blue score changed
    SinkBlueScoreChanged { sink_blue_score: u64 },
    /// Finality conflict detected
    FinalityConflict { violating_block_hash: TondiBlockHash },
    /// Finality conflict resolved
    FinalityConflictResolved { finality_block_hash: TondiBlockHash },
}

/// Listener for notifications
#[cfg(feature = "async")]
#[async_trait]
pub trait TondiNotificationListener: Send + Sync {
    /// Called when a notification is received
    async fn on_notification(&self, notification: TondiNotification);
}
