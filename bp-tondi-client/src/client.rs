//! Tondi RPC Client implementation

use std::collections::HashMap;
use std::time::Duration;

#[cfg(feature = "grpc")]
use amplify::{hex::ToHex, ByteArray, Bytes32};

use crate::{
    BalanceByAddressEntry, DagInfo, Error, FeeBucket, FeeEstimate, MempoolEntry, NetworkType,
    Result, ServerInfo, TondiBlock, TondiBlockHash, TondiTransaction, TondiTxid, TxStatus,
    UtxosByAddressEntry, VirtualChain, VirtualChainReplay, VirtualChainReplayMode,
};

// gRPC proto
#[cfg(feature = "grpc")]
pub mod proto {
    tonic::include_proto!("protowire");
}

#[cfg(feature = "grpc")]
fn next_request_id() -> Result<u64> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| Error::RpcCall(format!("System clock before UNIX_EPOCH: {err}")))?;

    Ok(duration.as_nanos() as u64)
}

#[cfg(feature = "grpc")]
fn tondi_tx_to_proto(tx: &TondiTransaction) -> Result<proto::RpcTransaction> {
    use crate::types::{TondiTxInput, TondiTxOutput};

    tx.validate_tondi_native_rgb()?;

    // Tondi v1.22.0 compatible proto format: flat fields without oneof wrappers.
    let rpc_inputs: Vec<proto::RpcTransactionInput> = tx
        .inputs
        .iter()
        .map(|input| match input {
            TondiTxInput::Standard {
                previous_outpoint,
                signature_script,
                sequence,
                sig_op_count,
            } => Ok(proto::RpcTransactionInput {
                previous_outpoint: Some(proto::RpcOutpoint {
                    transaction_id: previous_outpoint.transaction_id.to_hex(),
                    index: previous_outpoint.index,
                }),
                verbose_data: None,
                input: Some(proto::rpc_transaction_input::Input::Standard(
                    proto::RpcStandardInput {
                        signature_script: signature_script.to_hex(),
                        sequence: *sequence,
                        sig_op_count: *sig_op_count as u32,
                    },
                )),
            }),
            _ => Err(Error::InvalidNativeTransaction(
                "RGB-on-Tondi only supports Tondi Standard inputs".to_string(),
            )),
        })
        .collect::<Result<Vec<_>>>()?;

    let rpc_outputs: Vec<proto::RpcTransactionOutput> = tx
        .outputs
        .iter()
        .map(|output| match output {
            TondiTxOutput::Standard {
                value,
                script_public_key,
            } => Ok(proto::RpcTransactionOutput {
                amount: *value,
                verbose_data: None,
                output: Some(proto::rpc_transaction_output::Output::Standard(
                    proto::RpcStandardOutput {
                        script_public_key: Some(proto::RpcScriptPublicKey {
                            version: script_public_key.version as u32,
                            script_public_key: script_public_key.script.to_hex(),
                        }),
                    },
                )),
            }),
            _ => Err(Error::InvalidNativeTransaction(
                "RGB-on-Tondi only supports Tondi Standard P2MR outputs".to_string(),
            )),
        })
        .collect::<Result<Vec<_>>>()?;

    // Keep subnetwork ID and payload encoding compatible with the Tondi gRPC server.
    // Hex string fields are sent without a 0x prefix.
    let subnetwork_id_hex = {
        let bytes32 = tx.subnetwork_id.to_byte_array();
        let mut subnetwork_id_20 = [0u8; 20];
        subnetwork_id_20.copy_from_slice(&bytes32[..20]);
        // Always send 40 plain hex characters.
        hex::encode(subnetwork_id_20)
    };

    // Empty payloads are encoded as an empty string; non-empty payloads use plain hex.
    let payload_encoded = if tx.payload.is_empty() {
        "".to_string()
    } else {
        tx.payload.to_hex()
    };

    Ok(proto::RpcTransaction {
        version: tx.version as u32,
        inputs: rpc_inputs,
        outputs: rpc_outputs,
        lock_time: tx.lock_time,
        subnetwork_id: subnetwork_id_hex,
        gas: tx.gas,
        payload: payload_encoded,
        mass: tx.mass,
        verbose_data: None,
    })
}

