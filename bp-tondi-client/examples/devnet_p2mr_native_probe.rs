//! Devnet probe for Tondi-native RGB P2MR policy.
//!
//! This example intentionally does not publish RGB metadata on-chain. It checks
//! that the local wire contract accepts only Tondi Standard + P2MR v4 outputs
//! with empty payload, then connects to a devnet node for liveness/DAG checks.
//! Set `TONDI_DEVNET_ALLOW_UNSYNCED=1` only for an isolated local devnet node.

use amplify::Bytes32;
use tondi::{
    NetworkType, ScriptPublicKey, TondiClient, TondiConfig, TondiTransaction, TondiTxOutput,
    TONDI_P2MR_SCRIPT_VERSION,
};

fn native_p2mr_probe_tx() -> TondiTransaction {
    TondiTransaction {
        version: 1,
        inputs: Vec::new(),
        outputs: vec![TondiTxOutput {
            datum: None,
            value: 1,
            script_public_key: ScriptPublicKey {
                version: TONDI_P2MR_SCRIPT_VERSION,
                script: vec![0x42; 32],
            },
        }],
        lock_time: 0,
        subnetwork_id: Bytes32::from_byte_array([0u8; 32]),
        gas: 0,
        payload: Vec::new(),
        mass: 0,
    }
}

fn verify_native_policy() -> Result<(), tondi::Error> {
    let tx = native_p2mr_probe_tx();
    tx.validate_tondi_native_rgb()?;

    let mut payload_tx = tx.clone();
    payload_tx.payload = vec![0x50, 0x20, 0x20];
    assert!(
        payload_tx.validate_tondi_native_rgb().is_err(),
        "payload carriers must be rejected"
    );

    let mut legacy_tx = tx;
    let script_public_key = &mut legacy_tx.outputs[0].script_public_key;
    script_public_key.version = 0;
    assert!(
        legacy_tx.validate_tondi_native_rgb().is_err(),
        "non-P2MR outputs must be rejected"
    );

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), tondi::Error> {
    verify_native_policy()?;

    let endpoint = std::env::var("TONDI_DEVNET_RPC").map_err(|_| {
        tondi::Error::Connection(
            "TONDI_DEVNET_RPC must point at a devnet gRPC endpoint, e.g. 127.0.0.1:16210"
                .to_string(),
        )
    })?;
    let allow_unsynced = std::env::var("TONDI_DEVNET_ALLOW_UNSYNCED").as_deref() == Ok("1");

    let config = TondiConfig::new(endpoint.clone()).with_network(NetworkType::Devnet);
    let client = TondiClient::connect(config).await?;
    client.ping().await?;

    let info = client.get_server_info().await?;
    if !info.is_synced && !allow_unsynced {
        return Err(tondi::Error::NodeNotSynced);
    }

    let dag = client.get_block_dag_info().await?;
    println!(
        "devnet probe ok: endpoint={endpoint}, server={}, network={}, synced={}, virtual_daa_score={}, block_count={}",
        info.server_version, info.network_id, info.is_synced, dag.virtual_daa_score, dag.block_count
    );

    Ok(())
}
