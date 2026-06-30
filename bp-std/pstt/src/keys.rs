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

use std::fmt::Debug;
use std::hash::Hash;

use crate::{Encode, KeyData, PsttVer, ValueData};

pub trait KeyType: Copy + Ord + Eq + Hash + Debug + 'static {
    const STANDARD: &'static [Self];
    fn unknown(val: u8) -> Self {
        debug_assert!(!Self::STANDARD.iter().map(Self::to_u8).any(|x| x == val));
        Self::unknown_unchecked(val)
    }
    fn unknown_unchecked(val: u8) -> Self;
    fn from_u8(val: u8) -> Self;
    fn into_u8(self) -> u8;
    fn to_u8(&self) -> u8 {
        self.into_u8()
    }
    fn has_key_data(self) -> bool;
    fn present_since(self) -> PsttVer;
    fn deprecated_since(self) -> Option<PsttVer>;
    #[inline]
    fn is_allowed(self, version: PsttVer) -> bool {
        version >= self.present_since()
            && !matches!(self.deprecated_since(), Some(depr) if version >= depr)
    }
    fn is_required(self) -> bool;
    fn is_proprietary(self) -> bool;
}

const PSTT_GLOBAL_UNSIGNED_TX: u8 = 0x00;
const PSTT_GLOBAL_XPUB: u8 = 0x01;
const PSTT_GLOBAL_TX_VERSION: u8 = 0x02;
const PSTT_GLOBAL_FALLBACK_LOCKTIME: u8 = 0x03;
const PSTT_GLOBAL_INPUT_COUNT: u8 = 0x04;
const PSTT_GLOBAL_OUTPUT_COUNT: u8 = 0x05;
const PSTT_GLOBAL_TX_MODIFIABLE: u8 = 0x06;
const PSTT_GLOBAL_VERSION: u8 = 0xFB;
const PSTT_GLOBAL_PROPRIETARY: u8 = 0xFC;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum GlobalKey {
    /// `PSTT_GLOBAL_UNSIGNED_TX`
    UnsignedTx,

    /// `PSTT_GLOBAL_XPUB`
    Xpub,

    /// `PSTT_GLOBAL_TX_VERSION`
    TxVersion,

    /// `PSTT_GLOBAL_FALLBACK_LOCKTIME`
    FallbackLocktime,

    /// `PSTT_GLOBAL_INPUT_COUNT`
    InputCount,

    /// `PSTT_GLOBAL_OUTPUT_COUNT`
    OutputCount,

    /// `PSTT_GLOBAL_TX_MODIFIABLE`
    TxModifiable,

    /// `PSTT_GLOBAL_VERSION`
    Version,

    /// `PSTT_GLOBAL_PROPRIETARY`
    Proprietary,

    /// All unknown keys
    Unknown(u8),
}

