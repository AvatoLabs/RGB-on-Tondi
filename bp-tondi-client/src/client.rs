//! Tondi RPC Client implementation

use std::collections::HashMap;
use std::time::Duration;

use amplify::{hex::ToHex, ByteArray, Bytes32};
use hex;

use crate::{
    BalanceByAddressEntry, DagInfo, Error, FeeEstimate, MempoolEntry, NetworkType, Result,
    ServerInfo, TondiBlock, TondiBlockHash, TondiTransaction, TondiTxid, TxStatus,
    UtxosByAddressEntry,
};

// gRPC proto 
#[cfg(feature = "grpc")]
pub mod proto {
    tonic::include_proto!("protowire"); 
}

#[cfg(feature = "grpc")]
fn tondi_tx_to_proto(tx: &TondiTransaction) -> proto::RpcTransaction {
    use crate::types::{TondiTxInput, TondiTxOutput};
    
    let rpc_inputs: Vec<proto::RpcTransactionInput> = tx
        .inputs
        .iter()
        .map(|input| {
            match input {
                TondiTxInput::Standard {
                    previous_outpoint,
                    signature_script,
                    sequence,
                    sig_op_count,
                } => {
                    proto::RpcTransactionInput {
                        previous_outpoint: Some(proto::RpcOutpoint {
                            // 移除 0x 前缀（回归 Kaspa 标准）
                            transaction_id: previous_outpoint.transaction_id.to_hex(),
                            index: previous_outpoint.index,
                        }),
                        input: Some(proto::rpc_transaction_input::Input::Standard(
                            proto::RpcStandardInput {
                                // 移除 0x 前缀（回归 Kaspa 标准）
                                signature_script: signature_script.to_hex(),
                                sequence: *sequence,
                                sig_op_count: *sig_op_count as u32,
                            },
                        )),
                        verbose_data: None,
                    }
                }
                _ => {
                    panic!("RGB Purist Mode only supports Standard inputs");
                }
            }
        })
        .collect();

    let rpc_outputs: Vec<proto::RpcTransactionOutput> = tx
        .outputs
        .iter()
        .map(|output| {
            match output {
                TondiTxOutput::Standard {
                    value,
                    script_public_key,
                } => {
                    proto::RpcTransactionOutput {
                        amount: *value,
                        output: Some(proto::rpc_transaction_output::Output::Standard(
                            proto::RpcStandardOutput {
                                script_public_key: Some(proto::RpcScriptPublicKey {
                                    version: script_public_key.version as u32,
                                    // 移除 0x 前缀（回归 Kaspa 标准）
                                    script_public_key: script_public_key.script.to_hex(),
                                }),
                            },
                        )),
                        verbose_data: None,
                    }
                }
                _ => {
                    panic!("RGB Purist Mode only supports Standard outputs");
                }
            }
        })
        .collect();

    // ========================================================================
    // 核心修复：子网 ID 和 Payload 的编码逻辑（回归 Kaspa 标准）
    // ========================================================================
    // 移除所有 0x 前缀：绝大多数基于 Kaspa 的 gRPC 实现（Tondi 是其分支）
    // 在处理 string 类型的十六进制字段时，不使用 0x 前缀
    let subnetwork_id_hex = {
        let bytes32 = tx.subnetwork_id.to_byte_array();
        let subnetwork_id_20: [u8; 20] = bytes32[0..20].try_into()
            .expect("Failed to extract 20 bytes from Bytes32");
        // 始终发送 40 位纯 hex（不带 0x 前缀）
        hex::encode(subnetwork_id_20)
    };

    // 空 Payload 必须是空字符串 ""，非空 Payload 发送纯 hex（不带 0x 前缀）
    let payload_encoded = if tx.payload.is_empty() {
        "".to_string()
    } else {
        tx.payload.to_hex()
    };

    proto::RpcTransaction {
        version: tx.version as u32,
        inputs: rpc_inputs,
        outputs: rpc_outputs,
        lock_time: tx.lock_time,
        subnetwork_id: subnetwork_id_hex,
        gas: tx.gas,
        payload: payload_encoded,
        mass: tx.mass,
        verbose_data: None,
    }
}

