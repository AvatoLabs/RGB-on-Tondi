//! Query a Tondi transaction over RPC.
//!
//! ```bash
//! TONDI_RPC=127.0.0.1:16210 \
//! TONDI_TXID=<32-byte-hex-txid> \
//! cargo run --example query_proof --features grpc
//! ```

use amplify::{hex::ToHex, ByteArray};
use tondi::{TondiClient, TondiConfig, TondiTxid};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Tondi transaction query test ===\n");

    let endpoint = std::env::var("TONDI_RPC").unwrap_or_else(|_| "38.190.226.7:16210".to_owned());
    let txid_hex = std::env::var("TONDI_TXID").map_err(|_| "TONDI_TXID is required and must be a 32-byte hex transaction ID")?;

    println!("Connecting to node: {}", endpoint);
    println!("Querying transaction: {}", txid_hex);

    let client = TondiClient::connect(TondiConfig::new(endpoint)).await?;

    let txid_bytes = hex::decode(&txid_hex).map_err(|e| format!("Failed to decode transaction ID: {}", e))?;
    if txid_bytes.len() != 32 {
        return Err(format!("Invalid transaction ID length: {} (expected 32)", txid_bytes.len()).into());
    }

    let txid_array: [u8; 32] = txid_bytes.try_into().map_err(|_| "Failed to convert transaction ID into a byte array")?;
    let txid = TondiTxid::from_byte_array(txid_array);

    println!("  - Transaction ID (Bytes32): {}\n", txid.to_hex());

    match client.get_transaction(txid).await {
        Ok(tx) => {
            println!("Query succeeded.");
            println!("  - Transaction version: {}", tx.version);
            println!("  - Inputs: {}", tx.inputs.len());
            println!("  - Outputs: {}", tx.outputs.len());
            println!("  - Lock time: {}", tx.lock_time);
            println!("  - Subnetwork ID: {}", tx.subnetwork_id.to_hex());
            println!("  - Gas: {}", tx.gas);
            println!("  - Payload length: {} bytes", tx.payload.len());
            println!("  - Mass: {}", tx.mass);
        }
        Err(e) => {
            eprintln!("Query failed: {}", e);
            eprintln!("Possible causes:");
            eprintln!("  1. The transaction is not known by the node.");
            eprintln!("  2. The node has not indexed the block containing the transaction.");
            eprintln!("  3. The RPC endpoint or network does not match the transaction.");
            return Err(e.into());
        }
    }

    Ok(())
}
