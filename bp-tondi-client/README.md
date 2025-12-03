# bp-tondi

Tondi RPC client library for RGB/BP integration.

## Overview

This crate provides a native Tondi RPC client for the RGB protocol stack, replacing the Esplora client for Tondi blockchain integration.

Tondi is a Kaspa fork with Taproot support and BLAKE3 hashing.

## Features

- **Native Tondi RPC API**: Direct integration with Tondi's gRPC and wRPC protocols
- **UTXO queries**: Query UTXOs by address for wallet operations
- **Transaction submission**: Submit transactions to the Tondi mempool
- **Block and DAG queries**: Access block data and DAG information
- **Real-time notifications**: Subscribe to blockchain events via WebSocket
- **RGB/BP compatibility**: Type conversions for seamless integration with RGB stack

## Usage

### Basic Connection

```rust
use tondi::{TondiClient, TondiConfig};

#[tokio::main]
async fn main() -> Result<(), tondi::Error> {
    // Connect to mainnet
    let config = TondiConfig::mainnet("localhost:17110");
    let client = TondiClient::connect(config).await?;
    
    // Check connection
    client.ping().await?;
    
    // Get sync status
    let is_synced = client.get_sync_status().await?;
    println!("Node synced: {}", is_synced);
    
    Ok(())
}
```

### Query UTXOs

```rust
use tondi::{TondiClient, TondiConfig};

async fn query_utxos(client: &TondiClient) -> Result<(), tondi::Error> {
    let address = "tondi1qp...";
    let utxos = client.get_utxos_by_address(address).await?;
    
    for utxo in utxos {
        println!("UTXO: {:?} - {} saus", utxo.outpoint, utxo.utxo_entry.amount);
    }
    
    Ok(())
}
```

### Submit Transaction

```rust
use tondi::{TondiClient, TondiTransaction};

async fn submit_tx(client: &TondiClient, tx: TondiTransaction) -> Result<(), tondi::Error> {
    let txid = client.submit_transaction(tx, false).await?;
    println!("Transaction submitted: {:?}", txid);
    Ok(())
}
```

### Fee Estimation

```rust
use tondi::TondiClient;

async fn estimate_fee(client: &TondiClient) -> Result<f64, tondi::Error> {
    let estimate = client.get_fee_estimate().await?;
    
    // Get fee rate for ~10 second confirmation
    let fee_rate = TondiClient::convert_fee_rate(10, &estimate)
        .unwrap_or(estimate.priority_bucket.feerate);
    
    Ok(fee_rate)
}
```

## Network Configuration

### Mainnet

```rust
let config = TondiConfig::mainnet("node.tondi.io:17110");
```

### Testnet

```rust
let config = TondiConfig::testnet("testnet.tondi.io:17210");
```

## Features Flags

- `default` - Async client with Tokio runtime
- `async` - Enable async support (requires Tokio)
- `grpc` - Enable gRPC transport (requires tonic)
- `wrpc` - Enable wRPC (WebSocket) transport

## Requirements

For UTXO-related queries, the Tondi node must be started with the `--utxoindex` flag.

## License

MIT

