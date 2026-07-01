// Modern, minimalistic & standard-compliant cold wallet library.
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2020-2024 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2020-2024 LNP/BP Standards Association. All rights reserved.
// Copyright (C) 2020-2024 Dr Maxim Orlovsky. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::str::FromStr;

use pstt::Pstt;

fn parse_roundtrip(s: &str) {
    let pstt = Pstt::from_str(s).unwrap();
    Pstt::from_str(&pstt.to_string()).unwrap();
}

/// Case: PSTT with one P2PKH input. Outputs are empty.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn pkh_outputless() { parse_roundtrip(include_str!("valid.v0/pkh_outputless.psbt")); }

/// Case: PSTT with one P2PKH input and one P2SH-P2WPKH input. First input is signed and finalized.
/// Outputs are empty.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn pkh_sh_wpkh_outputless() {
    parse_roundtrip(include_str!("valid.v0/pkh_sh_wpkh_outputless.psbt"));
}

/// Case: PSTT with one P2PKH input which has a non-final scriptSig and has a sighash type
/// specified. Outputs are empty.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn pkh_signed() { parse_roundtrip(include_str!("valid.v0/pkh_signed.psbt")); }

/// Case: PSTT with one P2PKH input and one P2SH-P2WPKH input both with non-final scriptSigs.
/// P2SH-P2WPKH input's redeemScript is available. Outputs filled.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn pkh_sh_wpkh() { parse_roundtrip(include_str!("valid.v0/pkh_sh_wpkh.psbt")); }

/// Case: PSTT with one P2SH-P2WSH input of a 2-of-2 multisig, redeemScript, witnessScript, and
/// keypaths are available. Contains one signature.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn sh_wsh() { parse_roundtrip(include_str!("valid.v0/sh_wsh.psbt")); }

/// Case: PSTT with one P2WSH input of a 2-of-2 multisig. witnessScript, keypaths, and global xpubs
/// are available. Contains no signatures. Outputs filled.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn wsh() { parse_roundtrip(include_str!("valid.v0/wsh.psbt")); }

/// Case: PSTT with unknown types in the inputs.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn unknown_keys() { parse_roundtrip(include_str!("valid.v0/unknown_keys.psbt")); }

/// Case: PSTT with `PSTT_GLOBAL_XPUB`.
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn xpubs() { parse_roundtrip(include_str!("valid.v0/xpubs.psbt")); }

/// Case: PSTT with global unsigned tx that has 0 inputs and 0 outputs
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn no_inputs_outputs() { parse_roundtrip(include_str!("valid.v0/no_inputs_outputs.psbt")); }

/// Case: PSTT with 0 inputs
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn no_inputs() { parse_roundtrip(include_str!("valid.v0/no_inputs.psbt")); }