impl KeyType for GlobalKey {
    const STANDARD: &'static [Self] = &[
        Self::UnsignedTx,
        Self::Xpub,
        Self::TxVersion,
        Self::FallbackLocktime,
        Self::InputCount,
        Self::OutputCount,
        Self::TxModifiable,
        Self::Version,
    ];

    #[inline]
    fn unknown_unchecked(val: u8) -> Self {
        Self::Unknown(val)
    }

    fn from_u8(val: u8) -> Self {
        match val {
            x if x == Self::UnsignedTx.into_u8() => Self::UnsignedTx,
            x if x == Self::Xpub.into_u8() => Self::Xpub,
            x if x == Self::TxVersion.into_u8() => Self::TxVersion,
            x if x == Self::FallbackLocktime.into_u8() => Self::FallbackLocktime,
            x if x == Self::InputCount.into_u8() => Self::InputCount,
            x if x == Self::OutputCount.into_u8() => Self::OutputCount,
            x if x == Self::TxModifiable.into_u8() => Self::TxModifiable,
            x if x == Self::Version.into_u8() => Self::Version,
            x if x == Self::Proprietary.into_u8() => Self::Proprietary,
            unknown => Self::Unknown(unknown),
        }
    }

    fn into_u8(self) -> u8 {
        match self {
            GlobalKey::UnsignedTx => PSTT_GLOBAL_UNSIGNED_TX,
            GlobalKey::Xpub => PSTT_GLOBAL_XPUB,
            GlobalKey::TxVersion => PSTT_GLOBAL_TX_VERSION,
            GlobalKey::FallbackLocktime => PSTT_GLOBAL_FALLBACK_LOCKTIME,
            GlobalKey::InputCount => PSTT_GLOBAL_INPUT_COUNT,
            GlobalKey::OutputCount => PSTT_GLOBAL_OUTPUT_COUNT,
            GlobalKey::TxModifiable => PSTT_GLOBAL_TX_MODIFIABLE,
            GlobalKey::Version => PSTT_GLOBAL_VERSION,
            GlobalKey::Proprietary => PSTT_GLOBAL_PROPRIETARY,
            GlobalKey::Unknown(key_type) => key_type,
        }
    }

    fn has_key_data(self) -> bool {
        match self {
            GlobalKey::UnsignedTx => false,
            GlobalKey::Xpub => true,
            GlobalKey::TxVersion => false,
            GlobalKey::FallbackLocktime => false,
            GlobalKey::InputCount => false,
            GlobalKey::OutputCount => false,
            GlobalKey::TxModifiable => false,
            GlobalKey::Version => false,
            GlobalKey::Proprietary => true,
            GlobalKey::Unknown(_) => true,
        }
    }

    fn present_since(self) -> PsttVer {
        match self {
            GlobalKey::Version | GlobalKey::UnsignedTx | GlobalKey::Xpub => PsttVer::V0,

            GlobalKey::TxVersion
            | GlobalKey::FallbackLocktime
            | GlobalKey::InputCount
            | GlobalKey::OutputCount
            | GlobalKey::TxModifiable => PsttVer::V2,

            GlobalKey::Proprietary => PsttVer::V0,
            GlobalKey::Unknown(_) => PsttVer::V0,
        }
    }

    fn deprecated_since(self) -> Option<PsttVer> {
        match self {
            GlobalKey::UnsignedTx => Some(PsttVer::V2),

            GlobalKey::Xpub
            | GlobalKey::TxVersion
            | GlobalKey::FallbackLocktime
            | GlobalKey::InputCount
            | GlobalKey::OutputCount
            | GlobalKey::TxModifiable
            | GlobalKey::Version
            | GlobalKey::Proprietary
            | GlobalKey::Unknown(_) => None,
        }
    }

    fn is_required(self) -> bool {
        match self {
            GlobalKey::UnsignedTx => true,
            GlobalKey::Xpub => false,
            GlobalKey::TxVersion => true,
            GlobalKey::FallbackLocktime => false,
            GlobalKey::InputCount => true,
            GlobalKey::OutputCount => true,
            GlobalKey::TxModifiable => false,
            GlobalKey::Version => false,
            GlobalKey::Proprietary => false,
            GlobalKey::Unknown(_) => false,
        }
    }

    fn is_proprietary(self) -> bool {
        self == Self::Proprietary
    }
}

