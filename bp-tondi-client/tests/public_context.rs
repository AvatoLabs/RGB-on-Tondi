use amplify::Bytes32;
use bp::Tx;
use tondi::{
    PublicDataKind, PublicDatum, PublicReference, TondiOutpoint, TondiTransaction,
    UtxosByAddressEntry,
};
use tondi_vm::{
    builder::{DraftInput, NativeDraft},
    ClockContext, Hash32, OutPoint, ResolvedInput, StateNote,
};

fn draft() -> NativeDraft {
    let auth = tondi_vm::public_auth().0;
    NativeDraft {
        network: tondi::NetworkType::Devnet.domain(),
        inputs: vec![DraftInput {
            outpoint: OutPoint {
                transaction_id: Hash32::from_bytes([7; 32]),
                index: 0,
            },
            entry: ResolvedInput {
                note: StateNote::plain(2_000_000, auth),
                birth_daa_score: 10,
                is_coinbase: false,
            },
            auth: None,
            since: 0,
            consume: true,
        }],
        outputs: vec![StateNote::plain(1_900_000, auth)],
        programs: vec![],
        payload: vec![],
        cycles_limit: 100_000_000,
        invocation_cycles_limit: 1_000_000,
    }
}
fn context() -> (Tx, UtxosByAddressEntry) {
    let mut creation = draft();
    creation.outputs[0] = PublicDatum {
        kind: PublicDataKind::Law,
        data: b"explicit public law bytes".to_vec(),
    }
    .note(1_900_000, tondi_vm::public_auth().0)
    .unwrap();
    let creation = Tx::from_native(creation.prepare().unwrap().transaction).unwrap();
    let entry = UtxosByAddressEntry {
        address: None,
        outpoint: TondiOutpoint {
            transaction_id: creation.txid().to_string().parse::<Bytes32>().unwrap(),
            index: 0,
        },
        utxo_entry: ResolvedInput {
            note: creation.outputs[0].clone(),
            birth_daa_score: 15,
            is_coinbase: false,
        },
    };
    (creation, entry)
}
#[test]
fn reference_preserves_complete_note_and_cannot_fund_the_transaction() {
    let (_, entry) = context();
    let pin = PublicReference::from_live(&entry).unwrap();
    let mut anchor = draft();
    pin.append_to(&mut anchor, &entry).unwrap();
    assert!(pin.append_to(&mut anchor, &entry).is_err());
    let prepared = anchor.prepare().unwrap();
    assert!(!prepared.transaction.inputs[1].consumes());
    let report = tondi_vm::TondiVm::new(prepared.transaction.network)
        .validate(
            &prepared.transaction,
            &prepared.resolved_inputs,
            ClockContext {
                daa_score: 100,
                time_seconds: 1000,
            },
        )
        .unwrap();
    assert_eq!(report.validated.fee, 100_000);
    let tx = Tx::from_native(prepared.transaction).unwrap();
    assert_eq!(
        TondiTransaction::from_bp_tx(&tx)
            .unwrap()
            .to_bp_tx()
            .unwrap(),
        tx
    );
    let mut changed = entry.clone();
    changed.utxo_entry.note.state.as_mut().unwrap().datum[1] ^= 1;
    assert!(pin.authenticate(&changed).is_err());
    let mut changed = entry.clone();
    changed.utxo_entry.note.value += 1;
    assert!(pin.authenticate(&changed).is_err());
    let mut changed = entry;
    changed.outpoint.index = 1;
    assert!(pin.authenticate(&changed).is_err());
}
#[test]
fn historical_reference_requires_exact_creation_and_reference_kind() {
    let (creation, entry) = context();
    let pin = PublicReference::from_live(&entry).unwrap();
    let mut anchor = draft();
    pin.append_to(&mut anchor, &entry).unwrap();
    let anchor = Tx::from_native(anchor.prepare().unwrap().transaction).unwrap();
    pin.verify_witness(&creation, &anchor).unwrap();
    let mut native = anchor.into_native();
    native.body.inputs[1] = tondi_vm::Input::consume(pin.outpoint.into(), 0);
    assert!(pin
        .verify_witness(&creation, &Tx::from_native(native).unwrap())
        .is_err());
}
#[test]
fn public_data_is_bounded_and_domain_separated() {
    let datum = PublicDatum {
        kind: PublicDataKind::Metadata,
        data: vec![1; 4095],
    };
    assert_eq!(datum.encode().unwrap().len(), 4096);
    assert!(PublicDatum {
        kind: PublicDataKind::Metadata,
        data: vec![1; 4096]
    }
    .encode()
    .is_err());
    assert!(PublicDatum::decode(b"RGBD\0\x01x").is_err());
    let mut note = datum.note(10_000_000, tondi_vm::public_auth().0).unwrap();
    note.state.as_mut().unwrap().law.args = b"unrelated state domain".to_vec();
    assert!(PublicDatum::from_note(&note).is_err());
}