/// Convert proto::RpcTransaction back to TondiTransaction
#[cfg(feature = "grpc")]
fn proto_to_tondi_tx(rpc_tx: &proto::RpcTransaction) -> Result<TondiTransaction> {
    use crate::types::{ScriptPublicKey, TondiOutpoint, TondiTxInput, TondiTxOutput};

    // Convert inputs
    let inputs: Vec<TondiTxInput> = rpc_tx
        .inputs
        .iter()
        .map(|rpc_input| {
            let previous_outpoint = rpc_input
                .previous_outpoint
                .as_ref()
                .ok_or_else(|| Error::RpcCall("Missing previous_outpoint".into()))?;

            let txid = hex::decode(&previous_outpoint.transaction_id)
                .map_err(|e| Error::RpcCall(format!("Failed to decode transaction_id: {}", e)))?;

            if txid.len() != 32 {
                return Err(Error::RpcCall(format!(
                    "Invalid transaction_id length: {} (expected 32)",
                    txid.len()
                )));
            }

            let txid_bytes: [u8; 32] = txid
                .try_into()
                .map_err(|_| Error::RpcCall("Failed to convert txid to array".into()))?;

            let Some(proto::rpc_transaction_input::Input::Standard(standard)) =
                rpc_input.input.as_ref()
            else {
                return Err(Error::InvalidNativeTransaction(
                    "RGB-on-Tondi only supports Tondi Standard inputs".to_string(),
                ));
            };

            let signature_script = hex::decode(&standard.signature_script)
                .map_err(|e| Error::RpcCall(format!("Failed to decode signature_script: {}", e)))?;

            Ok(TondiTxInput::Standard {
                previous_outpoint: TondiOutpoint {
                    transaction_id: Bytes32::from_byte_array(txid_bytes),
                    index: previous_outpoint.index,
                },
                signature_script,
                sequence: standard.sequence,
                sig_op_count: standard.sig_op_count as u8,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Convert outputs
    let outputs: Vec<TondiTxOutput> = rpc_tx
        .outputs
        .iter()
        .map(|rpc_output| {
            let Some(proto::rpc_transaction_output::Output::Standard(standard)) =
                rpc_output.output.as_ref()
            else {
                return Err(Error::InvalidNativeTransaction(
                    "RGB-on-Tondi only supports Tondi Standard outputs".to_string(),
                ));
            };
            let script_pubkey = standard
                .script_public_key
                .as_ref()
                .ok_or_else(|| Error::RpcCall("Missing script_public_key".into()))?;

            let script = hex::decode(&script_pubkey.script_public_key)
                .map_err(|e| Error::RpcCall(format!("Failed to decode script: {}", e)))?;

            Ok(TondiTxOutput::Standard {
                value: rpc_output.amount,
                script_public_key: ScriptPublicKey {
                    version: script_pubkey.version as u16,
                    script,
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Convert subnetwork_id (from 40-char hex string to Bytes32)
    let subnetwork_id = {
        let subnetwork_bytes = hex::decode(&rpc_tx.subnetwork_id)
            .map_err(|e| Error::RpcCall(format!("Failed to decode subnetwork_id: {}", e)))?;

        if subnetwork_bytes.len() != 20 {
            return Err(Error::RpcCall(format!(
                "Invalid subnetwork_id length: {} (expected 20)",
                subnetwork_bytes.len()
            )));
        }

        let mut bytes32 = [0u8; 32];
        bytes32[0..20].copy_from_slice(&subnetwork_bytes);
        Bytes32::from_byte_array(bytes32)
    };

    // Convert payload (from hex string to bytes)
    let payload = if rpc_tx.payload.is_empty() {
        Vec::new()
    } else {
        hex::decode(&rpc_tx.payload)
            .map_err(|e| Error::RpcCall(format!("Failed to decode payload: {}", e)))?
    };

    Ok(TondiTransaction {
        version: rpc_tx.version as u16,
        inputs,
        outputs,
        lock_time: rpc_tx.lock_time,
        subnetwork_id,
        gas: rpc_tx.gas,
        payload,
        mass: rpc_tx.mass,
    })
}

#[cfg(feature = "grpc")]
fn proto_transaction_status_to_tx_status(status: proto::RpcTransactionStatus) -> Result<TxStatus> {
    let status_kind = proto::RpcTransactionStatusKind::try_from(status.status)
        .unwrap_or(proto::RpcTransactionStatusKind::Unknown);
    let accepted = matches!(status_kind, proto::RpcTransactionStatusKind::Accepted);
    let removed = matches!(status_kind, proto::RpcTransactionStatusKind::Removed);
    let accepting_block_hash = status
        .accepting_block_hash
        .as_deref()
        .map(|hash| {
            hash.trim_start_matches("0x")
                .parse::<TondiBlockHash>()
                .map_err(|err| {
                    Error::RpcCall(format!(
                        "Failed to parse accepting block hash '{hash}': {err}"
                    ))
                })
        })
        .transpose()?;

    if accepted && (accepting_block_hash.is_none() || status.accepting_block_daa_score.is_none()) {
        return Err(Error::InvalidServerResponse);
    }

    Ok(TxStatus {
        accepted,
        in_mempool: status.in_mempool
            || status.in_orphan_pool
            || matches!(status_kind, proto::RpcTransactionStatusKind::Mempool),
        removed,
        accepting_block_hash,
        accepting_block_daa_score: status.accepting_block_daa_score,
        block_time: None,
    })
}

#[cfg(feature = "grpc")]
fn proto_fee_bucket_to_fee_bucket(bucket: proto::RpcFeerateBucket) -> FeeBucket {
    FeeBucket {
        feerate: bucket.feerate,
        estimated_seconds: bucket.estimated_seconds,
    }
}

fn append_virtual_chain_page(accumulated: &mut VirtualChain, page: VirtualChain) {
    accumulated
        .removed_chain_block_hashes
        .extend(page.removed_chain_block_hashes);
    accumulated
        .added_chain_block_hashes
        .extend(page.added_chain_block_hashes);
    accumulated
        .accepted_transaction_ids
        .extend(page.accepted_transaction_ids);
}

fn is_virtual_chain_cursor_unavailable(err: &Error) -> bool {
    let message = match err {
        Error::RpcCall(message) | Error::BlockNotFound(message) => message.to_ascii_lowercase(),
        _ => return false,
    };

    message.contains("retention root")
        || message.contains("pruning")
        || message.contains("block not found")
        || message.contains("header not found")
        || message.contains("missingdata")
        || message.contains("missing data")
        || message.contains("queried hash")
}

#[cfg(feature = "grpc")]
fn proto_fee_estimate_to_fee_estimate(estimate: proto::RpcFeeEstimate) -> Result<FeeEstimate> {
    Ok(FeeEstimate {
        priority_bucket: proto_fee_bucket_to_fee_bucket(
            estimate
                .priority_bucket
                .ok_or(Error::InvalidServerResponse)?,
        ),
        normal_buckets: estimate
            .normal_buckets
            .into_iter()
            .map(proto_fee_bucket_to_fee_bucket)
            .collect(),
        low_buckets: estimate
            .low_buckets
            .into_iter()
            .map(proto_fee_bucket_to_fee_bucket)
            .collect(),
    })
}

/// Configuration for Tondi RPC client
#[derive(Debug, Clone)]
pub struct TondiConfig {
    /// RPC endpoint URL (e.g., "38.190.226.7:16210" for gRPC)
    /// Note: gRPC typically uses port 16210
    pub endpoint: String,
    /// Connection timeout
    pub timeout: Option<Duration>,
    /// Network type
    pub network: NetworkType,
    /// Maximum retry attempts
    pub max_retries: usize,
}

impl TondiConfig {
    /// Create a new configuration with the given endpoint
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            timeout: Some(Duration::from_secs(30)),
            network: NetworkType::default(),
            max_retries: 3,
        }
    }

    /// Set the network type
    pub fn with_network(mut self, network: NetworkType) -> Self {
        self.network = network;
        self
    }

    /// Set the connection timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set max retry attempts
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Create configuration for mainnet
    pub fn mainnet(endpoint: impl Into<String>) -> Self {
        Self::new(endpoint).with_network(NetworkType::Mainnet)
    }

    /// Create configuration for testnet
    pub fn testnet(endpoint: impl Into<String>) -> Self {
        Self::new(endpoint).with_network(NetworkType::Testnet)
    }
}

/// Tondi RPC Client
///
/// Provides methods to interact with a Tondi node via RPC.
/// This client supports the full Tondi RPC API and provides
/// compatibility types for RGB/BP integration.
#[derive(Debug)]
pub struct TondiClient {
    config: TondiConfig,
    #[cfg(feature = "grpc")]
    inner: Option<proto::rpc_client::RpcClient<tonic::transport::Channel>>,
    #[cfg(not(feature = "grpc"))]
    _phantom: std::marker::PhantomData<()>,
}

impl TondiClient {
    #[cfg(all(feature = "async", feature = "grpc"))]
    fn rpc_error_to_error(context: &str, message: &str, txid: TondiTxid) -> Error {
        let lower = message.to_ascii_lowercase();
        if lower.contains("not found") || lower.contains("missing") || lower.contains("unknown") {
            Error::TransactionNotFound(bp::Txid::from_byte_array(txid.to_byte_array()))
        } else {
            Error::RpcCall(format!("{context} failed: {message}"))
        }
    }

    /// Connect to a Tondi node
    #[cfg(feature = "async")]
    pub async fn connect(config: TondiConfig) -> Result<Self> {
        #[cfg(feature = "grpc")]
        {
            let endpoint_url = if config.endpoint.starts_with("http://")
                || config.endpoint.starts_with("https://")
            {
                config.endpoint.clone()
            } else {
                format!("http://{}", config.endpoint)
            };

            let mut endpoint = tonic::transport::Endpoint::from_shared(endpoint_url)
                .map_err(|e| Error::Connection(format!("Invalid endpoint URL: {}", e)))?;

            if let Some(timeout) = config.timeout {
                endpoint = endpoint.timeout(timeout);
            } else {
                endpoint = endpoint.timeout(Duration::from_secs(30));
            }

            let channel = endpoint.connect_lazy();

            let inner = proto::rpc_client::RpcClient::new(channel);

            Ok(Self {
                config,
                inner: Some(inner),
            })
        }

        #[cfg(not(feature = "grpc"))]
        {
            Ok(Self {
                config,
                _phantom: std::marker::PhantomData,
            })
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &TondiConfig {
        &self.config
    }

    /// Check connection status
    #[cfg(feature = "async")]
    pub async fn ping(&self) -> Result<()> {
        #[cfg(feature = "grpc")]
        {
            self.get_server_info().await?;
            Ok(())
        }

        #[cfg(not(feature = "grpc"))]
        {
            Ok(())
        }
    }

    /// Get server information
    ///
    /// This method uses the MessageStream bidirectional stream interface.
    #[cfg(feature = "async")]
    pub async fn get_server_info(&self) -> Result<ServerInfo> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let get_info_request = proto::GetInfoRequestMessage {};

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetInfoRequest(
                    get_info_request,
                )),
            };

            let response = client.message_stream(tonic::Request::new(tokio_stream::once(tondid_request))).await.map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetInfoResponse(info_response)) =
                    tondid_response.payload
                {
                    return Ok(ServerInfo::from(info_response));
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetInfo request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get sync status
    #[cfg(feature = "async")]
    pub async fn get_sync_status(&self) -> Result<bool> {
        let info = self.get_server_info().await?;
        Ok(info.is_synced)
    }

    /// Get current network type
    #[cfg(feature = "async")]
    pub async fn get_current_network(&self) -> Result<NetworkType> {
        Ok(self.config.network)
    }

    // ========== Transaction Operations ==========

    /// Submit a transaction to the mempool
    ///
    /// This method uses the MessageStream bidirectional stream interface as defined in the proto.
    #[cfg(feature = "async")]
    pub async fn submit_transaction(
        &self,
        transaction: TondiTransaction,
        allow_orphan: bool,
    ) -> Result<TondiTxid> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let rpc_tx = tondi_tx_to_proto(&transaction)?;

            let submit_request = proto::SubmitTransactionRequestMessage {
                transaction: Some(rpc_tx),
                allow_orphan,
            };

            let request_id = next_request_id()?;

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::SubmitTransactionRequest(
                    submit_request,
                )),
            };

            // Keep the bidirectional stream open until the node returns a response.
            // Closing immediately after sending can race with server-side validation and mempool insertion.
            let (request_tx, request_rx) = tokio::sync::mpsc::channel(1);
            request_tx.send(tondid_request).await.map_err(|_| {
                Error::RpcCall(format!(
                    "Failed to queue SubmitTransaction request ID {}",
                    request_id
                ))
            })?;
            let mut request_tx_opt = Some(request_tx);

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(request_rx)))
                .await
                .map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            let timeout_duration = self.config.timeout.unwrap_or(Duration::from_secs(30));
            let start_time = std::time::Instant::now();

            loop {
                if start_time.elapsed() > timeout_duration {
                    request_tx_opt.take();
                    return Err(Error::RpcCall(format!(
                        "Request timeout after {:?} for request ID {}",
                        timeout_duration, request_id
                    )));
                }

                let tondid_response = match tokio::time::timeout(
                    Duration::from_secs(2),
                    response_stream.next(),
                )
                .await
                {
                    Ok(Some(Ok(resp))) => resp,
                    Ok(Some(Err(e))) => {
                        request_tx_opt.take();
                        return Err(Error::RpcCall(format!("Stream error from node: {}", e)));
                    }
                    Ok(None) => {
                        return Err(Error::RpcCall(format!(
                            "Stream ended without response for request ID {}",
                            request_id
                        )));
                    }
                    Err(_) => {
                        continue;
                    }
                };

                if tondid_response.id != request_id {
                    continue;
                }

                // Drop the sender after receiving the matching response.
                request_tx_opt.take();

                if let Some(proto::tondid_response::Payload::SubmitTransactionResponse(
                    submit_response,
                )) = tondid_response.payload
                {
                    if let Some(err) = submit_response.error {
                        return Err(Error::RpcCall(format!(
                            "Transaction rejected: {}",
                            err.message
                        )));
                    }

                    let txid_str = &submit_response.transaction_id;

                    let txid = txid_str
                        .trim_start_matches("0x")
                        .parse::<TondiTxid>()
                        .map_err(|e| {
                            Error::RpcCall(format!(
                                "Failed to parse transaction ID '{}': {}",
                                txid_str, e
                            ))
                        })?;

                    return Ok(txid);
                }
            }
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = (&transaction, allow_orphan);
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get transaction by ID
    #[cfg(feature = "async")]
    pub async fn get_transaction(&self, txid: TondiTxid) -> Result<TondiTransaction> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetTransactionRequest(
                    proto::GetTransactionRequestMessage {
                        hash: txid.to_hex(),
                    },
                )),
            };

            // Keep the bidirectional stream open while waiting for the matching response.
            let (request_tx, request_rx) = tokio::sync::mpsc::channel(1);
            request_tx.send(tondid_request).await.map_err(|_| {
                Error::RpcCall(format!(
                    "Failed to queue GetTransaction request ID {}",
                    request_id
                ))
            })?;

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(request_rx)))
                .await
                .map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            let timeout_duration = self.config.timeout.unwrap_or(Duration::from_secs(30));
            let start_time = std::time::Instant::now();

            loop {
                if start_time.elapsed() > timeout_duration {
                    return Err(Error::RpcCall(format!(
                        "Request timeout after {:?} for request ID {}",
                        timeout_duration, request_id
                    )));
                }

                let tondid_response = match tokio::time::timeout(
                    Duration::from_secs(1),
                    response_stream.next(),
                )
                .await
                {
                    Ok(Some(Ok(resp))) => resp,
                    Ok(Some(Err(e))) => {
                        return Err(Error::RpcCall(format!("Stream error: {}", e)));
                    }
                    Ok(None) => {
                        return Err(Error::TransactionNotFound(bp::Txid::from_byte_array(
                            txid.to_byte_array(),
                        )));
                    }
                    Err(_) => {
                        continue;
                    }
                };

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetTransactionResponse(resp)) =
                    tondid_response.payload
                {
                    if let Some(err) = resp.error {
                        return Err(Self::rpc_error_to_error(
                            "GetTransaction",
                            &err.message,
                            txid,
                        ));
                    }

                    if let Some(rpc_tx) = resp.transaction {
                        return proto_to_tondi_tx(&rpc_tx);
                    } else {
                        return Err(Error::TransactionNotFound(bp::Txid::from_byte_array(
                            txid.to_byte_array(),
                        )));
                    }
                }
            }
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = txid;
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get transaction status
    #[cfg(feature = "async")]
    pub async fn get_transaction_status(&self, txid: TondiTxid) -> Result<TxStatus> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetTransactionStatusRequest(
                    proto::GetTransactionStatusRequestMessage {
                        transaction_id: txid.to_hex(),
                    },
                )),
            };

            let (request_tx, request_rx) = tokio::sync::mpsc::channel(1);
            request_tx.send(tondid_request).await.map_err(|_| {
                Error::RpcCall(format!(
                    "Failed to queue GetTransactionStatus request ID {}",
                    request_id
                ))
            })?;

            let response = client
                .message_stream(tonic::Request::new(
                    tokio_stream::wrappers::ReceiverStream::new(request_rx),
                ))
                .await
                .map_err(|e| {
                    Error::RpcCall(format!(
                        "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                        e, self.config.endpoint
                    ))
                })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            let timeout_duration = self.config.timeout.unwrap_or(Duration::from_secs(30));
            let start_time = std::time::Instant::now();

            loop {
                if start_time.elapsed() > timeout_duration {
                    return Err(Error::RpcCall(format!(
                        "Request timeout after {:?} for request ID {}",
                        timeout_duration, request_id
                    )));
                }

                let tondid_response = match tokio::time::timeout(
                    Duration::from_secs(1),
                    response_stream.next(),
                )
                .await
                {
                    Ok(Some(Ok(resp))) => resp,
                    Ok(Some(Err(e))) => {
                        return Err(Error::RpcCall(format!("Stream error: {}", e)));
                    }
                    Ok(None) => {
                        return Err(Error::RpcCall(format!(
                            "No response received for GetTransactionStatus request ID {}",
                            request_id
                        )));
                    }
                    Err(_) => {
                        continue;
                    }
                };

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetTransactionStatusResponse(
                    response,
                )) = tondid_response.payload
                {
                    if let Some(err) = response.error {
                        return Err(Self::rpc_error_to_error(
                            "GetTransactionStatus",
                            &err.message,
                            txid,
                        ));
                    }

                    let status = response.status.ok_or(Error::InvalidServerResponse)?;
                    return proto_transaction_status_to_tx_status(status);
                }
            }
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = txid;
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get mempool entry
    #[cfg(feature = "async")]
    pub async fn get_mempool_entry(&self, txid: TondiTxid) -> Result<MempoolEntry> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let request = proto::GetMempoolEntryRequestMessage {
                tx_id: txid.to_hex(),
                include_orphan_pool: true,
                filter_transaction_pool: true,
            };

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetMempoolEntryRequest(
                    request,
                )),
            };

            let response = client.message_stream(tonic::Request::new(tokio_stream::once(tondid_request))).await.map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetMempoolEntryResponse(response)) =
                    tondid_response.payload
                {
                    if let Some(err) = response.error {
                        return Err(Self::rpc_error_to_error(
                            "GetMempoolEntry",
                            &err.message,
                            txid,
                        ));
                    }

                    return response
                        .entry
                        .ok_or(Error::TransactionNotFound(bp::Txid::from_byte_array(
                            txid.to_byte_array(),
                        )))?
                        .try_into();
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetMempoolEntry request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = txid;
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get all mempool entries
    #[cfg(feature = "async")]
    pub async fn get_mempool_entries(&self) -> Result<Vec<MempoolEntry>> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let request = proto::GetMempoolEntriesRequestMessage {
                include_orphan_pool: true,
                filter_transaction_pool: true,
            };

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetMempoolEntriesRequest(
                    request,
                )),
            };

            let response = client.message_stream(tonic::Request::new(tokio_stream::once(tondid_request))).await.map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetMempoolEntriesResponse(response)) =
                    tondid_response.payload
                {
                    if let Some(err) = response.error {
                        return Err(Error::RpcCall(format!(
                            "GetMempoolEntries failed: {}",
                            err.message
                        )));
                    }

                    return response
                        .entries
                        .into_iter()
                        .map(MempoolEntry::try_from)
                        .collect();
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetMempoolEntries request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    // ========== Block Operations ==========

    /// Get block by hash
    #[cfg(feature = "async")]
    pub async fn get_block(
        &self,
        hash: TondiBlockHash,
        include_transactions: bool,
    ) -> Result<TondiBlock> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let request = proto::GetBlockRequestMessage {
                hash: hash.to_hex(),
                include_transactions,
            };

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetBlockRequest(request)),
            };

            let response = client.message_stream(tonic::Request::new(tokio_stream::once(tondid_request))).await.map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetBlockResponse(response)) =
                    tondid_response.payload
                {
                    if let Some(err) = response.error {
                        return Err(Error::RpcCall(format!("GetBlock failed: {}", err.message)));
                    }

                    return response
                        .block
                        .ok_or_else(|| Error::BlockNotFound(hash.to_hex()))?
                        .try_into();
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetBlock request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = (hash, include_transactions);
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get virtual selected-chain changes from a known block to the current virtual chain.
    #[cfg(feature = "async")]
    pub async fn get_virtual_chain_from_block(
        &self,
        start_hash: TondiBlockHash,
        include_accepted_transaction_ids: bool,
        min_confirmation_count: Option<u64>,
    ) -> Result<VirtualChain> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let request = proto::GetVirtualChainFromBlockRequestMessage {
                start_hash: start_hash.to_hex(),
                include_accepted_transaction_ids,
                min_confirmation_count,
            };

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(
                    proto::tondid_request::Payload::GetVirtualChainFromBlockRequest(request),
                ),
            };

            let response = client.message_stream(tonic::Request::new(tokio_stream::once(tondid_request))).await.map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetVirtualChainFromBlockResponse(
                    response,
                )) = tondid_response.payload
                {
                    if let Some(err) = response.error {
                        return Err(Error::RpcCall(format!(
                            "GetVirtualChainFromBlock failed: {}",
                            err.message
                        )));
                    }

                    return Ok(VirtualChain::from(response));
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetVirtualChainFromBlock request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = (
                start_hash,
                include_accepted_transaction_ids,
                min_confirmation_count,
            );
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    #[cfg(feature = "async")]
    async fn collect_virtual_chain_pages(
        &self,
        start_hash: TondiBlockHash,
        target_sink: TondiBlockHash,
        include_accepted_transaction_ids: bool,
    ) -> Result<(VirtualChain, TondiBlockHash, u64)> {
        const MAX_REPLAY_PAGES: u64 = 10_000;

        if start_hash == target_sink {
            return Ok((VirtualChain::default(), target_sink, 0));
        }

        let mut cursor = start_hash;
        let mut chain = VirtualChain::default();
        let mut page_count = 0;

        loop {
            page_count += 1;
            if page_count > MAX_REPLAY_PAGES {
                return Err(Error::RpcCall(format!(
                    "virtual-chain replay exceeded {MAX_REPLAY_PAGES} pages from checkpoint {}",
                    start_hash
                )));
            }

            let page = self
                .get_virtual_chain_from_block(cursor, include_accepted_transaction_ids, None)
                .await?;
            let last_added = page.added_chain_block_hashes.last().cloned();
            let has_removed = !page.removed_chain_block_hashes.is_empty();
            append_virtual_chain_page(&mut chain, page);

            match last_added {
                Some(next_cursor) => {
                    if next_cursor == cursor {
                        return Err(Error::InvalidServerResponse);
                    }
                    cursor = next_cursor;
                    if cursor == target_sink {
                        return Ok((chain, cursor, page_count));
                    }
                }
                None if has_removed => return Ok((chain, target_sink, page_count)),
                None => return Ok((chain, cursor, page_count)),
            }
        }
    }

    /// Replay virtual selected-chain changes from a persisted wallet checkpoint.
    ///
    /// This paginates `GetVirtualChainFromBlock` until the DAG sink observed at
    /// the start of the replay is reached. If the checkpoint has fallen outside
    /// the node's retention window, the replay restarts from the pruning point
    /// and marks the result as requiring a full local wallet/RGB rescan.
    #[cfg(feature = "async")]
    pub async fn replay_virtual_chain_from_checkpoint(
        &self,
        checkpoint: TondiBlockHash,
        include_accepted_transaction_ids: bool,
    ) -> Result<VirtualChainReplay> {
        let dag_info = self.get_block_dag_info().await?;

        match self
            .collect_virtual_chain_pages(
                checkpoint,
                dag_info.sink,
                include_accepted_transaction_ids,
            )
            .await
        {
            Ok((chain, next_checkpoint, page_count)) => Ok(VirtualChainReplay {
                requested_checkpoint: checkpoint,
                replay_start_hash: checkpoint,
                next_checkpoint,
                dag_info,
                chain,
                page_count,
                mode: VirtualChainReplayMode::Checkpoint,
            }),
            Err(err)
                if checkpoint != dag_info.pruning_point_hash
                    && is_virtual_chain_cursor_unavailable(&err) =>
            {
                let (chain, next_checkpoint, page_count) = self
                    .collect_virtual_chain_pages(
                        dag_info.pruning_point_hash,
                        dag_info.sink,
                        include_accepted_transaction_ids,
                    )
                    .await?;
                Ok(VirtualChainReplay {
                    requested_checkpoint: checkpoint,
                    replay_start_hash: dag_info.pruning_point_hash,
                    next_checkpoint,
                    dag_info,
                    chain,
                    page_count,
                    mode: VirtualChainReplayMode::PruningPointFallback,
                })
            }
            Err(err) => Err(err),
        }
    }

    /// Get current DAG sink (virtual selected parent)
    #[cfg(feature = "async")]
    pub async fn get_sink(&self) -> Result<TondiBlockHash> {
        Ok(self.get_block_dag_info().await?.sink)
    }

    /// Get DAG information
    ///
    /// This method uses the MessageStream bidirectional stream interface.
    #[cfg(feature = "async")]
    pub async fn get_block_dag_info(&self) -> Result<DagInfo> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let get_block_dag_info_request = proto::GetBlockDagInfoRequestMessage {};

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetBlockDagInfoRequest(
                    get_block_dag_info_request,
                )),
            };

            let response = client.message_stream(tonic::Request::new(tokio_stream::once(tondid_request))).await.map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetBlockDagInfoResponse(
                    dag_info_response,
                )) = tondid_response.payload
                {
                    return Ok(DagInfo::from(dag_info_response));
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetBlockDagInfo request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get block count
    #[cfg(feature = "async")]
    pub async fn get_block_count(&self) -> Result<u64> {
        let info = self.get_block_dag_info().await?;
        Ok(info.block_count)
    }

    // ========== Address/UTXO Operations ==========

    /// Get balance for an address
    ///
    /// Note: Requires the node to be started with `--utxoindex`
    #[cfg(feature = "async")]
    pub async fn get_balance_by_address(&self, address: &str) -> Result<u64> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;
            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetBalanceByAddressRequest(
                    proto::GetBalanceByAddressRequestMessage {
                        address: address.to_string(),
                    },
                )),
            };

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::once(tondid_request)))
                .await
                .map_err(|e| {
                    Error::RpcCall(format!(
                        "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                        e, self.config.endpoint
                    ))
                })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetBalanceByAddressResponse(
                    balance_response,
                )) = tondid_response.payload
                {
                    if let Some(err) = balance_response.error {
                        return Err(Error::RpcCall(format!(
                            "GetBalanceByAddress failed: {}",
                            err.message
                        )));
                    }
                    return Ok(balance_response.balance);
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetBalanceByAddress request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = address;
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get balances for multiple addresses
    ///
    /// Note: Requires the node to be started with `--utxoindex`
    #[cfg(feature = "async")]
    pub async fn get_balances_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<BalanceByAddressEntry>> {
        #[cfg(feature = "grpc")]
        {
            if addresses.is_empty() {
                return Ok(vec![]);
            }

            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;
            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(
                    proto::tondid_request::Payload::GetBalancesByAddressesRequest(
                        proto::GetBalancesByAddressesRequestMessage {
                            addresses: addresses.to_vec(),
                        },
                    ),
                ),
            };

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::once(tondid_request)))
                .await
                .map_err(|e| {
                    Error::RpcCall(format!(
                        "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                        e, self.config.endpoint
                    ))
                })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetBalancesByAddressesResponse(
                    balances_response,
                )) = tondid_response.payload
                {
                    if let Some(err) = balances_response.error {
                        return Err(Error::RpcCall(format!(
                            "GetBalancesByAddresses failed: {}",
                            err.message
                        )));
                    }
                    let entries = balances_response
                        .entries
                        .into_iter()
                        .map(BalanceByAddressEntry::from)
                        .collect();
                    return Ok(entries);
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetBalancesByAddresses request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = addresses;
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    #[cfg(all(feature = "async", feature = "grpc"))]
    async fn get_utxos_by_address_page(
        &self,
        address: &str,
        start: u64,
        limit: u32,
    ) -> Result<(Vec<UtxosByAddressEntry>, u64)> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;

            let get_utxos_request = proto::GetUtxosByAddressRequestMessage {
                address: address.to_string(),
                start,
                limit,
            };

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetUtxosByAddressRequest(
                    get_utxos_request,
                )),
            };

            let (request_tx, request_rx) = tokio::sync::mpsc::channel(1);
            request_tx.send(tondid_request).await.map_err(|_| {
                Error::RpcCall(format!(
                    "Failed to queue GetUtxosByAddress request ID {}",
                    request_id
                ))
            })?;
            let mut request_tx_opt = Some(request_tx);

            let response = client.message_stream(tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(request_rx))).await.map_err(|e| {
                Error::RpcCall(format!(
                    "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                    e, self.config.endpoint
                ))
            })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetUtxosByAddressResponse(
                    utxos_response,
                )) = tondid_response.payload
                {
                    request_tx_opt.take();
                    // Check for errors first
                    if let Some(err) = utxos_response.error {
                        return Err(Error::RpcCall(format!(
                            "GetUtxosByAddress failed: {}",
                            err.message
                        )));
                    }

                    // Convert proto entries to UtxosByAddressEntry
                    let entries: Vec<UtxosByAddressEntry> = utxos_response
                        .entries
                        .into_iter()
                        .map(UtxosByAddressEntry::from)
                        .collect();

                    return Ok((entries, utxos_response.total));
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetUtxosByAddress request ID {}",
                request_id
            )))
        }
    }

    /// Get UTXOs for an address
    ///
    /// Note: Requires the node to be started with `--utxoindex`
    #[cfg(feature = "async")]
    pub async fn get_utxos_by_address(&self, address: &str) -> Result<Vec<UtxosByAddressEntry>> {
        #[cfg(feature = "grpc")]
        {
            const PAGE_LIMIT: u32 = 1_000;
            let mut start = 0;
            let mut all_entries = Vec::new();

            loop {
                let (mut entries, total) = self
                    .get_utxos_by_address_page(address, start, PAGE_LIMIT)
                    .await?;
                let page_len = entries.len() as u64;
                all_entries.append(&mut entries);

                if page_len == 0
                    || all_entries.len() as u64 >= total
                    || page_len < PAGE_LIMIT as u64
                {
                    break;
                }

                start = start.saturating_add(page_len);
            }

            Ok(all_entries)
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = address;
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Get UTXOs for multiple addresses
    ///
    /// Note: Requires the node to be started with `--utxoindex`
    #[cfg(feature = "async")]
    pub async fn get_utxos_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<UtxosByAddressEntry>> {
        #[cfg(feature = "grpc")]
        {
            if addresses.is_empty() {
                return Ok(vec![]);
            }

            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;
            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetUtxosByAddressesRequest(
                    proto::GetUtxosByAddressesRequestMessage {
                        addresses: addresses.to_vec(),
                    },
                )),
            };

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::once(tondid_request)))
                .await
                .map_err(|e| {
                    Error::RpcCall(format!(
                        "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                        e, self.config.endpoint
                    ))
                })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetUtxosByAddressesResponse(
                    utxos_response,
                )) = tondid_response.payload
                {
                    if let Some(err) = utxos_response.error {
                        return Err(Error::RpcCall(format!(
                            "GetUtxosByAddresses failed: {}",
                            err.message
                        )));
                    }

                    let entries = utxos_response
                        .entries
                        .into_iter()
                        .map(UtxosByAddressEntry::from)
                        .collect();
                    return Ok(entries);
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetUtxosByAddresses request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = addresses;
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    // ========== Fee Estimation ==========

    /// Get fee estimate
    #[cfg(feature = "async")]
    pub async fn get_fee_estimate(&self) -> Result<FeeEstimate> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;
            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetFeeEstimateRequest(
                    proto::GetFeeEstimateRequestMessage {},
                )),
            };

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::once(tondid_request)))
                .await
                .map_err(|e| {
                    Error::RpcCall(format!(
                        "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                        e, self.config.endpoint
                    ))
                })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetFeeEstimateResponse(fee_response)) =
                    tondid_response.payload
                {
                    if let Some(err) = fee_response.error {
                        return Err(Error::RpcCall(format!(
                            "GetFeeEstimate failed: {}",
                            err.message
                        )));
                    }

                    let estimate = fee_response.estimate.ok_or(Error::InvalidServerResponse)?;
                    return proto_fee_estimate_to_fee_estimate(estimate);
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetFeeEstimate request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    /// Convert fee rate to the target confirmation time
    pub fn convert_fee_rate(target_seconds: u64, estimate: &FeeEstimate) -> Option<f64> {
        // Priority bucket is for fastest confirmation
        if estimate.priority_bucket.estimated_seconds <= target_seconds as f64 {
            return Some(estimate.priority_bucket.feerate);
        }

        // Check normal buckets
        for bucket in &estimate.normal_buckets {
            if bucket.estimated_seconds <= target_seconds as f64 {
                return Some(bucket.feerate);
            }
        }

        // Check low buckets
        for bucket in &estimate.low_buckets {
            if bucket.estimated_seconds <= target_seconds as f64 {
                return Some(bucket.feerate);
            }
        }

        None
    }

    // ========== Coin Supply ==========

    /// Get current coin supply
    #[cfg(feature = "async")]
    pub async fn get_coin_supply(&self) -> Result<(u64, u64)> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;
            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetCoinSupplyRequest(
                    proto::GetCoinSupplyRequestMessage {},
                )),
            };

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::once(tondid_request)))
                .await
                .map_err(|e| {
                    Error::RpcCall(format!(
                        "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                        e, self.config.endpoint
                    ))
                })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(proto::tondid_response::Payload::GetCoinSupplyResponse(
                    supply_response,
                )) = tondid_response.payload
                {
                    if let Some(err) = supply_response.error {
                        return Err(Error::RpcCall(format!(
                            "GetCoinSupply failed: {}",
                            err.message
                        )));
                    }

                    return Ok((supply_response.circulating_sau, supply_response.max_sau));
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetCoinSupply request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }

    // ========== Network Hash Rate ==========

    /// Estimate network hash rate
    #[cfg(feature = "async")]
    pub async fn estimate_network_hashes_per_second(
        &self,
        window_size: u32,
        start_hash: Option<TondiBlockHash>,
    ) -> Result<u64> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| {
                    Error::Connection("Client not connected. Call connect() first.".into())
                })?
                .clone();

            let request_id = next_request_id()?;
            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(
                    proto::tondid_request::Payload::EstimateNetworkHashesPerSecondRequest(
                        proto::EstimateNetworkHashesPerSecondRequestMessage {
                            window_size,
                            start_hash: start_hash.map(|hash| hash.to_hex()).unwrap_or_default(),
                        },
                    ),
                ),
            };

            let response = client
                .message_stream(tonic::Request::new(tokio_stream::once(tondid_request)))
                .await
                .map_err(|e| {
                    Error::RpcCall(format!(
                        "Failed to establish message stream: {}. Make sure the node is accessible at {}",
                        e, self.config.endpoint
                    ))
                })?;

            let mut response_stream = response.into_inner();
            use tokio_stream::StreamExt;

            while let Some(response_result) = response_stream.next().await {
                let tondid_response = response_result.map_err(|e| {
                    Error::RpcCall(format!("Failed to receive response from stream: {}", e))
                })?;

                if tondid_response.id != request_id {
                    continue;
                }

                if let Some(
                    proto::tondid_response::Payload::EstimateNetworkHashesPerSecondResponse(
                        hashes_response,
                    ),
                ) = tondid_response.payload
                {
                    if let Some(err) = hashes_response.error {
                        return Err(Error::RpcCall(format!(
                            "EstimateNetworkHashesPerSecond failed: {}",
                            err.message
                        )));
                    }

                    return Ok(hashes_response.network_hashes_per_second);
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for EstimateNetworkHashesPerSecond request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            let _ = (window_size, start_hash);
            Err(Error::Connection(
                "gRPC feature not enabled. Enable it with 'grpc' feature flag.".into(),
            ))
        }
    }
}

