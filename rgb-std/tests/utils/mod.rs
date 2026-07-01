use std::collections::BTreeSet;
use std::convert::Infallible;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use aluvm::{aluasm, Lib};
use amplify::confinement::{Confined, SmallOrdSet, U24 as U24MAX};
use amplify::none;
use binfile::BinFile;
use bp::seals::{Anchor, TxoSeal, WTxoSeal};
use bp::{LockTime, Tx, VarIntBytes};
use commit_verify::{DigestExt, TondiBlake3};
use hypersonic::CallParams;
use rand::prelude::SliceRandom;
use rand::rng;
use rgb::{
    Assignment, CellAddr, Codex, Contract, CoreParams, CreateParams, Issuer, NamedState, Outpoint,
    Semantics, ISSUER_MAGIC_NUMBER, ISSUER_VERSION,
};
use rgb_persist_fs::{PileFs, StockFs};
use rgbcore::{ContractApi, RgbSealDef};
use single_use_seals::SealWitness;
use strict_encoding::{vname, StreamReader, StrictDecode, StrictDumb};

pub fn setup(name: &str) -> Contract<StockFs, PileFs<TxoSeal>> {
    let mut noise_engine = TondiBlake3::default();
    noise_engine.input_raw(b"test");

    let issuer = load_test_issuer();

    let mut params = CreateParams::new_tondi_testnet(issuer.codex_id(), "Test");
    for _ in 0..20 {
        params.push_owned_unlocked(
            "amount",
            Assignment::new_internal(Outpoint::strict_dumb(), 100u64),
        );
    }

    let contract_path = PathBuf::from(format!("tests/data/{name}.contract"));
    if contract_path.exists() {
        fs::remove_dir_all(&contract_path).expect("Unable to remove a contract file");
    }
    fs::create_dir_all(&contract_path).expect("Unable to create a contract folder");
    let mut contract =
        Contract::issue(issuer, params.transform(noise_engine.clone()), |_| Ok(contract_path))
            .unwrap();
    let opid = contract.articles().genesis_opid();

    let owned = &contract.full_state().main.owned;
    assert_eq!(owned.len(), 1);
    let owned = owned.get("amount").unwrap();
    assert_eq!(owned.len(), 20);
    let mut prev = vec![];
    for (addr, val) in owned {
        assert_eq!(val, &svnum!(100u64));
        assert_eq!(addr.opid, opid);
        prev.push(*addr);
    }
    assert_eq!(prev.len(), 20);

    let mut no = 0;
    let mut next_seal = || -> WTxoSeal {
        no += 1;
        WTxoSeal::vout_no_fallback(no.into(), noise_engine.clone(), no as u64)
    };

    let params = CallParams {
        core: CoreParams { method: vname!("transfer"), global: none!(), owned: none!() },
        using: none!(),
        reading: none!(),
    };
    let mut tx = Tx {
        version: default!(),
        inputs: Confined::from_checked(vec![]),
        outputs: Confined::from_checked(vec![]),
        lock_time: default!(),
        subnetwork_id: [0u8; 20],
        gas: 0,
        payload: VarIntBytes::new(),
        mass: 0,
    };
    let anchor = Anchor::strict_dumb();
    for round in 0u16..10 {
        // shuffle outputs to create twisted DAG
        prev.shuffle(&mut rng());
        let mut iter = prev.into_iter();
        let mut new_prev = vec![];
        while let Some((first, second)) = iter.next().zip(iter.next()) {
            let mut params = params.clone();
            params.using.insert(first, None);
            params.using.insert(second, None);
            let seals = small_bmap![0 => next_seal(), 1 => next_seal()];
            let amount = 100u64 - round as u64;
            params.core.owned.push(NamedState::new_unlocked(
                "amount",
                seals[&0].auth_token(),
                amount,
            ));
            params.core.owned.push(NamedState::new_unlocked(
                "amount",
                seals[&1].auth_token(),
                amount,
            ));
            let op = contract.call(params, seals).unwrap();
            let opid = op.opid();
            new_prev.push(CellAddr::new(opid, 0));
            new_prev.push(CellAddr::new(opid, 1));

            tx.lock_time = LockTime::from_consensus_u32(tx.lock_time.into_consensus_u32() + 1);
            contract.apply_witness(opid, SealWitness::new(tx.clone(), anchor.clone()));
        }
        prev = new_prev;
    }

    let owned = &contract.full_state().main.owned;
    assert_eq!(owned.len(), 1);
    assert_eq!(prev.len(), 20);
    let owned = owned.get("amount").unwrap();
    assert_eq!(owned.len(), 20);
    for (_, val) in owned.iter() {
        assert_eq!(val, &svnum!(91u64));
    }
    assert_eq!(owned.keys().collect::<BTreeSet<_>>(), prev.iter().collect::<BTreeSet<_>>());

    contract
}

fn load_test_issuer() -> Issuer {
    let path = "tests/data/Test.issuer";
    if let Ok(issuer) = Issuer::load(path, |_, _, _| -> Result<_, Infallible> { unreachable!() }) {
        return issuer;
    }

    let file = BinFile::<ISSUER_MAGIC_NUMBER, ISSUER_VERSION>::open(path).unwrap();
    let mut reader = StreamReader::new::<U24MAX>(file);
    let mut codex = Codex::strict_read(&mut reader).unwrap();
    let mut semantics = Semantics::strict_read(&mut reader).unwrap();
    let verifier = Lib::assemble(&aluasm! { stop; }).unwrap();
    let verifier_id = verifier.lib_id();
    for site in codex.verifiers.values_mut() {
        site.lib_id = verifier_id;
        site.offset = 0;
    }

    let codex_id = codex.codex_id();
    semantics.default.codex_id = codex_id;
    for api in semantics.custom.values_mut() {
        api.codex_id = codex_id;
    }
    semantics.codex_libs =
        SmallOrdSet::try_from_iter(semantics.codex_libs.iter().cloned().chain([verifier])).unwrap();

    let _sig = Option::<rgb::SigBlob>::strict_read(&mut reader).unwrap();
    match reader.unconfine().read_exact(&mut [0u8; 1]) {
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {}
        Err(err) => panic!("failed to read test issuer fixture: {err}"),
        Ok(()) => panic!("test issuer fixture has trailing data"),
    }

    Issuer::new(codex, semantics).unwrap()
}
