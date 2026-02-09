//! 测试与远程 Tondi 节点的连接
//!
//! 运行方式：
//! ```bash
//! cd bp-tondi-client
//! cargo run --example test_remote_node --features grpc
//! ```

use tondi::{TondiClient, TondiConfig};

#[tokio::main]
async fn main() -> Result<(), tondi::Error> {
    println!("=== Tondi 远程节点连接测试 ===\n");

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
            return Err(e);
        }
    };

    // 测试 1: Ping
    println!("[测试 1] 测试 Ping...");
    match client.ping().await {
        Ok(_) => println!("✓ Ping 成功\n"),
        Err(e) => {
            eprintln!("✗ Ping 失败: {}\n", e);
            return Err(e);
        }
    }

    // 测试 2: 获取服务器信息
    println!("[测试 2] 获取服务器信息...");
    match client.get_server_info().await {
        Ok(info) => {
            println!("✓ 获取服务器信息成功:");
            println!("  - 服务器版本: {}", info.server_version);
            println!("  - 是否同步: {}", info.is_synced);
            println!("  - UTXO 索引: {}", info.has_utxo_index);
            println!("  - RPC API 版本: {}", info.rpc_api_version);
            println!("  - RPC API 修订: {}", info.rpc_api_revision);
            println!("  - 网络 ID: {}", if info.network_id.is_empty() { "N/A" } else { &info.network_id });
            println!("  - Virtual DAA Score: {}\n", info.virtual_daa_score);
        }
        Err(e) => {
            eprintln!("✗ 获取服务器信息失败: {}\n", e);
            return Err(e);
        }
    }

    // 测试 3: 获取同步状态
    println!("[测试 3] 获取同步状态...");
    match client.get_sync_status().await {
        Ok(is_synced) => {
            println!("✓ 同步状态: {}\n", if is_synced { "已同步" } else { "未同步" });
        }
        Err(e) => {
            eprintln!("✗ 获取同步状态失败: {}\n", e);
            return Err(e);
        }
    }

    // 测试 4: 获取 DAG 信息（如果实现）
    println!("[测试 4] 获取 DAG 信息...");
    match client.get_block_dag_info().await {
        Ok(dag_info) => {
            println!("✓ 获取 DAG 信息成功:");
            println!("  - 网络名称: {}", dag_info.network_name);
            println!("  - 区块数量: {}", dag_info.block_count);
            println!("  - 难度: {}", dag_info.difficulty);
            println!("  - Virtual DAA Score: {}\n", dag_info.virtual_daa_score);
        }
        Err(e) => {
            eprintln!("✗ 获取 DAG 信息失败: {} (可能尚未实现)\n", e);
        }
    }

    println!("=== 所有测试完成 ===");
    Ok(())
}