const PSTT_IN_NON_WITNESS_UTXO: u8 = 0x00;
const PSTT_IN_WITNESS_UTXO: u8 = 0x01;
const PSTT_IN_PARTIAL_SIG: u8 = 0x02;
const PSTT_IN_SIGHASH_TYPE: u8 = 0x03;
const PSTT_IN_REDEEM_SCRIPT: u8 = 0x04;
const PSTT_IN_WITNESS_SCRIPT: u8 = 0x05;
const PSTT_IN_BIP32_DERIVATION: u8 = 0x06;
const PSTT_IN_FINAL_SCRIPTSIG: u8 = 0x07;
const PSTT_IN_FINAL_SCRIPTWITNESS: u8 = 0x08;
const PSTT_IN_POR_COMMITMENT: u8 = 0x09;
const PSTT_IN_RIPEMD160: u8 = 0x0a;
const PSTT_IN_SHA256: u8 = 0x0b;
const PSTT_IN_HASH160: u8 = 0x0c;
const PSTT_IN_HASH256: u8 = 0x0d;
const PSTT_IN_PREVIOUS_TXID: u8 = 0x0e;
const PSTT_IN_OUTPUT_INDEX: u8 = 0x0f;
const PSTT_IN_SEQUENCE: u8 = 0x10;
const PSTT_IN_REQUIRED_TIME_LOCKTIME: u8 = 0x11;
const PSTT_IN_REQUIRED_HEIGHT_LOCKTIME: u8 = 0x12;
const PSTT_IN_TAP_KEY_SIG: u8 = 0x13;
const PSTT_IN_TAP_SCRIPT_SIG: u8 = 0x14;
const PSTT_IN_TAP_LEAF_SCRIPT: u8 = 0x15;
const PSTT_IN_TAP_BIP32_DERIVATION: u8 = 0x16;
const PSTT_IN_TAP_INTERNAL_KEY: u8 = 0x17;
const PSTT_IN_TAP_MERKLE_ROOT: u8 = 0x18;
const PSTT_IN_PROPRIETARY: u8 = 0xFC;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum InputKey {
    /// `PSTT_IN_NON_WITNESS_UTXO`
    NonWitnessUtxo,

    /// `PSTT_IN_WITNESS_UTXO`
    WitnessUtxo,

    /// `PSTT_IN_PARTIAL_SIG`
    PartialSig,

    /// `PSTT_IN_SIGHASH_TYPE`
    SighashType,

    /// `PSTT_IN_REDEEM_SCRIPT`
    RedeemScript,

    /// `PSTT_IN_WITNESS_SCRIPT`
    WitnessScript,

    /// `PSTT_IN_BIP32_DERIVATION`
    Bip32Derivation,

    /// `PSTT_IN_FINAL_SCRIPTSIG`
    FinalScriptSig,

    /// `PSTT_IN_FINAL_SCRIPTWITNESS`
    FinalWitness,

    /// `PSTT_IN_POR_COMMITMENT`
    PorCommitment,

    /// `PSTT_IN_RIPEMD160`
    Ripemd160,

    /// `PSTT_IN_SHA256`
    Sha256,

    /// `PSTT_IN_HASH160`
    Hash160,

    /// `PSTT_IN_HASH256`
    Hash256,

    /// `PSTT_IN_PREVIOUS_TXID`
    PreviousTxid,

    /// `PSTT_IN_OUTPUT_INDEX`
    OutputIndex,

    /// `PSTT_IN_SEQUENCE`
    Sequence,

    /// `PSTT_IN_REQUIRED_TIME_LOCKTIME`
    RequiredTimeLock,

    /// `PSTT_IN_REQUIRED_HEIGHT_LOCKTIME`
    RequiredHeightLock,

    /// `PSTT_IN_TAP_KEY_SIG`
    TapKeySig,

    /// `PSTT_IN_TAP_SCRIPT_SIG`
    TapScriptSig,

    /// `PSTT_IN_TAP_LEAF_SCRIPT`
    TapLeafScript,

    /// `PSTT_IN_TAP_BIP32_DERIVATION`
    TapBip32Derivation,

    /// `PSTT_IN_TAP_INTERNAL_KEY`
    TapInternalKey,

    /// `PSTT_IN_TAP_MERKLE_ROOT`
    TapMerkleRoot,

    /// `PSTT_IN_PROPRIETARY`
    Proprietary,

    /// All unknown keys
    Unknown(u8),
}

