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

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn base() { parse_roundtrip(include_str!("valid.v2/base.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn updated() { parse_roundtrip(include_str!("valid.v2/updated.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn nseq() { parse_roundtrip(include_str!("valid.v2/nseq.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn locks() { parse_roundtrip(include_str!("valid.v2/locks.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn in_modifiable() { parse_roundtrip(include_str!("valid.v2/in_modifiable.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn out_modifiable() { parse_roundtrip(include_str!("valid.v2/out_modifiable.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn sighash_single() { parse_roundtrip(include_str!("valid.v2/sighash_single.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn undefined_flag() { parse_roundtrip(include_str!("valid.v2/undefined_flag.psbt")); }

#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn all_modifiable() { parse_roundtrip(include_str!("valid.v2/all_modifiable.psbt")); }

/// Case: 1 input, 2 output updated PSTTv2, with all PSTTv2 fields
#[test]
#[ignore = "legacy Bitcoin PSBT fixture; Tondi-native PSTT rejects PSBT magic"]
fn all() { parse_roundtrip(include_str!("valid.v2/all.psbt")); }
