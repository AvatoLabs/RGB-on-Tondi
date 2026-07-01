//! Development-only P2MR funder for isolated Tondi devnet runs.
//!
//! The example spends one genesis-preallocated faucet P2MR UTXO into real
//! P2MR funding outputs. Those outputs have a resolvable previous transaction,
//! so the RGB wallet can later use them as normal seals and transaction inputs.

use std::str::FromStr;

use amplify::{ByteArray, Bytes32};
use anyhow::{anyhow, bail, Context};
use bpstd::{
    secp256k1, tondi_tapleaf_hash_from_tapscript, LockTime, Outpoint, Sats, ScriptPubkey, SeqNo,
    SigScript, Tx, TxIn, TxOut, TxVer, Txid, VarIntArray, VarIntBytes, Vout, Witness,
};
use sha2::{Digest, Sha256};
use tondi::{
    NetworkType, ScriptPublicKey, TondiClient, TondiConfig, TondiOutpoint, TondiTransaction,
    TondiTxInput, TondiTxOutput,
};

const OP_PUSHBYTES_32: u8 = 0x20;
const OP_CHECKSIG: u8 = 0xac;
const TONDI_TAPSIGHASH_TAG: &str = "TapSighash";
const TONDI_TAPROOT_KEY_VERSION_0: u8 = 0;
const TONDI_P2MR_CONTROL_BLOCK_SINGLE_LEAF: [u8; 1] = [0xc1];
const P2MR_ADDRESS_VERSION: u8 = 216;
const BECH32M_CONST: u32 = 0x2bc830a3;
const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const GENERATORS: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

struct Signer {
    keypair: secp256k1::Keypair,
    leaf_script: Vec<u8>,
    leaf_hash: bpstd::TapLeafHash,
    p2mr_root: [u8; 32],
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = env_required("TONDI_RPC")?;
    let secret_key = env_required("FAUCET_P2MR_SECRET_KEY")?;
    let target_spec = env_required("FUND_TARGETS")?;
    let prealloc_id = env_parse::<u64>("FAUCET_PREALLOC_ID")?.unwrap_or(1);
    let prealloc_amount = env_parse::<u64>("FAUCET_PREALLOC_AMOUNT")?.unwrap_or(10_000_000_000);
    let sequence = env_parse::<u32>("SEQUENCE")?.unwrap_or(0);

    let signer = p2mr_signer(&secret_key)?;
    let targets = parse_targets(&target_spec)?;
    let output_sum = targets.iter().try_fold(0u64, |sum, (_, amount)| {
        sum.checked_add(*amount)
            .ok_or_else(|| anyhow!("target amount overflow"))
    })?;
    if output_sum >= prealloc_amount {
        bail!("target outputs ({output_sum}) must be lower than faucet input amount ({prealloc_amount})");
    }

    let source_txid = prealloc_txid(prealloc_id);
    let source_outpoint =
        Outpoint::new(Txid::from_str(&source_txid.to_string())?, Vout::from_u32(0));
    let source_prevout = TxOut::p2mr(
        ScriptPubkey::from_checked(signer.p2mr_root.to_vec()),
        Sats::from_sats(prealloc_amount),
    );

    let inputs = vec![TxIn {
        prev_output: source_outpoint,
        sig_script: SigScript::default(),
        sequence: SeqNo::from_consensus_u32(sequence),
        witness: Witness::new(),
    }];
    let outputs = targets
        .into_iter()
        .map(|(root, amount)| {
            TxOut::p2mr(ScriptPubkey::from_checked(root), Sats::from_sats(amount))
        })
        .collect::<Vec<_>>();

    let mut tx = Tx {
        version: TxVer::V0,
        inputs: VarIntArray::from_checked(inputs),
        outputs: VarIntArray::from_checked(outputs),
        lock_time: LockTime::ZERO,
        subnetwork_id: [0u8; 20],
        gas: 0,
        payload: VarIntBytes::new(),
        mass: 0,
    };

    sign_input(&mut tx, &[source_prevout], 0, &signer)?;
    let expected_txid = tx.txid();
    let native_tx = bp_tx_to_tondi(&tx)?;

    let client =
        TondiClient::connect(TondiConfig::new(endpoint).with_network(NetworkType::Devnet)).await?;
    let submitted_txid = client.submit_transaction(native_tx, false).await?;
    let submitted_bp_txid = Txid::from_str(&submitted_txid.to_string())?;
    if submitted_bp_txid != expected_txid {
        bail!("node returned txid {submitted_bp_txid}, expected {expected_txid}");
    }