impl KeyType for InputKey {
    const STANDARD: &'static [Self] = &[
        Self::NonWitnessUtxo,
        Self::WitnessUtxo,
        Self::PartialSig,
        Self::SighashType,
        Self::RedeemScript,
        Self::WitnessScript,
        Self::Bip32Derivation,
        Self::FinalScriptSig,
        Self::FinalWitness,
        Self::PorCommitment,
        Self::Ripemd160,
        Self::Sha256,
        Self::Hash160,
        Self::Hash256,
        Self::PreviousTxid,
        Self::OutputIndex,
        Self::Sequence,
        Self::RequiredTimeLock,
        Self::RequiredHeightLock,
        Self::TapKeySig,
        Self::TapScriptSig,
        Self::TapLeafScript,
        Self::TapBip32Derivation,
        Self::TapInternalKey,
        Self::TapMerkleRoot,
    ];

    #[inline]
    fn unknown_unchecked(val: u8) -> Self {
        Self::Unknown(val)
    }

    fn from_u8(val: u8) -> Self {
        match val {
            x if x == Self::NonWitnessUtxo.into_u8() => Self::NonWitnessUtxo,
            x if x == Self::WitnessUtxo.into_u8() => Self::WitnessUtxo,
            x if x == Self::PartialSig.into_u8() => Self::PartialSig,
            x if x == Self::SighashType.into_u8() => Self::SighashType,
            x if x == Self::RedeemScript.into_u8() => Self::RedeemScript,
            x if x == Self::WitnessScript.into_u8() => Self::WitnessScript,
            x if x == Self::Bip32Derivation.into_u8() => Self::Bip32Derivation,
            x if x == Self::FinalScriptSig.into_u8() => Self::FinalScriptSig,
            x if x == Self::FinalWitness.into_u8() => Self::FinalWitness,
            x if x == Self::PorCommitment.into_u8() => Self::PorCommitment,
            x if x == Self::Ripemd160.into_u8() => Self::Ripemd160,
            x if x == Self::Sha256.into_u8() => Self::Sha256,
            x if x == Self::Hash160.into_u8() => Self::Hash160,
            x if x == Self::Hash256.into_u8() => Self::Hash256,
            x if x == Self::PreviousTxid.into_u8() => Self::PreviousTxid,
            x if x == Self::OutputIndex.into_u8() => Self::OutputIndex,
            x if x == Self::Sequence.into_u8() => Self::Sequence,
            x if x == Self::RequiredTimeLock.into_u8() => Self::RequiredTimeLock,
            x if x == Self::RequiredHeightLock.into_u8() => Self::RequiredHeightLock,
            x if x == Self::TapKeySig.into_u8() => Self::TapKeySig,
            x if x == Self::TapScriptSig.into_u8() => Self::TapScriptSig,
            x if x == Self::TapLeafScript.into_u8() => Self::TapLeafScript,
            x if x == Self::TapBip32Derivation.into_u8() => Self::TapBip32Derivation,
            x if x == Self::TapInternalKey.into_u8() => Self::TapInternalKey,
            x if x == Self::TapMerkleRoot.into_u8() => Self::TapMerkleRoot,
            x if x == Self::Proprietary.into_u8() => Self::Proprietary,
            unknown => Self::Unknown(unknown),
        }
    }

    fn into_u8(self) -> u8 {
        match self {
            InputKey::NonWitnessUtxo => PSTT_IN_NON_WITNESS_UTXO,
            InputKey::WitnessUtxo => PSTT_IN_WITNESS_UTXO,
            InputKey::PartialSig => PSTT_IN_PARTIAL_SIG,
            InputKey::SighashType => PSTT_IN_SIGHASH_TYPE,
            InputKey::RedeemScript => PSTT_IN_REDEEM_SCRIPT,
            InputKey::WitnessScript => PSTT_IN_WITNESS_SCRIPT,
            InputKey::Bip32Derivation => PSTT_IN_BIP32_DERIVATION,
            InputKey::FinalScriptSig => PSTT_IN_FINAL_SCRIPTSIG,
            InputKey::FinalWitness => PSTT_IN_FINAL_SCRIPTWITNESS,
            InputKey::PorCommitment => PSTT_IN_POR_COMMITMENT,
            InputKey::Ripemd160 => PSTT_IN_RIPEMD160,
            InputKey::Sha256 => PSTT_IN_SHA256,
            InputKey::Hash160 => PSTT_IN_HASH160,
            InputKey::Hash256 => PSTT_IN_HASH256,
            InputKey::PreviousTxid => PSTT_IN_PREVIOUS_TXID,
            InputKey::OutputIndex => PSTT_IN_OUTPUT_INDEX,
            InputKey::Sequence => PSTT_IN_SEQUENCE,
            InputKey::RequiredTimeLock => PSTT_IN_REQUIRED_TIME_LOCKTIME,
            InputKey::RequiredHeightLock => PSTT_IN_REQUIRED_HEIGHT_LOCKTIME,
            InputKey::TapKeySig => PSTT_IN_TAP_KEY_SIG,
            InputKey::TapScriptSig => PSTT_IN_TAP_SCRIPT_SIG,
            InputKey::TapLeafScript => PSTT_IN_TAP_LEAF_SCRIPT,
            InputKey::TapBip32Derivation => PSTT_IN_TAP_BIP32_DERIVATION,
            InputKey::TapInternalKey => PSTT_IN_TAP_INTERNAL_KEY,
            InputKey::TapMerkleRoot => PSTT_IN_TAP_MERKLE_ROOT,
            InputKey::Proprietary => PSTT_IN_PROPRIETARY,
            InputKey::Unknown(key_type) => key_type,
        }
    }

    fn has_key_data(self) -> bool {
        match self {
            InputKey::NonWitnessUtxo => false,
            InputKey::WitnessUtxo => false,
            InputKey::PartialSig => true,
            InputKey::SighashType => false,
            InputKey::RedeemScript => false,
            InputKey::WitnessScript => false,
            InputKey::Bip32Derivation => true,
            InputKey::FinalScriptSig => false,
            InputKey::FinalWitness => false,

            InputKey::PorCommitment => false,
            InputKey::Ripemd160 => true,
            InputKey::Sha256 => true,
            InputKey::Hash160 => true,
            InputKey::Hash256 => true,

            InputKey::PreviousTxid => false,
            InputKey::OutputIndex => false,
            InputKey::Sequence => false,
            InputKey::RequiredTimeLock => false,
            InputKey::RequiredHeightLock => false,

            InputKey::TapKeySig => false,
            InputKey::TapScriptSig => true,
            InputKey::TapLeafScript => true,
            InputKey::TapBip32Derivation => true,
            InputKey::TapInternalKey => false,
            InputKey::TapMerkleRoot => false,

            InputKey::Proprietary => true,
            InputKey::Unknown(_) => true,
        }
    }

    fn present_since(self) -> PsttVer {
        match self {
            InputKey::NonWitnessUtxo
            | InputKey::WitnessUtxo
            | InputKey::PartialSig
            | InputKey::SighashType
            | InputKey::RedeemScript
            | InputKey::WitnessScript
            | InputKey::Bip32Derivation
            | InputKey::FinalScriptSig
            | InputKey::FinalWitness => PsttVer::V0,

            InputKey::PorCommitment
            | InputKey::Ripemd160
            | InputKey::Sha256
            | InputKey::Hash160
            | InputKey::Hash256 => PsttVer::V0,

            InputKey::PreviousTxid
            | InputKey::OutputIndex
            | InputKey::Sequence
            | InputKey::RequiredTimeLock
            | InputKey::RequiredHeightLock => PsttVer::V2,

            InputKey::TapKeySig
            | InputKey::TapScriptSig
            | InputKey::TapLeafScript
            | InputKey::TapBip32Derivation
            | InputKey::TapInternalKey
            | InputKey::TapMerkleRoot => PsttVer::V0,

            InputKey::Proprietary => PsttVer::V0,
            InputKey::Unknown(_) => PsttVer::V0,
        }
    }

    fn deprecated_since(self) -> Option<PsttVer> {
        match self {
            InputKey::NonWitnessUtxo
            | InputKey::WitnessUtxo
            | InputKey::PartialSig
            | InputKey::SighashType
            | InputKey::RedeemScript
            | InputKey::WitnessScript
            | InputKey::Bip32Derivation
            | InputKey::FinalScriptSig
            | InputKey::FinalWitness
            | InputKey::PorCommitment
            | InputKey::Ripemd160
            | InputKey::Sha256
            | InputKey::Hash160
            | InputKey::Hash256
            | InputKey::PreviousTxid
            | InputKey::OutputIndex
            | InputKey::Sequence
            | InputKey::RequiredTimeLock
            | InputKey::RequiredHeightLock
            | InputKey::TapKeySig
            | InputKey::TapScriptSig
            | InputKey::TapLeafScript
            | InputKey::TapBip32Derivation
            | InputKey::TapInternalKey
            | InputKey::TapMerkleRoot
            | InputKey::Proprietary
            | InputKey::Unknown(_) => None,
        }
    }

    fn is_required(self) -> bool {
        match self {
            InputKey::NonWitnessUtxo
            | InputKey::WitnessUtxo
            | InputKey::PartialSig
            | InputKey::SighashType
            | InputKey::RedeemScript
            | InputKey::WitnessScript
            | InputKey::Bip32Derivation
            | InputKey::FinalScriptSig
            | InputKey::FinalWitness => false,

            InputKey::PorCommitment
            | InputKey::Ripemd160
            | InputKey::Sha256
            | InputKey::Hash160
            | InputKey::Hash256 => false,

            InputKey::PreviousTxid | InputKey::OutputIndex => true,
            InputKey::Sequence | InputKey::RequiredTimeLock | InputKey::RequiredHeightLock => false,

            InputKey::TapKeySig
            | InputKey::TapScriptSig
            | InputKey::TapLeafScript
            | InputKey::TapBip32Derivation
            | InputKey::TapInternalKey
            | InputKey::TapMerkleRoot => false,

            InputKey::Proprietary => false,
            InputKey::Unknown(_) => false,
        }
    }

    fn is_proprietary(self) -> bool {
        self == Self::Proprietary
    }
}

