//! Current Tondi P2MR script-path signing (Default hash type, no annex).
use crate::{
    tx::{tondi_native_txid_bytes, tondi_sequence_from_bp},
    Tx, TxOut, TxVer, TONDI_P2MR_SCRIPT_VERSION,
};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2mrSigningError;
impl std::fmt::Display for P2mrSigningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid current P2MR transaction or input context")
    }
}
impl std::error::Error for P2mrSigningError {}

/// Commits all input kinds, outpoints, input Datum and outputs, matching the node.
/// The supplied prevouts must be authenticated by the caller against its node.
pub fn tondi_p2mr_script_sighash(
    tx: &Tx,
    prevouts: &[TxOut],
    index: usize,
    leaf_hash: [u8; 32],
) -> Result<[u8; 32], P2mrSigningError> {
    if tx.version != TxVer::CURRENT
        || tx.inputs.len() != prevouts.len()
        || index >= tx.inputs.len()
        || tx.inputs[index].is_reference
        || prevouts[index].script_pubkey_version != TONDI_P2MR_SCRIPT_VERSION
    {
        return Err(P2mrSigningError);
    }
    let mut points = Vec::new();
    let mut amounts = Vec::new();
    let mut scripts = Vec::new();
    let mut sequences = Vec::new();
    let mut outputs = Vec::new();
    for (input, prevout) in tx.inputs.iter().zip(prevouts) {
        if input.is_reference
            && (!input.sig_script.is_empty()
                || !input.witness.is_empty()
                || input.sig_op_count != 0
                || input.sequence.to_consensus_u32() != 0)
        {
            return Err(P2mrSigningError);
        }
        points.push(u8::from(input.is_reference));
        points.extend(tondi_native_txid_bytes(input.prev_output.txid));
        points.extend(input.prev_output.vout.to_u32().to_le_bytes());
        sequences.extend(tondi_sequence_from_bp(input.sequence).to_le_bytes());
        amounts.extend(prevout.value.sats().to_le_bytes());
        encode_script_datum(&mut scripts, prevout);
    }
    for output in &tx.outputs {
        outputs.extend(output.value.sats().to_le_bytes());
        encode_script_datum(&mut outputs, output);
    }
    let mut message = vec![0, 0];
    message.extend(tx.version.to_consensus_u16().to_le_bytes());
    message.extend(tx.lock_time.to_consensus_u64().to_le_bytes());
    for value in [&points, &amounts, &scripts, &sequences, &outputs] {
        message.extend(Sha256::digest(value));
    }
    message.push(2);
    message.extend((index as u32).to_le_bytes());
    message.extend(leaf_hash);
    message.push(0);
    message.extend(u32::MAX.to_le_bytes());
    let tag = Sha256::digest(b"TapSighash");
    let mut h = Sha256::new();
    h.update(tag);
    h.update(tag);
    h.update(message);
    Ok(h.finalize().into())
}
fn encode_script_datum(bytes: &mut Vec<u8>, output: &TxOut) {
    bytes.extend(output.script_pubkey_version.to_le_bytes());
    compact(bytes, output.script_pubkey.len());
    bytes.extend(output.script_pubkey.as_slice());
    bytes.push(u8::from(output.datum.is_some()));
    if let Some(data) = &output.datum {
        compact(bytes, data.len());
        bytes.extend(data.as_slice());
    }
}
fn compact(bytes: &mut Vec<u8>, len: usize) {
    match len {
        0..=0xfc => bytes.push(len as u8),
        0xfd..=0xffff => {
            bytes.push(0xfd);
            bytes.extend((len as u16).to_le_bytes());
        }
        0x10000..=0xffffffff => {
            bytes.push(0xfe);
            bytes.extend((len as u32).to_le_bytes());
        }
        _ => {
            bytes.push(0xff);
            bytes.extend((len as u64).to_le_bytes());
        }
    }
}
