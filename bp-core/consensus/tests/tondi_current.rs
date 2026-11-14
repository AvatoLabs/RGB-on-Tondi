use amplify::hex::{FromHex, ToHex};
use amplify::ByteArray;
use bc::{
    tondi_p2mr_script_sighash, ConsensusDecode, ConsensusEncode, Datum, LockTime, Outpoint, Sats,
    ScriptPubkey, SeqNo, SigScript, Tx, TxIn, TxOut, TxVer, VarIntArray, VarIntBytes, Witness,
};

fn fixture_tx(v: &serde_json::Value) -> (Tx, Vec<TxOut>) {
    let root = Vec::<u8>::from_hex(v["root"].as_str().unwrap()).unwrap();
    let datum: Option<Vec<u8>> = serde_json::from_value(v["datum"].clone()).unwrap();
    let datum = datum.map(Datum::from_checked);
    let mut input = TxIn {
        is_reference: false,
        sig_op_count: 1,
        prev_output: Outpoint::new(
            "1111111111111111111111111111111111111111111111111111111111111111".parse().unwrap(),
            7u32,
        ),
        sig_script: SigScript::new(),
        sequence: SeqNo::from_consensus_u32(u32::MAX),
        witness: Witness::new(),
    };
    let mut inputs = vec![input.clone()];
    let mut prevout = TxOut::p2mr(ScriptPubkey::from_checked(root.clone()), 5000u64);
    prevout.datum = datum.clone();
    let mut prevouts = vec![prevout.clone()];
    if v["reference"].as_bool().unwrap() {
        input.is_reference = true;
        input.sig_op_count = 0;
        input.sequence = SeqNo::from_consensus_u32(0);
        input.prev_output = Outpoint::new(
            "2222222222222222222222222222222222222222222222222222222222222222".parse().unwrap(),
            3u32,
        );
        inputs.push(input);
        prevout.value = Sats::from_sats(9000u64);
        prevout.datum = Some(Datum::from_checked(b"policy".to_vec()));
        prevouts.push(prevout);
    }
    let mut output = TxOut::p2mr(ScriptPubkey::from_checked(root), 42u64);
    output.datum = datum;
    (
        Tx {
            version: TxVer::CURRENT,
            inputs: VarIntArray::from_checked(inputs),
            outputs: VarIntArray::from_checked(vec![output]),
            lock_time: LockTime::from_consensus_u64(9),
            subnetwork_id: [0; 20],
            gas: 0,
            payload: VarIntBytes::new(),
            mass: 0,
        },
        prevouts,
    )
}
#[test]
fn matches_current_node_vectors_including_references_and_bounded_datum() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("data/tondi_current.json")).unwrap();
    for v in vectors.as_array().unwrap() {
        let (tx, prevouts) = fixture_tx(v);
        assert_eq!(tx.txid().to_string(), v["txid"].as_str().unwrap(), "{} id", v["name"]);
        let bytes = tx.consensus_serialize();
        assert_eq!(bytes.to_hex(), v["borsh"].as_str().unwrap(), "{} Borsh", v["name"]);
        assert_eq!(Tx::consensus_deserialize(&bytes).unwrap(), tx);
        let leaf = <[u8; 32]>::from_hex(v["leafHash"].as_str().unwrap()).unwrap();
        assert_eq!(
            tondi_p2mr_script_sighash(&tx, &prevouts, 0, leaf).unwrap().to_hex(),
            v["sighash"].as_str().unwrap(),
            "{} signing",
            v["name"]
        );
        let generic = bc::SighashCache::new(&tx, prevouts.clone())
            .unwrap()
            .tap_sighash_script(0, bc::TapLeafHash::from_byte_array(leaf), None)
            .unwrap();
        assert_eq!(generic.to_byte_array().to_hex(), v["sighash"].as_str().unwrap());
        assert!(Tx::consensus_deserialize(&bytes[..bytes.len() - 1]).is_err());
        let mut unsupported = bytes.clone();
        unsupported[..2].copy_from_slice(&0u16.to_le_bytes());
        assert!(Tx::consensus_deserialize(unsupported).is_err());
    }
}
#[test]
fn changing_reference_kind_or_datum_changes_signatures() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("data/tondi_current.json")).unwrap();
    let v = &vectors[2];
    let (mut tx, mut prevouts) = fixture_tx(v);
    let leaf = <[u8; 32]>::from_hex(v["leafHash"].as_str().unwrap()).unwrap();
    let signed = tondi_p2mr_script_sighash(&tx, &prevouts, 0, leaf).unwrap();
    prevouts[1].datum = Some(Datum::from_checked(b"different policy".to_vec()));
    assert_ne!(tondi_p2mr_script_sighash(&tx, &prevouts, 0, leaf).unwrap(), signed);
    prevouts[1].datum = Some(Datum::from_checked(b"policy".to_vec()));
    tx.inputs[1].is_reference = false;
    assert_ne!(tondi_p2mr_script_sighash(&tx, &prevouts, 0, leaf).unwrap(), signed);
}

#[test]
fn unrepresentable_native_sequences_are_rejected_without_changing_identity() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("data/tondi_current.json")).unwrap();
    let (tx, _) = fixture_tx(&vectors[0]);
    let bytes = tx.inputs[0].consensus_serialize();
    for sequence in [u32::MAX as u64, u32::MAX as u64 + 1, u64::MAX - 1] {
        let mut encoded = bytes.clone();
        encoded[37..45].copy_from_slice(&sequence.to_le_bytes());
        assert!(TxIn::consensus_deserialize(encoded).is_err());
    }
    assert_eq!(TxIn::consensus_deserialize(bytes).unwrap(), tx.inputs[0]);
}