const PSTT_OUT_REDEEM_SCRIPT: u8 = 0x00;
const PSTT_OUT_WITNESS_SCRIPT: u8 = 0x01;
const PSTT_OUT_BIP32_DERIVATION: u8 = 0x02;
const PSTT_OUT_AMOUNT: u8 = 0x03;
const PSTT_OUT_SCRIPT: u8 = 0x04;
const PSTT_OUT_TAP_INTERNAL_KEY: u8 = 0x05;
const PSTT_OUT_TAP_TREE: u8 = 0x06;
const PSTT_OUT_TAP_BIP32_DERIVATION: u8 = 0x07;
const PSTT_OUT_PROPRIETARY: u8 = 0xFC;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum OutputKey {
    /// `PSTT_OUT_REDEEM_SCRIPT`
    RedeemScript,

    /// `PSTT_OUT_WITNESS_SCRIPT`
    WitnessScript,

    /// `PSTT_OUT_BIP32_DERIVATION`
    Bip32Derivation,

    /// `PSTT_OUT_AMOUNT`
    Amount,

    /// `PSTT_OUT_SCRIPT`
    Script,

    /// `PSTT_OUT_TAP_INTERNAL_KEY`
    TapInternalKey,

    /// `PSTT_OUT_TAP_TREE`
    TapTree,

    /// `PSTT_OUT_TAP_BIP32_DERIVATION`
    TapBip32Derivation,

    /// `PSTT_OUT_PROPRIETARY`
    Proprietary,

    /// All unknown keys
    Unknown(u8),
}