    println!("funding_txid={expected_txid}");
    println!("funding_fee={}", prealloc_amount - output_sum);
    Ok(())
}

fn env_required(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("{name} is required"))
}

fn env_parse<T>(name: &str) -> anyhow::Result<Option<T>>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<T>()
                .with_context(|| format!("invalid {name}"))
        })
        .transpose()
}

fn parse_targets(spec: &str) -> anyhow::Result<Vec<(Vec<u8>, u64)>> {
    let mut targets = Vec::new();
    for target in spec
        .split(',')
        .map(str::trim)
        .filter(|target| !target.is_empty())
    {
        let (address, amount) = target
            .rsplit_once('=')
            .ok_or_else(|| anyhow!("target must use <address>=<amount>, got `{target}`"))?;
        let amount = amount
            .parse::<u64>()
            .with_context(|| format!("invalid amount in `{target}`"))?;
        if amount == 0 {
            bail!("target amount must be greater than zero in `{target}`");
        }
        targets.push((p2mr_root_from_address(address)?, amount));
    }
    if targets.is_empty() {
        bail!("FUND_TARGETS must contain at least one target");
    }
    Ok(targets)
}

fn p2mr_root_from_address(address: &str) -> anyhow::Result<Vec<u8>> {
    let (prefix, payload) = address
        .split_once(':')
        .ok_or_else(|| anyhow!("P2MR address `{address}` is missing a network prefix"))?;
    if !matches!(prefix, "tondi" | "tondi0" | "tondisim" | "tondidev") {
        bail!("address `{address}` uses unsupported Tondi prefix `{prefix}`");
    }
    let values = payload
        .bytes()
        .map(|byte| {
            CHARSET
                .iter()
                .position(|candidate| *candidate == byte)
                .map(|index| index as u8)
                .ok_or_else(|| anyhow!("address `{address}` contains an invalid P2MR character"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if values.len() <= 6 {
        bail!("address `{address}` is too short");
    }

    let mut checksum_values = bytes_to_five_bit(prefix.as_bytes());
    checksum_values.extend_from_slice(&values);
    if polymod(checksum_values) != BECH32M_CONST {
        bail!("address `{address}` has an invalid P2MR checksum");
    }

    let payload_values = &values[..values.len() - 6];
    let bytes = five_bit_to_bytes(payload_values)?;
    match bytes.as_slice() {
        [P2MR_ADDRESS_VERSION, root @ ..]
            if root.len() == 32 && root.iter().any(|byte| *byte != 0) =>
        {
            Ok(root.to_vec())
        }
        [version, root @ ..] => bail!(
            "address `{address}` has unsupported P2MR version {version}; root bytes={}",
            root.len()
        ),
        _ => bail!("address `{address}` does not encode a 32-byte P2MR root"),
    }
}

fn bytes_to_five_bit(bytes: &[u8]) -> Vec<u8> {
    let mut values = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u32;

    for byte in bytes {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            values.push(((accumulator >> bits) & 0x1f) as u8);
        }
    }

    if bits > 0 {
        values.push(((accumulator << (5 - bits)) & 0x1f) as u8);
    }
    values
}

fn five_bit_to_bytes(values: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u32;

    for value in values {
        if *value > 0x1f {
            bail!("invalid five-bit P2MR value {value}");
        }
        accumulator = (accumulator << 5) | u32::from(*value);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            bytes.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    if bits > 0 && ((accumulator << (8 - bits)) & 0xff) != 0 {
        bail!("non-zero P2MR address padding");
    }
    Ok(bytes)
}

fn polymod(values: impl IntoIterator<Item = u8>) -> u32 {
    let mut checksum = 1u32;
    for value in values {
        let top = checksum >> 25;
        checksum = ((checksum & 0x1ffffff) << 5) ^ u32::from(value);
        for (index, generator) in GENERATORS.iter().enumerate() {
            if ((top >> index) & 1) != 0 {
                checksum ^= generator;
            }
        }
    }
    checksum
}

fn prealloc_txid(id: u64) -> Bytes32 {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&id.to_le_bytes());
    Bytes32::from_byte_array(bytes)
}

fn p2mr_signer(secret_key: &str) -> anyhow::Result<Signer> {
    let secret_key = hex::decode(secret_key.trim().trim_start_matches("0x"))
        .context("invalid faucet secret key hex")?;
    let secret_key: [u8; 32] = secret_key.try_into().map_err(|bytes: Vec<u8>| {
        anyhow!("faucet secret key must be 32 bytes, got {}", bytes.len())
    })?;
    let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key)?;
    let xonly = keypair.public_key().x_only_public_key().0.serialize();
    let leaf_script = p2mr_single_key_leaf_script(&xonly);
    let leaf_hash = tondi_tapleaf_hash_from_tapscript(&leaf_script);
    Ok(Signer {
        keypair,
        leaf_script,
        leaf_hash,
        p2mr_root: leaf_hash.to_byte_array(),
    })
}

fn p2mr_single_key_leaf_script(xonly_public_key: &[u8; 32]) -> Vec<u8> {
    let mut script = Vec::with_capacity(34);
    script.push(OP_PUSHBYTES_32);
    script.extend_from_slice(xonly_public_key);
    script.push(OP_CHECKSIG);
    script
}

fn sign_input(
    tx: &mut Tx,
    prevouts: &[TxOut],
    input_index: usize,
    signer: &Signer,
) -> anyhow::Result<()> {
    let sighash = p2mr_script_sighash(tx, prevouts, input_index, signer.leaf_hash)?;
    let sig = secp256k1::SECP256K1
        .sign_schnorr_no_aux_rand(&sighash, &signer.keypair)
        .to_byte_array();
    let sig_script = encode_p2mr_witness_stack([
        sig.to_vec(),
        signer.leaf_script.clone(),
        TONDI_P2MR_CONTROL_BLOCK_SINGLE_LEAF.to_vec(),
    ])?;
    tx.inputs[input_index].sig_script = SigScript::from_checked(sig_script);
    Ok(())
}

fn p2mr_script_sighash(
    tx: &Tx,
    prevouts: &[TxOut],
    input_index: usize,
    leaf_hash: bpstd::TapLeafHash,
) -> anyhow::Result<[u8; 32]> {
    if tx.inputs.len() != prevouts.len() {
        bail!(
            "transaction input count ({}) does not match prevout count ({})",
            tx.inputs.len(),
            prevouts.len()
        );
    }
    if input_index >= tx.inputs.len() {
        bail!(
            "input index {input_index} is out of bounds for {} inputs",
            tx.inputs.len()
        );
    }

    let mut preimage = Vec::new();
    preimage.push(0);
    preimage.push(0);
    append_le_u16(&mut preimage, tx.version.to_consensus_u16());
    append_le_u64(&mut preimage, tx.lock_time.to_consensus_u64());
    preimage.extend_from_slice(&sha256_digest(&encode_prevouts(tx)?));
    preimage.extend_from_slice(&sha256_digest(&encode_prevout_amounts(prevouts)));
    preimage.extend_from_slice(&sha256_digest(&encode_prevout_script_pubkeys(prevouts)?));
    preimage.extend_from_slice(&sha256_digest(&encode_sequences(tx)));
    preimage.extend_from_slice(&sha256_digest(&encode_outputs(tx)?));
    preimage.push(2);
    append_le_u32(
        &mut preimage,
        u32::try_from(input_index).context("input index exceeds u32")?,
    );
    preimage.extend_from_slice(&leaf_hash.to_byte_array());
    preimage.push(TONDI_TAPROOT_KEY_VERSION_0);
    append_le_u32(&mut preimage, u32::MAX);
    Ok(sha256_tagged_hash(TONDI_TAPSIGHASH_TAG, &preimage))
}

fn encode_prevouts(tx: &Tx) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for input in tx.inputs.iter() {
        let txid = input.prev_output.txid.to_string().parse::<Bytes32>()?;
        bytes.extend_from_slice(&txid.to_byte_array());
        append_le_u32(&mut bytes, input.prev_output.vout.to_u32());
    }
    Ok(bytes)
}

fn encode_prevout_amounts(prevouts: &[TxOut]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for prevout in prevouts {
        append_le_u64(&mut bytes, prevout.value.sats());
    }
    bytes
}

fn encode_prevout_script_pubkeys(prevouts: &[TxOut]) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for prevout in prevouts {
        append_script_public_key(
            &mut bytes,
            prevout.script_pubkey_version,
            prevout.script_pubkey.as_slice(),
        )?;
    }
    Ok(bytes)
}

