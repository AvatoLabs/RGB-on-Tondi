//! Official RPC4 transports. No copied protobuf or alternate transaction codec.
use crate::types::{bytes, hash};
use crate::*;
use amplify::ByteArray;
use std::{sync::Arc, time::Duration};
use tondi_rpc_core::{api::rpc::RpcApi, *};
#[derive(Clone, Debug)]
pub struct TondiConfig {
    pub endpoint: String,
    pub network: NetworkType,
    pub timeout: Option<Duration>,
}
impl TondiConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            network: NetworkType::Mainnet,
            timeout: Some(Duration::from_secs(30)),
        }
    }
    pub fn with_network(mut self, network: NetworkType) -> Self {
        self.network = network;
        self
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    pub fn mainnet(e: impl Into<String>) -> Self {
        Self::new(e)
    }
    pub fn testnet(e: impl Into<String>) -> Self {
        Self::new(e).with_network(NetworkType::Testnet)
    }
}
#[derive(Clone)]
pub struct TondiClient {
    config: TondiConfig,
    inner: Arc<dyn RpcApi>,
    transport: Transport,
}
#[derive(Clone)]
enum Transport {
    Grpc(tondi_grpc_client::GrpcClient),
    Wrpc(tondi_wrpc_client::TondiRpcClient),
}
impl std::fmt::Debug for TondiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TondiClient")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
fn rpc(e: RpcError) -> Error {
    match e {
        RpcError::TransactionNotFound(id) => {
            Error::TransactionNotFound(bp::Txid::from_byte_array(id.as_bytes()))
        }
        RpcError::NoUtxoIndex => Error::UtxoIndexNotEnabled,
        _ => Error::RpcCall(e.to_string()),
    }
}
impl TondiClient {
    pub async fn connect(config: TondiConfig) -> Result<Self> {
        let (inner, transport): (Arc<dyn RpcApi>, Transport) =
            if config.endpoint.starts_with("ws://") || config.endpoint.starts_with("wss://") {
                let c = tondi_wrpc_client::TondiRpcClient::new(
                    tondi_wrpc_client::WrpcEncoding::Borsh,
                    Some(&config.endpoint),
                    None,
                    Some(config.network.id()),
                    None,
                )
                .map_err(|e| Error::Connection(e.to_string()))?;
                let options = tondi_wrpc_client::prelude::ConnectOptions::blocking_fallback();
                c.connect(Some(options))
                    .await
                    .map_err(|e| Error::Connection(e.to_string()))?;
                (Arc::new(c.clone()), Transport::Wrpc(c))
            } else {
                let url = if config.endpoint.starts_with("grpc://") {
                    config.endpoint.clone()
                } else if let Some(e) = config.endpoint.strip_prefix("http://") {
                    format!("grpc://{e}")
                } else if config.endpoint.contains("://") {
                    return Err(Error::Connection("use grpc://, ws:// or wss://".into()));
                } else {
                    format!("grpc://{}", config.endpoint)
                };
                let c = tondi_grpc_client::GrpcClient::connect_with_args(
                    tondi_rpc_core::notify::mode::NotificationMode::Direct,
                    url,
                    None,
                    false,
                    None,
                    config.timeout.map(|d| d.as_millis() as u64),
                    Default::default(),
                )
                .await
                .map_err(|e| Error::Connection(e.to_string()))?;
                (Arc::new(c.clone()), Transport::Grpc(c))
            };
        let client = Self {
            config,
            inner,
            transport,
        };
        let info = client.get_server_info().await?;
        if info.rpc_api_version != 4 || info.network_id != client.config.network.id() {
            client.disconnect().await?;
            return Err(Error::InvalidServerResponse);
        }
        Ok(client)
    }
    pub async fn disconnect(&self) -> Result<()> {
        match &self.transport {
            Transport::Grpc(c) => c
                .disconnect()
                .await
                .map_err(|e| Error::Connection(e.to_string())),
            Transport::Wrpc(c) => c
                .disconnect()
                .await
                .map_err(|e| Error::Connection(e.to_string())),
        }
    }
    pub fn native_api(&self) -> &dyn RpcApi {
        self.inner.as_ref()
    }
    pub fn config(&self) -> &TondiConfig {
        &self.config
    }
    pub async fn ping(&self) -> Result<()> {
        self.inner.ping().await.map_err(rpc)
    }
    pub async fn get_server_info(&self) -> Result<ServerInfo> {
        self.inner.get_server_info().await.map_err(rpc)
    }
    pub async fn get_sync_status(&self) -> Result<bool> {
        self.inner.get_sync_status().await.map_err(rpc)
    }
    pub async fn get_current_network(&self) -> Result<NetworkType> {
        self.get_server_info().await?;
        Ok(self.config.network)
    }
    pub async fn submit_transaction(
        &self,
        tx: TondiTransaction,
        allow_orphan: bool,
    ) -> Result<TondiTxid> {
        tx.validate(self.config.network.domain())?;
        let id = tx.id();
        let result = self
            .inner
            .submit_transaction(
                RpcTransaction {
                    transaction: tondi_consensus_core::tx::Transaction { native: tx.0 },
                    verbose_data: None,
                },
                allow_orphan,
            )
            .await
            .map_err(rpc)?;
        if id != result {
            return Err(Error::InvalidServerResponse);
        }
        Ok(bytes(result))
    }
    pub async fn get_transaction(&self, id: TondiTxid) -> Result<TondiTransaction> {
        let tx = match self.inner.get_transaction(hash(id)).await {
            Ok(tx) => tx,
            Err(e) => {
                let s = self
                    .inner
                    .get_transaction_status(hash(id))
                    .await
                    .map_err(rpc)?;
                if s.status == RpcTransactionStatusKind::Unknown {
                    return Err(Error::TransactionNotFound(bp::Txid::from_byte_array(
                        id.to_byte_array(),
                    )));
                }
                return Err(rpc(e));
            }
        };
        if tx.id() != hash(id) || tx.network != self.config.network.domain() {
            return Err(Error::InvalidServerResponse);
        }
        Ok(TondiTransaction(tx.transaction.native))
    }
    pub async fn get_transaction_status(&self, id: TondiTxid) -> Result<TxStatus> {
        let s = self
            .inner
            .get_transaction_status(hash(id))
            .await
            .map_err(rpc)?;
        if s.transaction_id != hash(id) {
            return Err(Error::InvalidServerResponse);
        }
        let accepted = s.status == RpcTransactionStatusKind::Accepted;
        if accepted && (s.accepting_block_hash.is_none() || s.accepting_block_daa_score.is_none()) {
            return Err(Error::InvalidServerResponse);
        }
        Ok(TxStatus {
            accepted,
            in_mempool: s.in_mempool || s.in_orphan_pool,
            removed: s.status == RpcTransactionStatusKind::Removed,
            accepting_block_hash: s.accepting_block_hash.map(bytes),
            accepting_block_daa_score: s.accepting_block_daa_score,
            block_time: None,
        })
    }
    pub async fn get_mempool_entry(&self, id: TondiTxid) -> Result<MempoolEntry> {
        let m = self
            .inner
            .get_mempool_entry(hash(id), true, false)
            .await
            .map_err(rpc)?;
        if m.transaction.id() != hash(id) {
            return Err(Error::InvalidServerResponse);
        }
        Ok(MempoolEntry {
            fee: m.fee,
            transaction: TondiTransaction(m.transaction.transaction.native),
            is_orphan: m.is_orphan,
        })
    }
    pub async fn get_mempool_entries(&self) -> Result<Vec<MempoolEntry>> {
        Ok(self
            .inner
            .get_mempool_entries(true, false)
            .await
            .map_err(rpc)?
            .into_iter()
            .map(|m| MempoolEntry {
                fee: m.fee,
                transaction: TondiTransaction(m.transaction.transaction.native),
                is_orphan: m.is_orphan,
            })
            .collect())
    }
    pub async fn get_block(&self, h: TondiBlockHash, include: bool) -> Result<TondiBlock> {
        self.inner.get_block(hash(h), include).await.map_err(rpc)
    }
    pub async fn get_sink(&self) -> Result<TondiBlockHash> {
        Ok(bytes(self.inner.get_sink().await.map_err(rpc)?.sink))
    }
    pub async fn get_block_dag_info(&self) -> Result<DagInfo> {
        let i = self.inner.get_block_dag_info().await.map_err(rpc)?;
        if i.network != self.config.network.id() {
            return Err(Error::InvalidServerResponse);
        }
        Ok(DagInfo {
            network_name: i.network.to_string(),
            block_count: i.block_count,
            header_count: i.header_count,
            tip_hashes: i.tip_hashes.into_iter().map(bytes).collect(),
            difficulty: i.difficulty,
            past_median_time: i.past_median_time,
            virtual_parent_hashes: i.virtual_parent_hashes.into_iter().map(bytes).collect(),
            pruning_point_hash: bytes(i.pruning_point_hash),
            virtual_daa_score: i.virtual_daa_score,
            sink: bytes(i.sink),
        })
    }
    /// The node's deterministic virtual clock; host wall time is never used for Since.
    pub async fn clock(&self) -> Result<tondi_vm::ClockContext> {
        let d = self.get_block_dag_info().await?;
        Ok(tondi_vm::ClockContext {
            daa_score: d.virtual_daa_score,
            time_seconds: d.past_median_time / 1000,
        })
    }
    pub async fn get_block_count(&self) -> Result<u64> {
        Ok(self.inner.get_block_count().await.map_err(rpc)?.block_count)
    }
    pub async fn get_balance_by_address(&self, a: &str) -> Result<u64> {
        self.inner
            .get_balance_by_address(a.parse().map_err(|e| Error::Address(format!("{e}")))?)
            .await
            .map_err(rpc)
    }
    pub async fn get_balances_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<BalanceByAddressEntry>> {
        let a = addresses
            .iter()
            .map(|a| a.parse().map_err(|e| Error::Address(format!("{e}"))))
            .collect::<Result<Vec<_>>>()?;
        self.inner.get_balances_by_addresses(a).await.map_err(rpc)
    }
    pub async fn get_utxos_by_address(&self, a: &str) -> Result<Vec<UtxosByAddressEntry>> {
        self.get_utxos_by_addresses(&[a.into()]).await
    }
    pub async fn get_utxos_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<UtxosByAddressEntry>> {
        let a = addresses
            .iter()
            .map(|a| a.parse().map_err(|e| Error::Address(format!("{e}"))))
            .collect::<Result<Vec<_>>>()?;
        Ok(self
            .inner
            .get_utxos_by_addresses(a)
            .await
            .map_err(rpc)?
            .into_iter()
            .map(|e| UtxosByAddressEntry {
                address: e.address.map(|a| a.to_string()),
                outpoint: TondiOutpoint {
                    transaction_id: bytes(e.outpoint.transaction_id),
                    index: e.outpoint.index,
                },
                utxo_entry: tondi_vm::ResolvedInput {
                    note: e.utxo_entry.note,
                    birth_daa_score: e.utxo_entry.birth_daa_score,
                    is_coinbase: e.utxo_entry.is_coinbase,
                },
            })
            .collect())
    }
    pub async fn get_fee_estimate(&self) -> Result<FeeEstimate> {
        self.inner.get_fee_estimate().await.map_err(rpc)
    }
    pub fn convert_fee_rate(target_seconds: u64, e: &FeeEstimate) -> Option<f64> {
        std::iter::once(&e.priority_bucket)
            .chain(&e.normal_buckets)
            .chain(&e.low_buckets)
            .filter(|b| b.estimated_seconds <= target_seconds as f64)
            .map(|b| b.feerate)
            .reduce(f64::min)
    }
    pub async fn get_coin_supply(&self) -> Result<(u64, u64)> {
        let s = self.inner.get_coin_supply().await.map_err(rpc)?;
        Ok((s.circulating_sau, s.max_sau))
    }
    pub async fn estimate_network_hashes_per_second(
        &self,
        w: u32,
        start: Option<TondiBlockHash>,
    ) -> Result<u64> {
        self.inner
            .estimate_network_hashes_per_second(w, start.map(hash))
            .await
            .map_err(rpc)
    }
    pub async fn get_virtual_chain_from_block(
        &self,
        start: TondiBlockHash,
        include: bool,
        min: Option<u64>,
    ) -> Result<VirtualChain> {
        let mut request = GetVirtualChainFromBlockRequest::new(hash(start), include);
        request.min_confirmation_count = min;
        let c = self
            .inner
            .get_virtual_chain_from_block_call(None, request)
            .await
            .map_err(rpc)?;
        Ok(VirtualChain {
            removed_chain_block_hashes: c
                .removed_chain_block_hashes
                .into_iter()
                .map(bytes)
                .collect(),
            added_chain_block_hashes: c.added_chain_block_hashes.into_iter().map(bytes).collect(),
            accepted_transaction_ids: c
                .accepted_transaction_ids
                .into_iter()
                .map(|a| AcceptedTransactionIds {
                    accepting_block_hash: bytes(a.accepting_block_hash),
                    accepting_block_daa_score: a.accepting_block_daa_score,
                    accepted_transaction_ids: a
                        .accepted_transaction_ids
                        .into_iter()
                        .map(bytes)
                        .collect(),
                })
                .collect(),
        })
    }
    pub async fn replay_virtual_chain_from_checkpoint(
        &self,
        checkpoint: TondiBlockHash,
        include: bool,
    ) -> Result<VirtualChainReplay> {
        let dag = self.get_block_dag_info().await?;
        let mut cursor = checkpoint;
        let mut chain = VirtualChain::default();
        let mut pages = 0;
        while cursor != dag.sink {
            if pages >= 10_000 {
                return Err(Error::InvalidServerResponse);
            }
            let page = self
                .get_virtual_chain_from_block(cursor, include, None)
                .await?;
            pages += 1;
            let next = page
                .added_chain_block_hashes
                .last()
                .copied()
                .ok_or(Error::InvalidServerResponse)?;
            if next == cursor {
                return Err(Error::InvalidServerResponse);
            }
            chain
                .removed_chain_block_hashes
                .extend(page.removed_chain_block_hashes);
            chain
                .added_chain_block_hashes
                .extend(page.added_chain_block_hashes);
            chain
                .accepted_transaction_ids
                .extend(page.accepted_transaction_ids);
            cursor = next;
        }
        Ok(VirtualChainReplay {
            requested_checkpoint: checkpoint,
            replay_start_hash: checkpoint,
            next_checkpoint: cursor,
            dag_info: dag,
            chain,
            page_count: pages,
            mode: VirtualChainReplayMode::Checkpoint,
        })
    }
}
