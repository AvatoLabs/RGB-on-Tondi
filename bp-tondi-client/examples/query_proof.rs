//! 示例：查询交易证明
//! 
//! 验证 GetTransaction RPC 调用是否正常工作
//! 查询一个已知存在的交易（UTXO 的来源交易）
//!
//! 运行：cargo run --example query_proof --features grpc

use tondi::{TondiClient, TondiConfig, TondiTxid};
use amplify::{ByteArray, hex::ToHex};
use hex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Tondi 交易查询测试 ===\n");
    
    let endpoint = "38.190.226.7:16210";
    let client = TondiClient::connect(TondiConfig::new(endpoint)).await?;
    
    // 查询你那个 UTXO 的来源交易
    // 这个交易 ID 来自 fund_bob.rs 中使用的 UTXO
    let existing_txid_hex = "0012875645e355832f97a0eff09d79f661531fd73184e59841547b03c326e547";
    println!("正在查询交易: {}", existing_txid_hex);
    
    // 解析交易 ID（32 字节的 BLAKE3 哈希）
    let txid_bytes = hex::decode(existing_txid_hex)
        .map_err(|e| format!("Failed to decode transaction ID: {}", e))?;
    
    if txid_bytes.len() != 32 {
        return Err(format!("Invalid transaction ID length: {} (expected 32)", txid_bytes.len()).into());
    }
    
    let txid_array: [u8; 32] = txid_bytes.try_into()
        .map_err(|_| "Failed to convert to array")?;
    
    let existing_txid = TondiTxid::from_byte_array(txid_array);
    
    println!("  - 交易 ID (Bytes32): {}\n", existing_txid.to_hex());
    
    match client.get_transaction(existing_txid).await {
        Ok(tx) => {
            println!("✓ 查询成功！");
            println!("  - 交易版本: {}", tx.version);
            println!("  - 输入数量: {}", tx.inputs.len());
            println!("  - 输出数量: {}", tx.outputs.len());
            println!("  - Lock Time: {}", tx.lock_time);
            println!("  - Subnetwork ID: {}", tx.subnetwork_id.to_hex());
            println!("  - Gas: {}", tx.gas);
            println!("  - Payload 长度: {} 字节", tx.payload.len());
            println!("  - Mass: {}", tx.mass);
            println!("\n🎉 交易查询功能正常工作！\n");
        }
        Err(e) => {
            eprintln!("✗ 查询失败: {}", e);
            eprintln!("\n可能的原因：");
            eprintln!("  1. 交易不存在于节点账本中");
            eprintln!("  2. 节点未同步到该交易所在的区块");
            eprintln!("  3. RPC 调用格式错误");
            eprintln!("  4. 网络连接问题\n");
            return Err(e.into());
        }
    }
    
    Ok(())
}

