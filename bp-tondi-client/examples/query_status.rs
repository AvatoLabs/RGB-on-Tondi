//! Query a Tondi transaction status over RPC.
//!
//! ```bash
//! TONDI_RPC=127.0.0.1:16210 \
//! TONDI_TXID=<32-byte-hex-txid> \
//! cargo run -p bp-tondi --features grpc --example query_status
//! ```

use tondi::{TondiClient, TondiConfig, TondiTxid};

fn status_label(status: &tondi::TxStatus) -> &'static str {
    if status.removed {
        "removed"
    } else if status.accepted {
        "accepted"
    } else if status.in_mempool {
        "mempool"
    } else {
        "unknown"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::var("TONDI_RPC").unwrap_or_else(|_| "127.0.0.1:16210".to_owned());
    let txid_hex = std::env::var("TONDI_TXID")
        .map_err(|_| "TONDI_TXID is required and must be a 32-byte hex transaction ID")?;

    let txid_bytes = hex::decode(txid_hex.trim_start_matches("0x"))
        .map_err(|err| format!("Failed to decode transaction ID: {err}"))?;
    if txid_bytes.len() != 32 {
        return Err(format!(
            "Invalid transaction ID length: {} (expected 32)",
            txid_bytes.len()
        )
        .into());
    }

    let txid_array: [u8; 32] = txid_bytes
        .try_into()
        .map_err(|_| "Failed to convert transaction ID into a byte array")?;
    let txid = TondiTxid::from_byte_array(txid_array);
    let client = TondiClient::connect(TondiConfig::new(endpoint)).await?;
    let status = client.get_transaction_status(txid).await?;
    let status_label = status_label(&status);

    println!("status={status_label}");
    println!("accepted={}", status.accepted);
    println!("in_mempool={}", status.in_mempool);
    println!("removed={}", status.removed);
    if let Some(hash) = status.accepting_block_hash {
        println!("accepting_block_hash={hash}");
    }
    if let Some(daa_score) = status.accepting_block_daa_score {
        println!("accepting_block_daa_score={daa_score}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx_status(accepted: bool, in_mempool: bool, removed: bool) -> tondi::TxStatus {
        tondi::TxStatus {
            accepted,
            in_mempool,
            removed,
            accepting_block_hash: None,
            accepting_block_daa_score: None,
            block_time: None,
        }
    }

    #[test]
    fn labels_removed_separately_from_unknown() {
        assert_eq!(status_label(&tx_status(false, false, true)), "removed");
        assert_eq!(status_label(&tx_status(false, false, false)), "unknown");
    }

    #[test]
    fn labels_mempool_and_accepted() {
        assert_eq!(status_label(&tx_status(false, true, false)), "mempool");
        assert_eq!(status_label(&tx_status(true, false, false)), "accepted");
    }
}
