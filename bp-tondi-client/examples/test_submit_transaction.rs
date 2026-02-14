//! 测试提交交易到远程 Tondi 节点
//!
//! 运行方式：
//! ```bash
//! cd bp-tondi-client
//! cargo run --example test_submit_transaction --features grpc
//! ```

use tondi::{TondiClient, TondiConfig, TondiTransaction, TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey};
use amplify::{Bytes32, ByteArray, confinement::Confined, hex::ToHex};
use bp::{Tx, TxVer, LockTime, TxIn, TxOut, Outpoint, Vout, SigScript, SeqNo, Witness, ScriptPubkey, Txid, VarIntBytes};
use commit_verify::create_annex_with_commitment;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Tondi 交易提交测试 ===\n");

    // 连接到远程节点
    let endpoint = "38.190.226.7:16210";
    println!("正在连接到远程节点: {}", endpoint);
    
    let config = TondiConfig::new(endpoint)
        .with_timeout(std::time::Duration::from_secs(10));
    
    let client = match TondiClient::connect(config).await {
        Ok(client) => {
            println!("✓ 连接成功！\n");
            client
        }
        Err(e) => {
            eprintln!("✗ 连接失败: {}", e);
            return Err(e.into());
        }
    };

    // 测试 1: 获取服务器信息
    println!("[测试 1] 获取服务器信息...");
    match client.get_server_info().await {
        Ok(info) => {
            println!("✓ 服务器信息:");
            println!("  - 版本: {}", info.server_version);
            println!("  - 是否同步: {}", info.is_synced);
            println!("  - 网络 ID: {}\n", if info.network_id.is_empty() { "N/A" } else { &info.network_id });
        }
        Err(e) => {
            eprintln!("✗ 获取服务器信息失败: {}\n", e);
            return Err(e.into());
        }
    }

    // 测试 2: 使用 bp-core 的 Tx 结构体构造交易（包含 Annex 承诺数据）
    println!("[测试 2] 使用 bp-core Tx 结构体构造交易...");
    
    // 创建一个测试用的 RGB commitment（32 字节）
    let test_commitment = [0xAAu8; 32];
    
    // 创建 Annex 数据（格式：0x50 + 0x20 + 0x20 + 32-byte commitment）
    let annex_data = create_annex_with_commitment(&test_commitment);
    println!("  - RGB Commitment (hex): {}", hex::encode(&test_commitment));
    println!("  - Annex 数据长度: {} 字节", annex_data.len());
    println!("  - Annex 数据 (hex): {}", hex::encode(&annex_data));
    
    // 构造 bp-core 的 Tx 结构体
    // 注意：这是一个示例交易，实际使用时需要有效的 UTXO 和签名
    let bp_tx = Tx {
        version: TxVer::from_consensus_u16(0),
        inputs: Confined::try_from_iter(vec![
            TxIn {
                prev_output: Outpoint::new(
                    Txid::from_byte_array([0u8; 32]), // 占位符，实际需要有效的 txid
                    Vout::from_u32(0),
                ),
                sig_script: SigScript::empty(), // 占位符，实际需要签名脚本
                sequence: SeqNo::from_consensus_u32(0),
                witness: {
                    // 将 Annex 数据放入第一个输入的 witness（最后一个元素）
                    let mut witness_items = Vec::new();
                    witness_items.push(Vec::new()); // 占位符签名
                    witness_items.push(annex_data.clone()); // Annex 数据
                    Witness::from_consensus_stack(witness_items)
                },
            },
        ]).unwrap(),
        outputs: Confined::try_from_iter(vec![
            TxOut {
                value: 1000u64.into(), // 1000 sats
                script_pubkey: ScriptPubkey::from_checked(vec![
                    0x76, 0xa9, 0x14, // OP_DUP OP_HASH160 <20 bytes>
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, // 占位符地址
                    0x88, 0xac, // OP_EQUALVERIFY OP_CHECKSIG
                ]),
            },
        ]).unwrap(),
        lock_time: LockTime::from_consensus_u64(0),
        subnetwork_id: [0u8; 20], // 默认子网络
        gas: 0,
        payload: VarIntBytes::try_from(annex_data.clone()).unwrap(), // Annex 数据（RGB commitment）
        mass: 0, // 节点会自动计算
    };

    println!("  - bp::Tx 构造完成");
    println!("  - 交易版本: {}", bp_tx.version.to_consensus_u16());
    println!("  - 输入数量: {}", bp_tx.inputs.len());
    println!("  - 输出数量: {}", bp_tx.outputs.len());
    
    // 转换为 TondiTransaction
    let tondi_tx = convert_bp_to_tondi_tx(&bp_tx)?;
    println!("  - 转换为 TondiTransaction 完成");
    println!("  - Payload 长度: {} 字节", tondi_tx.payload.len());
    println!("✓ 交易构造完成\n");

    // 测试 3: 提交交易
    println!("[测试 3] 提交交易到节点...");
    println!("⚠️  注意：这是一个测试交易，可能因为缺少有效的 UTXO 或签名而被拒绝");
    
    match client.submit_transaction(tondi_tx, false).await {
        Ok(txid) => {
            println!("✓ 交易提交成功！");
            println!("  - 交易 ID: {}\n", txid.to_hex());
        }
        Err(e) => {
            eprintln!("✗ 交易提交失败: {}", e);
            eprintln!("  这是预期的，因为测试交易缺少有效的 UTXO 和签名\n");
            // 不返回错误，因为这是预期的测试结果
        }
    }

    println!("=== 测试完成 ===");
    Ok(())
}