/// Convert proto::RpcTransaction back to TondiTransaction
#[cfg(feature = "grpc")]
fn proto_to_tondi_tx(rpc_tx: &proto::RpcTransaction) -> Result<TondiTransaction> {
    use crate::types::{TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey};
    
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
            
            let txid_bytes: [u8; 32] = txid.try_into()
                .map_err(|_| Error::RpcCall("Failed to convert txid to array".into()))?;
            
            match &rpc_input.input {
                Some(proto::rpc_transaction_input::Input::Standard(standard)) => {
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
                }
                _ => {
                    Err(Error::RpcCall("RGB Purist Mode only supports Standard inputs".into()))
                }
            }
        })
        .collect::<Result<Vec<_>>>()?;
    
    // Convert outputs
    let outputs: Vec<TondiTxOutput> = rpc_tx
        .outputs
        .iter()
        .map(|rpc_output| {
            match &rpc_output.output {
                Some(proto::rpc_transaction_output::Output::Standard(standard)) => {
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
                }
                _ => {
                    Err(Error::RpcCall("RGB Purist Mode only supports Standard outputs".into()))
                }
            }
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
    /// Connect to a Tondi node
    #[cfg(feature = "async")]
    pub async fn connect(config: TondiConfig) -> Result<Self> {
        #[cfg(feature = "grpc")]
        {
            let endpoint_url = if config.endpoint.starts_with("http://") || config.endpoint.starts_with("https://") {
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

            let channel = endpoint
                .connect()
                .await
                .map_err(|e| {
                    Error::Connection(format!("Failed to connect to Tondi node at {}: {}. Make sure the node is running and the port is correct (gRPC typically uses 16210).", config.endpoint, e))
                })?;

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
                .ok_or_else(|| Error::Connection("Client not connected. Call connect() first.".into()))?
                .clone();

            use std::time::{SystemTime, UNIX_EPOCH};
            let request_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;

            let get_info_request = proto::GetInfoRequestMessage {};

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetInfoRequest(
                    get_info_request,
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

                if let Some(proto::tondid_response::Payload::GetInfoResponse(
                    info_response,
                )) = tondid_response.payload
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
            Err(Error::Connection("gRPC feature not enabled. Enable it with 'grpc' feature flag.".into()))
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
                .ok_or_else(|| Error::Connection("Client not connected. Call connect() first.".into()))?
                .clone();

            let rpc_tx = tondi_tx_to_proto(&transaction);
            
            println!("\n[调试信息] 最终生成的 RpcTransaction 结构:");
            println!("  - Version: {}", rpc_tx.version);
            println!("  - Inputs 数量: {}", rpc_tx.inputs.len());
            if let Some(first_input) = rpc_tx.inputs.first() {
                if let Some(proto::rpc_transaction_input::Input::Standard(standard)) = &first_input.input {
                    println!("  - 第一个输入的 signature_script 长度: {} 字符 (hex)", standard.signature_script.len());
                    println!("  - 第一个输入的 signature_script (hex): {}", standard.signature_script);
                    println!("  - 第一个输入的 sequence: {}", standard.sequence);
                    println!("  - 第一个输入的 sig_op_count: {}", standard.sig_op_count);
                }
            }
            println!("  - Outputs 数量: {}", rpc_tx.outputs.len());
            println!("  - Lock Time: {}", rpc_tx.lock_time);
            println!("  - Subnetwork ID: {} (长度: {} 字符, 应为 40)", 
                rpc_tx.subnetwork_id, rpc_tx.subnetwork_id.len());
            println!("  - Gas: {}", rpc_tx.gas);
            println!("  - Payload: {} (长度: {} 字符, 空为正常普通转账)", 
                if rpc_tx.payload.len() > 50 { 
                    format!("{}...", &rpc_tx.payload[..50]) 
                } else if rpc_tx.payload.is_empty() {
                    "(空)".to_string()
                } else { 
                    rpc_tx.payload.clone() 
                },
                rpc_tx.payload.len());
            println!("  - Mass: {}", rpc_tx.mass);
            println!();

            let submit_request = proto::SubmitTransactionRequestMessage {
                transaction: Some(rpc_tx),
                allow_orphan,
            };

            use std::time::{SystemTime, UNIX_EPOCH};
            let request_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::SubmitTransactionRequest(
                    submit_request,
                )),
            };

            // ========================================================================
            // 优化：使用 mpsc 通道保持 gRPC 流开启，给节点足够时间处理
            // ========================================================================
            // 为了让节点有足够时间"思考"和回传响应，使用 mpsc 通道替代 tokio_stream::once
            // 这样可以保持流开启，不会立即发出结束信号，给节点更多处理时间
            let (request_tx, request_rx) = tokio::sync::mpsc::channel(1);
            
            // 发送请求到通道（使用 expect 因为通道刚创建，不应该失败）
            request_tx.send(tondid_request).await
                .expect("Failed to send request to channel");

            // 使用 ReceiverStream 包装接收端，保持流开启
            // 这样流就不会立即发出结束信号，给节点足够的时间去处理和回传响应
            let response = client
                .message_stream(tonic::Request::new(
                    tokio_stream::wrappers::ReceiverStream::new(request_rx)
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
            
            // 设置超时时间，避免无限等待
            let timeout_duration = self.config.timeout.unwrap_or(Duration::from_secs(30));
            let start_time = std::time::Instant::now();
            
            // 保持流循环等待响应，给节点足够时间处理
            // 使用循环 + 超时检查，给节点足够时间"思考"和回传
            loop {
                // 检查总超时
                if start_time.elapsed() > timeout_duration {
                    return Err(Error::RpcCall(format!(
                        "Request timeout after {:?} for request ID {}",
                        timeout_duration, request_id
                    )));
                }
                
                // 使用带超时的 next()，每次最多等待1秒，然后检查总超时
                // 这样可以让流保持开启，给节点足够时间处理
                let tondid_response = match tokio::time::timeout(Duration::from_secs(1), response_stream.next()).await {
                    Ok(Some(Ok(resp))) => resp,
                    Ok(Some(Err(e))) => {
                        // 流错误
                        return Err(Error::RpcCall(format!(
                            "Stream closed by node (possibly due to invalid transaction format): {}. \
                            Check transaction fields (e.g., subnetwork_id must be 20 bytes, not 32 bytes).",
                            e
                        )));
                    }
                    Ok(None) => {
                        // 流结束，但没有收到响应
                        return Err(Error::RpcCall(format!(
                            "Stream ended without response for request ID {}",
                            request_id
                        )));
                    }
                    Err(_) => {
                        // 1秒超时，继续循环检查总超时（给节点更多时间）
                        continue;
                    }
                };

                // ========================================================================
                // 关键调试代码：打印节点返回的每一个消息
                // ========================================================================
                // 不要只检查 ID 为 1 的消息，打印所有响应以便调试
                println!("[gRPC 原始响应] ID: {}, Payload: {:?}", tondid_response.id, tondid_response.payload);

                if tondid_response.id != request_id {
                    println!("  - 跳过不匹配的响应 ID (期望: {}, 实际: {})", request_id, tondid_response.id);
                    continue; 
                }

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
            Err(Error::Connection("gRPC feature not enabled. Enable it with 'grpc' feature flag.".into()))
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
                .ok_or_else(|| Error::Connection("Client not connected. Call connect() first.".into()))?
                .clone();
            
            use std::time::{SystemTime, UNIX_EPOCH};
            let request_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            
            // 构造请求
            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetTransactionRequest(
                    proto::GetTransactionRequestMessage {
                        hash: txid.to_hex(),
                    },
                )),
            };
            
            // 使用优化过的 mpsc 异步流逻辑，保持流开启
            let (request_tx, request_rx) = tokio::sync::mpsc::channel(1);
            request_tx.send(tondid_request).await
                .expect("Failed to send request to channel");
            
            let response = client
                .message_stream(tonic::Request::new(
                    tokio_stream::wrappers::ReceiverStream::new(request_rx)
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
            
            // 设置超时时间，避免无限等待
            let timeout_duration = self.config.timeout.unwrap_or(Duration::from_secs(30));
            let start_time = std::time::Instant::now();
            
            // 保持流循环等待响应，给节点足够时间处理
            loop {
                // 检查总超时
                if start_time.elapsed() > timeout_duration {
                    return Err(Error::RpcCall(format!(
                        "Request timeout after {:?} for request ID {}",
                        timeout_duration, request_id
                    )));
                }
                
                // 使用带超时的 next()，每次最多等待1秒，然后检查总超时
                let tondid_response = match tokio::time::timeout(Duration::from_secs(1), response_stream.next()).await {
                    Ok(Some(Ok(resp))) => resp,
                    Ok(Some(Err(e))) => {
                        return Err(Error::RpcCall(format!(
                            "Stream error: {}",
                            e
                        )));
                    }
                    Ok(None) => {
                        return Err(Error::TransactionNotFound(bp::Txid::from_byte_array(
                            txid.to_byte_array(),
                        )));
                    }
                    Err(_) => {
                        // 1秒超时，继续循环检查总超时（给节点更多时间）
                        continue;
                    }
                };
                
                // 检查响应 ID 是否匹配
                if tondid_response.id != request_id {
                    continue;
                }
                
                // 处理 GetTransactionResponse
                if let Some(proto::tondid_response::Payload::GetTransactionResponse(resp)) = tondid_response.payload {
                    if let Some(err) = resp.error {
                        return Err(Error::RpcCall(format!(
                            "GetTransaction failed: {}",
                            err.message
                        )));
                    }
                    
                    if let Some(rpc_tx) = resp.transaction {
                        println!("[调试] 收到查询响应，交易版本号: {}", rpc_tx.version);
                        // 转换 proto::RpcTransaction 到 TondiTransaction
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
            Err(Error::Connection("gRPC feature not enabled. Enable it with 'grpc' feature flag.".into()))
        }
    }

    /// Get transaction status
    #[cfg(feature = "async")]
    pub async fn get_transaction_status(&self, _txid: TondiTxid) -> Result<TxStatus> {
        // TODO: Implement actual RPC call
        Err(Error::Connection("Not implemented".into()))
    }

    /// Get mempool entry
    #[cfg(feature = "async")]
    pub async fn get_mempool_entry(&self, _txid: TondiTxid) -> Result<MempoolEntry> {
        // TODO: Implement actual RPC call
        Err(Error::Connection("Not implemented".into()))
    }

    /// Get all mempool entries
    #[cfg(feature = "async")]
    pub async fn get_mempool_entries(&self) -> Result<Vec<MempoolEntry>> {
        // TODO: Implement actual RPC call
        Err(Error::Connection("Not implemented".into()))
    }

    // ========== Block Operations ==========

    /// Get block by hash
    #[cfg(feature = "async")]
    pub async fn get_block(
        &self,
        _hash: TondiBlockHash,
        _include_transactions: bool,
    ) -> Result<TondiBlock> {
        // TODO: Implement actual RPC call
        Err(Error::Connection("Not implemented".into()))
    }

    /// Get current DAG sink (virtual selected parent)
    #[cfg(feature = "async")]
    pub async fn get_sink(&self) -> Result<TondiBlockHash> {
        // TODO: Implement actual RPC call
        Err(Error::Connection("Not implemented".into()))
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
                .ok_or_else(|| Error::Connection("Client not connected. Call connect() first.".into()))?
                .clone();

            use std::time::{SystemTime, UNIX_EPOCH};
            let request_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;

            let get_block_dag_info_request = proto::GetBlockDagInfoRequestMessage {};

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetBlockDagInfoRequest(
                    get_block_dag_info_request,
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
            Err(Error::Connection("gRPC feature not enabled. Enable it with 'grpc' feature flag.".into()))
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
    pub async fn get_balance_by_address(&self, _address: &str) -> Result<u64> {
        // TODO: Implement actual RPC call
        Err(Error::UtxoIndexNotEnabled)
    }

    /// Get balances for multiple addresses
    ///
    /// Note: Requires the node to be started with `--utxoindex`
    #[cfg(feature = "async")]
    pub async fn get_balances_by_addresses(
        &self,
        _addresses: &[String],
    ) -> Result<Vec<BalanceByAddressEntry>> {
        // TODO: Implement actual RPC call
        Err(Error::UtxoIndexNotEnabled)
    }

    /// Get UTXOs for an address
    ///
    /// Note: Requires the node to be started with `--utxoindex`
    #[cfg(feature = "async")]
    pub async fn get_utxos_by_address(&self, address: &str) -> Result<Vec<UtxosByAddressEntry>> {
        #[cfg(feature = "grpc")]
        {
            let mut client = self
                .inner
                .as_ref()
                .ok_or_else(|| Error::Connection("Client not connected. Call connect() first.".into()))?
                .clone();

            use std::time::{SystemTime, UNIX_EPOCH};
            let request_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;

            let get_utxos_request = proto::GetUtxosByAddressRequestMessage {
                address: address.to_string(),
                start: 0, // Start from the beginning
                limit: 1000, // Limit to 1000 entries (adjust as needed)
            };

            let tondid_request = proto::TondidRequest {
                id: request_id,
                payload: Some(proto::tondid_request::Payload::GetUtxosByAddressRequest(
                    get_utxos_request,
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

                if let Some(proto::tondid_response::Payload::GetUtxosByAddressResponse(
                    utxos_response,
                )) = tondid_response.payload
                {
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
                    
                    return Ok(entries);
                }
            }

            Err(Error::RpcCall(format!(
                "No response received for GetUtxosByAddress request ID {}",
                request_id
            )))
        }

        #[cfg(not(feature = "grpc"))]
        {
            Err(Error::Connection("gRPC feature not enabled. Enable it with 'grpc' feature flag.".into()))
        }
    }

    /// Get UTXOs for multiple addresses
    ///
    /// Note: Requires the node to be started with `--utxoindex`
    #[cfg(feature = "async")]
    pub async fn get_utxos_by_addresses(
        &self,
        _addresses: &[String],
    ) -> Result<Vec<UtxosByAddressEntry>> {
        // TODO: Implement actual RPC call
        Err(Error::UtxoIndexNotEnabled)
    }

    // ========== Fee Estimation ==========

    /// Get fee estimate
    #[cfg(feature = "async")]
    pub async fn get_fee_estimate(&self) -> Result<FeeEstimate> {
        // TODO: Implement actual RPC call
        Err(Error::Connection("Not implemented".into()))
    }

    /// Convert fee rate to the target confirmation time
    pub fn convert_fee_rate(
        target_seconds: u64,
        estimate: &FeeEstimate,
    ) -> Option<f64> {
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
        // TODO: Implement actual RPC call
        // Returns (circulating_supply, max_supply)
        Err(Error::Connection("Not implemented".into()))
    }

    // ========== Network Hash Rate ==========

    /// Estimate network hash rate
    #[cfg(feature = "async")]
    pub async fn estimate_network_hashes_per_second(
        &self,
        _window_size: u32,
        _start_hash: Option<TondiBlockHash>,
    ) -> Result<u64> {
        // TODO: Implement actual RPC call
        Err(Error::Connection("Not implemented".into()))
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

