use derive::{
    Datum, LockTime, Outpoint, Sats, ScriptPubkey, SeqNo, SigScript, Tx, TxIn, TxOut, TxVer,
    VarIntArray, VarIntBytes, Witness,
};
use pstt::{Pstt, PsttVer};
#[test]
fn datum_reference_kind_and_fee_exclusion_survive_pstt_transport() {
    let mut spent = TxOut::p2mr(ScriptPubkey::from_checked(vec![1; 32]), 5000u64);
    spent.datum = Some(Datum::from_checked(vec![]));
    let mut read = spent.clone();
    read.value = Sats::from_sats(9000u64);
    read.datum = Some(Datum::from_checked(b"policy".to_vec()));
    let spending = TxIn {
        is_reference: false,
        sig_op_count: 1,
        prev_output: Outpoint::new(
            "1111111111111111111111111111111111111111111111111111111111111111".parse().unwrap(),
            0u32,
        ),
        sig_script: SigScript::new(),
        sequence: SeqNo::from_consensus_u32(0),
        witness: Witness::new(),
    };
    let mut reference = spending.clone();
    reference.prev_output.vout = 1u32.into();
    reference.is_reference = true;
    reference.sig_op_count = 0;
    let mut out = spent.clone();
    out.value = Sats::from_sats(1000u64);
    out.datum = Some(Datum::from_checked(vec![3; 1024]));
    let tx = Tx {
        version: TxVer::CURRENT,
        inputs: VarIntArray::from_checked(vec![spending, reference]),
        outputs: VarIntArray::from_checked(vec![out]),
        lock_time: LockTime::ZERO,
        subnetwork_id: [0; 20],
        gas: 0,
        payload: VarIntBytes::new(),
        mass: 0,
    };
    let mut pstt = Pstt::from_tx(tx.clone());
    pstt.input_mut(0).unwrap().witness_utxo = Some(spent);
    pstt.input_mut(1).unwrap().witness_utxo = Some(read);
    let bytes = pstt.serialize(PsttVer::V0);
    let mut decoded = Pstt::deserialize(bytes).unwrap();
    assert_eq!(decoded.input_sum().sats(), 5000);
    assert_eq!(decoded.fee().unwrap().sats(), 4000);
    assert_eq!(Tx::from(decoded.to_unsigned_tx()), tx);
    assert!(decoded.input(1).unwrap().is_finalized());
    decoded.input_mut(0).unwrap().final_script_sig = Some(SigScript::from_checked(vec![1]));
    let extracted = decoded.extract().unwrap();
    assert!(extracted.inputs[1].is_reference);
    assert_eq!(extracted.outputs[0].datum, tx.outputs[0].datum);
}
