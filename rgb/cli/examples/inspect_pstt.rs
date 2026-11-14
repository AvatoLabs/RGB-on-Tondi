// Wallet Library for RGB smart contracts
//
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::env;
use std::fs::File;

use bpstd::pstt::Pstt;

fn main() -> anyhow::Result<()> {
    let path = env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: inspect_pstt <file.pstt>"))?;
    let mut file = File::open(&path)?;
    let pstt = Pstt::decode(&mut file)?;

    let inputs = pstt.inputs().collect::<Vec<_>>();
    let unique_inputs = inputs
        .iter()
        .map(|input| input.previous_outpoint.to_string())
        .collect::<BTreeSet<_>>();
    let change_outputs = pstt
        .outputs()
        .filter(|output| output.terminal.is_some())
        .collect::<Vec<_>>();

    println!("input_count={}", inputs.len());
    println!("unique_input_count={}", unique_inputs.len());
    for input in &inputs {
        println!("input_outpoint={}", input.previous_outpoint);
        println!("input_value_sats={}", input.entry.note.value);
    }
    println!("output_count={}", pstt.outputs().count());
    println!("change_output_count={}", change_outputs.len());
    for output in change_outputs {
        println!("change_vout={}", output.index);
        println!("change_value_sats={}", output.note.value);
    }
    if let Ok(fee) = pstt.fee() {
        println!("fee_sats={}", fee.sats());
    }

    Ok(())
}
