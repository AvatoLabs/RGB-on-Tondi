//! Acceptance proof for P2MR + public Datum + reusable references on an isolated devnet.
use amplify::ByteArray;
use anyhow::{ensure, Context};
use bpstd::{
    secp256k1, tondi_p2mr_script_sighash, tondi_tapleaf_hash_from_tapscript, Datum, LockTime,
    Outpoint, ScriptPubkey, SeqNo, SigScript, Tx, TxIn, TxOut, TxVer, VarIntArray, VarIntBytes,
    Vout, Witness,
};
use std::time::Duration;
use tondi::{
    NetworkType, PublicDataKind, PublicDatum, PublicReference, TondiClient, TondiConfig,
    TondiTransaction, TondiTxid,
};
const FEE: u64 = 1_000_000;
struct Signer {
    key: secp256k1::Keypair,
    leaf: Vec<u8>,
    hash: [u8; 32],
    root: Vec<u8>,
}
fn signer(secret: &str) -> anyhow::Result<Signer> {
    let bytes = hex::decode(secret)?;
    let key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &bytes)?;
    let mut leaf = vec![0x20];
    leaf.extend(key.public_key().x_only_public_key().0.serialize());
    leaf.push(0xac);
    let hash = tondi_tapleaf_hash_from_tapscript(&leaf).to_byte_array();
    Ok(Signer {
        key,
        leaf,
        hash,
        root: hash.to_vec(),
    })
}
fn prealloc(id: u64) -> Outpoint {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&id.to_le_bytes());
    Outpoint::new(hex::encode(bytes).parse().unwrap(), 0u32)
}
fn make_tx(point: Outpoint, amount: u64, signer: &Signer) -> Tx {
    Tx {
        version: TxVer::CURRENT,
        inputs: VarIntArray::from_checked(vec![TxIn {
            prev_output: point,
            is_reference: false,
            sig_op_count: 1,
            sig_script: SigScript::new(),
            sequence: SeqNo::from_consensus_u32(u32::MAX),
            witness: Witness::new(),
        }]),
        outputs: VarIntArray::from_checked(vec![TxOut::p2mr(
            ScriptPubkey::from_checked(signer.root.clone()),
            amount - FEE,
        )]),
        lock_time: LockTime::ZERO,
        subnetwork_id: [0; 20],
        gas: 0,
        payload: VarIntBytes::new(),
        mass: 0,
    }
}
fn sign(tx: &mut Tx, prevouts: &[TxOut], signer: &Signer) -> anyhow::Result<()> {
    let digest = tondi_p2mr_script_sighash(tx, prevouts, 0, signer.hash)?;
    let signature = secp256k1::SECP256K1
        .sign_schnorr_no_aux_rand(&digest, &signer.key)
        .to_byte_array();
    let items = [signature.to_vec(), signer.leaf.clone(), vec![0xc1]];
    let mut script = 3u32.to_le_bytes().to_vec();
    for item in items {
        script.extend((item.len() as u32).to_le_bytes());
        script.extend(item);
    }
    tx.inputs[0].sig_script = SigScript::from_checked(script);
    Ok(())
}
async fn broadcast(client: &TondiClient, tx: &Tx) -> anyhow::Result<TondiTxid> {
    let id = client
        .submit_transaction(TondiTransaction::from_bp_tx(tx)?, false)
        .await?;
    ensure!(
        id.to_string() == tx.txid().to_string(),
        "node/client transaction ID mismatch"
    );
    Ok(id)
}
async fn wait_accepted(client: &TondiClient, id: TondiTxid) -> anyhow::Result<()> {
    for _ in 0..120 {
        if client.get_transaction_status(id).await?.accepted {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("native transaction did not reach accepted status: {id}")
}
async fn submit(client: &TondiClient, tx: &Tx) -> anyhow::Result<()> {
    wait_accepted(client, broadcast(client, tx).await?).await
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = std::env::var("TONDI_RPC")?;
    let address = std::env::var("FAUCET_P2MR_ADDRESS")?;
    let signer = signer(&std::env::var("FAUCET_P2MR_SECRET_KEY")?)?;
    let amount: u64 = std::env::var("FAUCET_PREALLOC_AMOUNT")?.parse()?;
    let client =
        TondiClient::connect(TondiConfig::new(endpoint).with_network(NetworkType::Devnet)).await?;
    let source = TxOut::p2mr(ScriptPubkey::from_checked(signer.root.clone()), amount);
    let mut creation = make_tx(prealloc(2), amount, &signer);
    creation.outputs[0].datum = Some(
        PublicDatum {
            kind: PublicDataKind::Policy,
            data: Datum::from_checked(b"explicit public policy v1".to_vec()),
        }
        .encode()?,
    );
    sign(&mut creation, std::slice::from_ref(&source), &signer)?;
    submit(&client, &creation)
        .await
        .context("publish public context")?;
    let id = creation.txid().to_string();
    let entry = client
        .get_utxos_by_address(&address)
        .await?
        .into_iter()
        .find(|e| e.outpoint.transaction_id.to_string() == id && e.outpoint.index == 0)
        .context("published context must remain live")?;
    let pin = PublicReference::from_live(&entry)?;
    pin.resolve(&client, &address).await?;
    let mut readers = Vec::new();
    for source_id in [3, 4] {
        let mut reader = make_tx(prealloc(source_id), amount, &signer);
        let read = pin.append_to(&mut reader, &entry)?;
        reader.outputs[0].datum = Some(
            PublicDatum {
                kind: PublicDataKind::StateDigest,
                data: Datum::from_checked(vec![source_id as u8; 32]),
            }
            .encode()?,
        );
        sign(&mut reader, &[source.clone(), read], &signer)?;
        readers.push(reader);
    }
    // Put both readers into the node before waiting for either acceptance.
    let mut reader_ids = Vec::new();
    for reader in &readers {
        reader_ids.push(
            broadcast(&client, reader)
                .await
                .context("submit shared reader")?,
        );
    }
    for id in reader_ids {
        wait_accepted(&client, id)
            .await
            .context("accept shared reader")?;
    }
    pin.resolve(&client, &address)
        .await
        .context("references must not consume context")?;
    let mut consume = make_tx(
        Outpoint::new(creation.txid(), Vout::from_u32(0)),
        creation.outputs[0].value.sats(),
        &signer,
    );
    sign(&mut consume, &[creation.outputs[0].clone()], &signer)?;
    submit(&client, &consume)
        .await
        .context("consume old context version")?;
    ensure!(
        pin.resolve(&client, &address).await.is_err(),
        "spent context must not resolve as live"
    );
    pin.verify_witness(&creation, &readers[0])?;
    let prev = readers[0].outputs[0].clone();
    let mut stale = make_tx(
        Outpoint::new(readers[0].txid(), 0u32),
        prev.value.sats(),
        &signer,
    );
    let old = pin.append_to(&mut stale, &entry)?;
    sign(&mut stale, &[prev, old], &signer)?;
    let rejection = client
        .submit_transaction(TondiTransaction::from_bp_tx(&stale)?, false)
        .await;
    ensure!(
        rejection.is_err(),
        "node accepted a reference to a spent UTXO"
    );
    println!("public_context_txid={id}");
    println!("public_context_readers_accepted={}", readers.len());
    println!("public_context_readers_submitted_before_wait=true");
    println!("public_context_stale_reference_rejected=true");
    println!("public_context_historical_witness_verified=true");
    Ok(())
}