impl KeyType for OutputKey {
    const STANDARD: &'static [Self] = &[
        Self::RedeemScript,
        Self::WitnessScript,
        Self::Bip32Derivation,
        Self::Amount,
        Self::Script,
        Self::TapInternalKey,
        Self::TapTree,
        Self::TapBip32Derivation,
    ];

    #[inline]
    fn unknown_unchecked(val: u8) -> Self {
        Self::Unknown(val)
    }

    fn from_u8(val: u8) -> Self {
        match val {
            x if x == Self::RedeemScript.into_u8() => Self::RedeemScript,
            x if x == Self::WitnessScript.into_u8() => Self::WitnessScript,
            x if x == Self::Bip32Derivation.into_u8() => Self::Bip32Derivation,
            x if x == Self::Amount.into_u8() => Self::Amount,
            x if x == Self::Script.into_u8() => Self::Script,

            x if x == Self::TapInternalKey.into_u8() => Self::TapInternalKey,
            x if x == Self::TapTree.into_u8() => Self::TapTree,
            x if x == Self::TapBip32Derivation.into_u8() => Self::TapBip32Derivation,

            x if x == Self::Proprietary.into_u8() => Self::Proprietary,
            unknown => Self::Unknown(unknown),
        }
    }

    fn into_u8(self) -> u8 {
        match self {
            OutputKey::RedeemScript => PSTT_OUT_REDEEM_SCRIPT,
            OutputKey::WitnessScript => PSTT_OUT_WITNESS_SCRIPT,
            OutputKey::Bip32Derivation => PSTT_OUT_BIP32_DERIVATION,
            OutputKey::Amount => PSTT_OUT_AMOUNT,
            OutputKey::Script => PSTT_OUT_SCRIPT,
            OutputKey::TapInternalKey => PSTT_OUT_TAP_INTERNAL_KEY,
            OutputKey::TapTree => PSTT_OUT_TAP_TREE,
            OutputKey::TapBip32Derivation => PSTT_OUT_TAP_BIP32_DERIVATION,
            OutputKey::Proprietary => PSTT_OUT_PROPRIETARY,
            OutputKey::Unknown(key_type) => key_type,
        }
    }

    fn has_key_data(self) -> bool {
        match self {
            OutputKey::RedeemScript | OutputKey::WitnessScript => false,
            OutputKey::Bip32Derivation => true,
            OutputKey::Amount | OutputKey::Script => false,
            OutputKey::TapInternalKey => false,
            OutputKey::TapTree => false,
            OutputKey::TapBip32Derivation => true,
            OutputKey::Proprietary => true,
            OutputKey::Unknown(_) => true,
        }
    }

    fn present_since(self) -> PsttVer {
        match self {
            OutputKey::RedeemScript | OutputKey::WitnessScript | OutputKey::Bip32Derivation => {
                PsttVer::V0
            }
            OutputKey::Amount | OutputKey::Script => PsttVer::V2,

            OutputKey::TapInternalKey | OutputKey::TapTree | OutputKey::TapBip32Derivation => {
                PsttVer::V0
            }

            OutputKey::Proprietary => PsttVer::V0,
            OutputKey::Unknown(_) => PsttVer::V0,
        }
    }

    fn deprecated_since(self) -> Option<PsttVer> {
        match self {
            OutputKey::RedeemScript
            | OutputKey::WitnessScript
            | OutputKey::Bip32Derivation
            | OutputKey::Amount
            | OutputKey::Script
            | OutputKey::TapInternalKey
            | OutputKey::TapTree
            | OutputKey::TapBip32Derivation => None,

            OutputKey::Proprietary => None,
            OutputKey::Unknown(_) => None,
        }
    }

    fn is_required(self) -> bool {
        match self {
            OutputKey::RedeemScript | OutputKey::WitnessScript | OutputKey::Bip32Derivation => {
                false
            }
            OutputKey::Amount | OutputKey::Script => true,
            OutputKey::TapInternalKey | OutputKey::TapTree | OutputKey::TapBip32Derivation => false,
            OutputKey::Proprietary => false,
            OutputKey::Unknown(_) => false,
        }
    }

    fn is_proprietary(self) -> bool {
        self == Self::Proprietary
    }
}

pub enum KeyValue<T: KeyType> {
    Pair(KeyPair<T, KeyData, ValueData>),
    Separator,
}

#[derive(Debug)]
pub struct KeyPair<T: KeyType, K, V> {
    pub key_type: T,
    pub key_data: K,
    pub value_data: V,
}

impl<T: KeyType, K, V> KeyPair<T, K, V> {
    pub fn new(key_type: T, key_data: K, value_data: V) -> Self {
        Self {
            key_type,
            key_data,
            value_data,
        }
    }
}

impl<'a, T: KeyType> KeyPair<T, Box<dyn Encode + 'a>, Box<dyn Encode + 'a>> {
    pub fn boxed<K: Encode + 'a, V: Encode + 'a>(key_type: T, key_data: K, value_data: V) -> Self {
        Self::new(key_type, Box::new(key_data), Box::new(value_data))
    }
}

#[derive(Clone, PartialOrd, Ord, Eq, PartialEq, Hash, Debug, Display)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
#[display("{identifier} {subtype:#x} {data:#x}")]
pub struct PropKey {
    pub identifier: String,
    pub subtype: u64,
    pub data: KeyData,
}
