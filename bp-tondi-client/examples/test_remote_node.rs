//! Test connectivity to a Tondi RPC node.
//!
//! ```bash
//! cd bp-tondi-client
//! TONDI_RPC=127.0.0.1:16210 cargo run --example test_remote_node --features grpc
//! ```

use tondi::{TondiClient, TondiConfig};

#[tokio::main]
async fn main() -> Result<(), tondi::Error> {
    println!("=== Tondi remote node connectivity test ===\n");

    let endpoint = std::env::var("TONDI_RPC").unwrap_or_else(|_| "38.190.226.7:16210".to_owned());
    println!("Connecting to remote node: {}", endpoint);

    let config = TondiConfig::new(endpoint).with_timeout(std::time::Duration::from_secs(10));

    let client = match TondiClient::connect(config).await {
        Ok(client) => {
            println!("Connection established.\n");
            client
        }
        Err(e) => {
            eprintln!("Connection failed: {}", e);
            return Err(e);
        }
    };

    println!("[Test 1] Ping");
    match client.ping().await {
        Ok(_) => println!("Ping succeeded.\n"),
        Err(e) => {
            eprintln!("Ping failed: {}\n", e);
            return Err(e);
        }
    }

    println!("[Test 2] Server info");
    match client.get_server_info().await {
        Ok(info) => {
            println!("Server info received:");
            println!("  - Server version: {}", info.server_version);
            println!("  - Synced: {}", info.is_synced);
            println!("  - UTXO index: {}", info.has_utxo_index);
            println!("  - RPC API version: {}", info.rpc_api_version);
            println!("  - RPC API revision: {}", info.rpc_api_revision);
            println!(
                "  - Network ID: {}",
                if info.network_id.is_empty() {
                    "N/A"
                } else {
                    &info.network_id
                }
            );
            println!("  - Virtual DAA score: {}\n", info.virtual_daa_score);
        }
        Err(e) => {
            eprintln!("Server info failed: {}\n", e);
            return Err(e);
        }
    }

    println!("[Test 3] Sync status");
    match client.get_sync_status().await {
        Ok(is_synced) => {
            println!(
                "Sync status: {}\n",
                if is_synced { "synced" } else { "not synced" }
            );
        }
        Err(e) => {
            eprintln!("Sync status failed: {}\n", e);
            return Err(e);
        }
    }

    println!("[Test 4] DAG info");
    match client.get_block_dag_info().await {
        Ok(dag_info) => {
            println!("DAG info received:");
            println!("  - Network name: {}", dag_info.network_name);
            println!("  - Block count: {}", dag_info.block_count);
            println!("  - Difficulty: {}", dag_info.difficulty);
            println!("  - Virtual DAA score: {}\n", dag_info.virtual_daa_score);
        }
        Err(e) => {
            eprintln!(
                "DAG info failed: {} (endpoint may not expose this call)\n",
                e
            );
        }
    }

    println!("=== Connectivity test complete ===");
    Ok(())
}