fn encode_sequences(tx: &Tx) -> Vec<u8> {
    let mut bytes = Vec::new();
    for input in tx.inputs.iter() {
        append_le_u64(
            &mut bytes,
            tondi_wire_sequence(input.sequence.to_consensus_u32()),
        );
    }
    bytes
}

fn encode_outputs(tx: &Tx) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for output in tx.outputs.iter() {
        append_le_u64(&mut bytes, output.value.sats());
        append_script_public_key(
            &mut bytes,
            output.script_pubkey_version,
            output.script_pubkey.as_slice(),
        )?;
    }
    Ok(bytes)
}

fn append_script_public_key(
    bytes: &mut Vec<u8>,
    script_pubkey_version: u16,
    script: &[u8],
) -> anyhow::Result<()> {
    append_le_u16(bytes, script_pubkey_version);
    append_compact_size(bytes, script.len(), "Tondi script public key length")?;
    bytes.extend_from_slice(script);
    Ok(())
}

fn bp_tx_to_tondi(tx: &Tx) -> anyhow::Result<TondiTransaction> {
    let inputs = tx
        .inputs
        .iter()
        .map(|input| {
            Ok(TondiTxInput::Standard {
                previous_outpoint: TondiOutpoint {
                    transaction_id: input.prev_output.txid.to_string().parse::<Bytes32>()?,
                    index: input.prev_output.vout.to_u32(),
                },
                signature_script: input.sig_script.to_vec(),
                sequence: tondi_wire_sequence(input.sequence.to_consensus_u32()),
                sig_op_count: if input.sig_script.is_empty() { 0 } else { 1 },
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let outputs = tx
        .outputs
        .iter()
        .map(|output| {
            let script_public_key = ScriptPublicKey {
                version: output.script_pubkey_version,
                script: output.script_pubkey.to_vec(),
            };
            script_public_key
                .validate_p2mr_root()
                .map_err(|err| anyhow!("{err}"))?;
            Ok(TondiTxOutput::Standard {
                value: output.value.sats(),
                script_public_key,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut subnetwork_id = [0u8; 32];
    subnetwork_id[0..20].copy_from_slice(&tx.subnetwork_id);
    let native = TondiTransaction {
        version: tx.version.to_consensus_u16(),
        inputs,
        outputs,
        lock_time: tx.lock_time.to_consensus_u64(),
        subnetwork_id: Bytes32::from_byte_array(subnetwork_id),
        gas: tx.gas,
        payload: Vec::new(),
        mass: tx.mass,
    };
    native
        .validate_tondi_native_rgb()
        .map_err(|err| anyhow!("{err}"))?;
    Ok(native)
}

fn encode_p2mr_witness_stack(items: impl IntoIterator<Item = Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    let items = items.into_iter().collect::<Vec<_>>();
    let mut bytes = Vec::new();
    append_le_u32(
        &mut bytes,
        u32::try_from(items.len()).context("too many witness items")?,
    );
    for item in items {
        append_le_u32(
            &mut bytes,
            u32::try_from(item.len()).context("witness item too large")?,
        );
        bytes.extend_from_slice(&item);
    }
    Ok(bytes)
}

fn sha256_digest(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    digest.into()
}

fn sha256_tagged_hash(tag: &str, msg: &[u8]) -> [u8; 32] {
    let tag_hash = sha256_digest(tag.as_bytes());
    let mut hasher = Sha256::new();
    hasher.update(tag_hash);
    hasher.update(tag_hash);
    hasher.update(msg);
    hasher.finalize().into()
}

fn append_le_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn append_le_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn append_le_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn append_compact_size(bytes: &mut Vec<u8>, len: usize, what: &str) -> anyhow::Result<()> {
    let len = u64::try_from(len).with_context(|| format!("{what} exceeds u64"))?;
    match len {
        0..=0xFC => bytes.push(len as u8),
        0xFD..=0xFFFF => {
            bytes.push(0xFD);
            append_le_u16(bytes, len as u16);
        }
        0x10000..=0xFFFF_FFFF => {
            bytes.push(0xFE);
            append_le_u32(bytes, len as u32);
        }
        _ => {
            bytes.push(0xFF);
            append_le_u64(bytes, len);
        }
    }
    Ok(())
}

fn tondi_wire_sequence(sequence: u32) -> u64 {
    if sequence == u32::MAX {
        u64::MAX
    } else {
        sequence as u64
    }
}
