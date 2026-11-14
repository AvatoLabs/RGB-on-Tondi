use amplify::ByteArray;
use bc::{ConsensusDecode, ConsensusEncode, Tx};
use strict_encoding::{StrictDeserialize, StrictSerialize};
use tondi_vm::{
    builder::{DraftInput, NativeDraft},
    *,
};
fn fixture() -> Transition {
    let auth = public_auth().0;
    let definition = assets::AssetDefinition {
        genesis: OutPoint {
            transaction_id: Hash32::from_bytes([8; 32]),
            index: 0,
        },
        nonce: 1,
        max_issued: u128::MAX,
        metadata: assets::AssetMetadata {
            name: "Native".into(),
            symbol: "NTV".into(),
            decimals: 0,
            uri: String::new(),
        },
        transfer_law: None,
    };
    let asset = AssetBalance {
        asset_id: definition.id(),
        amount: u128::MAX,
    };
    let mut note = StateNote::plain(10_000_000, auth);
    note.assets.push(asset.clone());
    let input = ResolvedInput {
        note,
        birth_daa_score: 1u64 << 40,
        is_coinbase: false,
    };
    let mut output = StateNote::plain(9_900_000, auth);
    output.assets.push(asset);
    output.state = Some(StatePayload {
        law: LawInstance {
            program: ProgramRef::Standard(LAW_DATA),
            args: b"test".to_vec(),
        },
        datum: vec![0xab; 4096],
    });
    NativeDraft {
        network: Hash32::from_bytes([3; 32]),
        inputs: vec![
            DraftInput {
                outpoint: OutPoint {
                    transaction_id: Hash32::from_bytes([5; 32]),
                    index: 9,
                },
                entry: input.clone(),
                auth: None,
                since: 1u64 << 40,
                consume: true,
            },
            DraftInput {
                outpoint: OutPoint {
                    transaction_id: Hash32::from_bytes([6; 32]),
                    index: 3,
                },
                entry: ResolvedInput {
                    note: definition.definition_note(1_000_000),
                    birth_daa_score: 1,
                    is_coinbase: false,
                },
                auth: None,
                since: 0,
                consume: false,
            },
        ],
        outputs: vec![output],
        programs: vec![],
        payload: b"opaque action bytes".to_vec(),
        cycles_limit: 100_000_000,
        invocation_cycles_limit: 1_000_000,
    }
    .prepare()
    .unwrap()
    .transaction
}
#[test]
fn complete_native_wire_and_strict_frame_are_lossless() {
    let native = fixture();
    let tx = Tx::from_native(native.clone()).unwrap();
    assert_eq!(tx.txid().to_byte_array(), native.id().as_bytes());
    assert_eq!(tx.wtxid().to_string(), native.witness_id().to_string());
    assert_eq!(tx.consensus_serialize(), codec::encode(&native));
    assert_eq!(Tx::consensus_deserialize(tx.wire_bytes()).unwrap().native(), &native);
    let strict = tx.to_strict_serialized::<2_000_000>().unwrap();
    assert_eq!(Tx::from_strict_serialized(strict).unwrap(), tx);
    assert_eq!(tx.inputs[0].since(), Some(1u64 << 40));
    assert!(!tx.inputs[1].consumes());
    assert_eq!(tx.outputs[0].assets[0].amount, u128::MAX);
    let text = serde_json::to_string(&tx).unwrap();
    assert!(text.contains(&u128::MAX.to_string()));
    assert_eq!(serde_json::from_str::<Tx>(&text).unwrap(), tx);
}
#[test]
fn unsigned_removes_only_signature_bytes_and_rejects_obsolete_wire() {
    let mut native = fixture();
    native.witness.data[0].signatures = vec![4; 64];
    let tx = Tx::from_native(native.clone()).unwrap();
    let unsigned = tx.to_unsigned_tx();
    assert_eq!(unsigned.txid(), tx.txid());
    assert_ne!(unsigned.wtxid(), tx.wtxid());
    for w in &mut native.witness.data {
        w.signatures.clear();
    }
    assert_eq!(unsigned.native(), &native);
    let mut old = tx.consensus_serialize();
    old[..2].copy_from_slice(&1u16.to_le_bytes());
    assert!(Tx::consensus_deserialize(old).is_err());
    let mut trailing = tx.consensus_serialize();
    trailing.push(0);
    assert!(Tx::consensus_deserialize(trailing).is_err());
    assert!(Tx::consensus_deserialize(&tx.wire_bytes()[..tx.wire_bytes().len() - 1]).is_err());
}
