use amplify::Bytes32;
use bp::{Datum, LockTime, ScriptPubkey, Tx, TxOut, TxVer, VarIntArray, VarIntBytes};
use tondi::{
    PublicDataKind, PublicDatum, PublicReference, ScriptPublicKey, TondiOutpoint, TondiTransaction,
    TondiUtxoEntry, UtxosByAddressEntry,
};
fn tx() -> Tx {
    Tx {
        version: TxVer::CURRENT,
        inputs: VarIntArray::new(),
        outputs: VarIntArray::from_checked(vec![TxOut::p2mr(
            ScriptPubkey::from_checked(vec![7; 32]),
            42u64,
        )]),
        lock_time: LockTime::ZERO,
        subnetwork_id: [0; 20],
        gas: 0,
        payload: VarIntBytes::new(),
        mass: 0,
    }
}
fn context() -> UtxosByAddressEntry {
    let datum = PublicDatum {
        kind: PublicDataKind::Policy,
        data: Datum::from_checked(b"explicit policy bytes".to_vec()),
    }
    .encode()
    .unwrap();
    UtxosByAddressEntry {
        address: None,
        outpoint: TondiOutpoint {
            transaction_id: Bytes32::from_byte_array([3; 32]),
            index: 0,
        },
        utxo_entry: TondiUtxoEntry {
            amount: 9000,
            script_public_key: ScriptPublicKey {
                version: 4,
                script: vec![7; 32],
            },
            block_daa_score: 0,
            is_coinbase: false,
            datum: Some(datum),
        },
    }
}
#[test]
fn public_reference_survives_current_rpc_bridge_and_does_not_supply_spending_value() {
    let entry = context();
    let pin = PublicReference::from_live(&entry).unwrap();
    let mut tx = tx();
    let previous = pin.append_to(&mut tx, &entry).unwrap();
    assert_eq!(previous.value.sats(), 9000);
    assert!(tx.inputs[0].is_reference);
    assert_eq!(tx.inputs[0].sig_op_count, 0);
    let native = TondiTransaction::from_bp_tx(&tx).unwrap();
    let restored = native.to_bp_tx().unwrap();
    assert_eq!(restored, tx);
    assert!(pin.append_to(&mut tx, &entry).is_err());
    let mut changed = entry.clone();
    changed.utxo_entry.datum.as_mut().unwrap()[6] ^= 1;
    assert!(pin.authenticate(&changed).is_err());
    changed = entry;
    changed.outpoint.index = 1;
    assert!(pin.authenticate(&changed).is_err());
}

#[test]
fn bridge_rejects_sequences_it_cannot_preserve_and_non_native_anchors() {
    let mut native = TondiTransaction::from_bp_tx(&tx()).unwrap();
    native.inputs.push(tondi::TondiTxInput::Standard {
        previous_outpoint: context().outpoint,
        signature_script: vec![],
        sequence: u32::MAX as u64,
        sig_op_count: 1,
    });
    assert!(native.to_bp_tx().is_err());
    let tondi::TondiTxInput::Standard { sequence, .. } = &mut native.inputs[0] else {
        unreachable!()
    };
    *sequence = u64::MAX;
    assert!(native.to_bp_tx().is_ok());
    native.gas = 1;
    assert!(native.to_bp_tx().is_err());
}
#[test]
fn historical_reference_witness_remains_verifiable_after_the_live_lookup_is_gone() {
    let mut entry = context();
    let mut creation = tx();
    creation.outputs[0].datum = entry.utxo_entry.datum.clone();
    creation.outputs[0].value = 9000u64.into();
    entry.outpoint.transaction_id = creation.txid().to_string().parse().unwrap();
    let pin = PublicReference::from_live(&entry).unwrap();
    let mut anchor = tx();
    pin.append_to(&mut anchor, &entry).unwrap();
    pin.verify_witness(&creation, &anchor).unwrap();
    anchor.inputs[0].is_reference = false;
    assert!(pin.verify_witness(&creation, &anchor).is_err());
}
#[test]
fn public_data_is_bounded_explicit_and_versioned() {
    let datum = PublicDatum {
        kind: PublicDataKind::Metadata,
        data: Datum::from_checked(vec![1; 4090]),
    };
    assert_eq!(datum.encode().unwrap().len(), 4096);
    assert!(PublicDatum {
        kind: PublicDataKind::Metadata,
        data: Datum::from_checked(vec![1; 4091])
    }
    .encode()
    .is_err());
    assert!(PublicDatum::decode(b"RGBD\0\x01x").is_err());
    assert!(PublicDatum::decode(b"private consignment").is_err());
}
