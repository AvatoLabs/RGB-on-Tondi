use bc::secp256k1::{Keypair, Secp256k1, SecretKey};
use pstt::{Pstt, PsttVer};
use tondi_vm::{
    assets::{AssetDefinition, AssetMetadata},
    builder::{AuthDescriptor, DraftInput, NativeDraft},
    *,
};
use tondi_wallet_pstt::extensions::rgb::{RgbAnchor, RgbSidecar};
fn fixture(since: u64) -> Pstt {
    let key =
        Keypair::from_secret_key(&Secp256k1::new(), &SecretKey::from_slice(&[7; 32]).unwrap())
            .x_only_public_key()
            .0
            .serialize();
    let (auth, leaf) = schnorr_auth(key);
    let definition = AssetDefinition {
        genesis: OutPoint {
            transaction_id: Hash32::from_bytes([8; 32]),
            index: 0,
        },
        nonce: 1,
        max_issued: u128::MAX,
        metadata: AssetMetadata {
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
    let mut consumed = StateNote::plain(2_000_000, auth);
    consumed.assets.push(asset.clone());
    let sidecar = RgbSidecar {
        network: Hash32::from_bytes([3; 32]),
        transition_bundles: b"canonical client-side operation bundle".to_vec(),
    };
    let mut output = RgbAnchor::new(Hash32::from_bytes([9; 32]), sidecar.id().unwrap())
        .note(1_900_000, auth)
        .unwrap();
    output.assets.push(asset);
    let native = NativeDraft {
        network: sidecar.network,
        inputs: vec![
            DraftInput {
                outpoint: OutPoint {
                    transaction_id: Hash32::from_bytes([5; 32]),
                    index: 9,
                },
                entry: ResolvedInput {
                    note: consumed,
                    birth_daa_score: 10,
                    is_coinbase: false,
                },
                auth: Some(AuthDescriptor {
                    leaf,
                    merkle_path: vec![],
                }),
                since,
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
        payload: b"RGB + native asset transfer".to_vec(),
        cycles_limit: 100_000_000,
        invocation_cycles_limit: 1_000_000,
    }
    .prepare()
    .unwrap();
    Pstt::from_native(tondi_wallet_pstt::Pstt {
        transaction: native.transaction,
        resolved_inputs: native.resolved_inputs,
    })
    .unwrap()
}
#[test]
fn signed_roundtrip_keeps_native_assets_state_references_and_body_identity() {
    let mut p = fixture(0);
    let id = p.txid().unwrap();
    let body = p.native().unwrap().transaction.body.clone();
    assert_eq!(p.sign(&[[7; 32]]).unwrap(), 1);
    assert_eq!(p.native().unwrap().transaction.body, body);
    assert_eq!(p.txid().unwrap(), id);
    let tx = p
        .extract(ClockContext {
            daa_score: 100,
            time_seconds: 1000,
        })
        .unwrap();
    assert_eq!(tx.outputs[0].assets[0].amount, u128::MAX);
    assert!(!tx.inputs[1].consumes());
    assert!(RgbAnchor::from_note(&tx.outputs[0]).unwrap().is_some());
    let mut bytes = vec![];
    p.encode(PsttVer::V4, &mut bytes).unwrap();
    let copy = Pstt::decode(&mut bytes.as_slice()).unwrap();
    assert_eq!(copy.native().unwrap(), p.native().unwrap());
    assert_eq!(
        copy.extract(ClockContext {
            daa_score: 100,
            time_seconds: 1000
        })
        .unwrap(),
        tx
    );
}
#[test]
fn imported_mutations_cannot_rebuild_a_prepared_body() {
    let mut p = fixture(0);
    p.output_mut(0).unwrap().note.value -= 1;
    assert!(p.sign(&[[7; 32]]).is_err());
    let mut p = fixture(0);
    p.inputs_mut().next().unwrap().entry.note.auth = public_auth().0;
    assert!(p.sign(&[[7; 32]]).is_err());
    let mut p = fixture(0);
    p.inputs_mut().next().unwrap().index = 4;
    assert!(p.validate_native().is_err());
    let mut p = fixture(0);
    p.inputs_mut().next().unwrap().since = 1;
    assert!(p.txid().is_err());
}
#[test]
fn maturity_is_checked_with_explicit_clock_without_truncating_since() {
    let since = 1u64 << 40;
    let mut p = fixture(since);
    p.sign(&[[7; 32]]).unwrap();
    assert!(p
        .extract(ClockContext {
            daa_score: 100,
            time_seconds: u64::MAX
        })
        .is_err());
    assert!(p
        .extract(ClockContext {
            daa_score: since,
            time_seconds: 0
        })
        .is_ok());
    let mut bytes = vec![];
    p.encode(PsttVer::V4, &mut bytes).unwrap();
    assert_eq!(Pstt::decode(&mut bytes.as_slice()).unwrap().input(0).unwrap().since, since);
}
#[test]
fn old_maps_unknown_fields_and_duplicate_metadata_are_rejected() {
    assert!(Pstt::decode(&mut &b"pstt\xff"[..]).is_err());
    let p = fixture(0);
    let mut value = serde_json::to_value(&p).unwrap();
    value["version"] = serde_json::json!("v2");
    assert!(Pstt::decode(&mut value.to_string().as_bytes()).is_err());
    let mut value = serde_json::to_value(&p).unwrap();
    value["mass"] = serde_json::json!(0);
    assert!(Pstt::decode(&mut value.to_string().as_bytes()).is_err());
    let mut value = serde_json::to_value(&p).unwrap();
    let pair = serde_json::json!([{ "identifier":"RGB", "subtype":1,"data":"" },""]);
    value["outputs"][0]["proprietary"] = serde_json::json!([pair.clone(), pair]);
    assert!(Pstt::decode(&mut value.to_string().as_bytes()).is_err());
}