/// 从 bp::Tx 转换为 TondiTransaction
/// 
/// 这个转换函数提取 Annex 数据（RGB commitment）从 BP Witness 并存储到 Tondi 的 payload 字段
fn convert_bp_to_tondi_tx(tx: &Tx) -> Result<TondiTransaction, Box<dyn std::error::Error>> {
    use commit_verify::extract_commitment_from_annex;
    
    // 从第一个输入的 witness 中提取 Annex 数据（RGB commitment）
    let mut payload = Vec::new();
    if let Some(first_input) = tx.inputs().next() {
        if !first_input.witness.is_empty() {
            // 查找 Annex（通常是 witness 的最后一个元素）
            // Annex 格式：0x50 + TLV 数据
            for witness_item in first_input.witness.iter().rev() {
                if !witness_item.is_empty() && witness_item[0] == 0x50 {
                    // 找到 Annex 数据，验证它是有效的 RGB commitment 格式
                    if extract_commitment_from_annex(witness_item).is_ok() {
                        payload = witness_item.to_vec();
                        break;
                    }
                }
            }
        }
    }
    
    // 转换输入：RGB 只使用 Standard 输入
    let inputs: Vec<TondiTxInput> = tx.inputs.iter().map(|input| {
        let signature_script = input.sig_script.to_vec();
        
        // 简单的 sig_op_count 计算（实际应该解析脚本）
        let sig_op_count = if signature_script.is_empty() {
            0
        } else {
            // 对于 P2PKH，通常是 1 个 sig op
            // 这里简化处理，实际应该解析脚本
            1
        };
        
        TondiTxInput::Standard {
            previous_outpoint: TondiOutpoint {
                transaction_id: Bytes32::from_byte_array(
                    input.prev_output.txid.to_byte_array()
                ),
                index: input.prev_output.vout.to_u32(),
            },
            signature_script,
            sequence: input.sequence.to_consensus_u32() as u64,
            sig_op_count,
        }
    }).collect();
    
    // 转换输出：RGB 只使用 Standard 输出
    let outputs: Vec<TondiTxOutput> = tx.outputs.iter().map(|output| {
        TondiTxOutput::Standard {
            value: output.value.sats(),
            script_public_key: ScriptPublicKey {
                version: 0, // P2PK version
                script: output.script_pubkey.to_vec(),
            },
        }
    }).collect();
    
    Ok(TondiTransaction {
        version: tx.version.to_consensus_u16(),
        inputs,
        outputs,
        lock_time: tx.lock_time.to_consensus_u64(),
        // Tondi RPC 使用 Bytes32 (32 字节)，但我们需要从 BP 的 20 字节转换
        subnetwork_id: {
            let mut bytes32 = [0u8; 32];
            bytes32[0..20].copy_from_slice(&tx.subnetwork_id);
            Bytes32::from_byte_array(bytes32)
        },
        gas: tx.gas,
        payload, // 保留 RGB commitment Annex 数据
        mass: tx.mass,
    })
}

