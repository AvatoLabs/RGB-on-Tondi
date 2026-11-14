// Tondi protocol consensus library.
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2019-2024 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2019-2024 LNP/BP Standards Association. All rights reserved.
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
// Coding conventions

use strict_types::{CompileError, LibBuilder, TypeLib};

use crate::{
    Bip340Sig, ByteStr, CompressedPk, ControlBlock, FutureLeafVer, InternalPk, LeafScript,
    LegacyPk, LegacySig, OpCode, Outpoint, OutputPk, PubkeyHash, RedeemScript, ScriptHash, TapCode,
    TapLeafHash, TapNodeHash, TapScript, TondiOutpoint, TondiTxid, Tx, Txid, UncompressedPk,
    VarInt, Vout, WPubkeyHash, WScriptHash, WitnessProgram, WitnessScript, WitnessVer, Wtxid,
    LIB_NAME_TONDI,
};

pub const LIB_ID_BP_TX: &str =
    "stl:ETMP1vno-AW24hmU-IolKRhA-eEwNsTc-AIM9gd8-_LaLPuc#fiona-fiction-yogurt";
pub const LIB_ID_BP_CONSENSUS: &str =
    "stl:ETMP1vno-AW24hmU-IolKRhA-eEwNsTc-AIM9gd8-_LaLPuc#fiona-fiction-yogurt";

#[allow(clippy::result_large_err)]
fn _bp_tx_stl() -> Result<TypeLib, CompileError> {
    _bp_consensus_stl()
}

#[allow(clippy::result_large_err)]
fn _bp_consensus_stl() -> Result<TypeLib, CompileError> {
    LibBuilder::with(libname!(LIB_NAME_TONDI), [strict_types::stl::std_stl().to_dependency_types()])
        .transpile::<LegacySig>()
        .transpile::<Bip340Sig>()
        .transpile::<OpCode>()
        .transpile::<PubkeyHash>()
        .transpile::<WPubkeyHash>()
        .transpile::<ScriptHash>()
        .transpile::<WScriptHash>()
        .transpile::<WitnessScript>()
        .transpile::<RedeemScript>()
        .transpile::<Wtxid>()
        .transpile::<WitnessProgram>()
        .transpile::<WitnessVer>()
        .transpile::<CompressedPk>()
        .transpile::<UncompressedPk>()
        .transpile::<LegacyPk>()
        .transpile::<InternalPk>()
        .transpile::<OutputPk>()
        .transpile::<TapNodeHash>()
        .transpile::<TapLeafHash>()
        .transpile::<FutureLeafVer>()
        .transpile::<LeafScript>()
        .transpile::<TapCode>()
        .transpile::<TapScript>()
        .transpile::<ControlBlock>()
        .transpile::<Tx>()
        .transpile::<VarInt>()
        .transpile::<ByteStr>()
        .transpile::<Vout>()
        .transpile::<Txid>()
        .transpile::<TondiTxid>()
        .transpile::<Outpoint>()
        .transpile::<TondiOutpoint>()
        .compile()
}

pub fn bp_tx_stl() -> TypeLib {
    _bp_tx_stl().expect("invalid strict type Tondi transaction library")
}

pub fn bp_consensus_stl() -> TypeLib {
    _bp_consensus_stl().expect("invalid strict type Tondi consensus library")
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use super::*;

    #[test]
    fn lib_id_tx() {
        let lib = bp_tx_stl();
        assert_eq!(lib.id().to_string(), LIB_ID_BP_TX);
    }

    #[test]
    fn lib_id_consensus() {
        let lib = bp_consensus_stl();
        assert_eq!(lib.id().to_string(), LIB_ID_BP_CONSENSUS);
    }
}