/// Builder for creating TondiClient with custom options
#[derive(Debug, Clone)]
pub struct TondiClientBuilder {
    config: TondiConfig,
    headers: HashMap<String, String>,
}

impl TondiClientBuilder {
    /// Create a new builder with the given endpoint
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            config: TondiConfig::new(endpoint),
            headers: HashMap::new(),
        }
    }

    /// Set the network type
    pub fn network(mut self, network: NetworkType) -> Self {
        self.config.network = network;
        self
    }

    /// Set connection timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = Some(timeout);
        self
    }

    /// Set max retry attempts
    pub fn max_retries(mut self, retries: usize) -> Self {
        self.config.max_retries = retries;
        self
    }

    /// Add a custom header
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Build the client
    #[cfg(feature = "async")]
    pub async fn build(self) -> Result<TondiClient> {
        TondiClient::connect(self.config).await
    }
}

#[cfg(test)]
mod tests {
    use amplify::Bytes32;

    use super::*;
    use crate::AcceptedTransactionIds;

    fn hash(byte: u8) -> TondiBlockHash {
        Bytes32::from_byte_array([byte; 32])
    }

    #[test]
    fn virtual_chain_page_append_preserves_reorg_and_acceptance_order() {
        let mut accumulated = VirtualChain {
            removed_chain_block_hashes: vec![hash(1)],
            added_chain_block_hashes: vec![hash(2)],
            accepted_transaction_ids: vec![AcceptedTransactionIds {
                accepting_block_hash: hash(2),
                accepting_block_daa_score: Some(20),
                accepted_transaction_ids: vec![hash(3)],
            }],
        };
        let page = VirtualChain {
            removed_chain_block_hashes: vec![hash(4)],
            added_chain_block_hashes: vec![hash(5), hash(6)],
            accepted_transaction_ids: vec![AcceptedTransactionIds {
                accepting_block_hash: hash(5),
                accepting_block_daa_score: Some(50),
                accepted_transaction_ids: vec![hash(7), hash(8)],
            }],
        };

        append_virtual_chain_page(&mut accumulated, page);

        assert_eq!(
            accumulated.removed_chain_block_hashes,
            vec![hash(1), hash(4)]
        );
        assert_eq!(
            accumulated.added_chain_block_hashes,
            vec![hash(2), hash(5), hash(6)]
        );
        assert_eq!(accumulated.accepted_transaction_ids.len(), 2);
        assert_eq!(
            accumulated.accepted_transaction_ids[1].accepted_transaction_ids,
            vec![hash(7), hash(8)]
        );
    }

    #[test]
    fn virtual_chain_cursor_unavailable_detects_pruning_and_missing_history() {
        assert!(is_virtual_chain_cursor_unavailable(&Error::RpcCall(
            "GetVirtualChainFromBlock failed: the queried hash does not have retention root on its chain"
                .to_string()
        )));
        assert!(is_virtual_chain_cursor_unavailable(&Error::BlockNotFound(
            "block not found: abc".to_string()
        )));
        assert!(!is_virtual_chain_cursor_unavailable(&Error::RpcCall(
            "GetVirtualChainFromBlock failed: node is not synced".to_string()
        )));
    }
}
