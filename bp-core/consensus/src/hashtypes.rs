// Tondi protocol consensus library.
//
// SPDX-License-Identifier: Apache-2.0
//
// Based on Bitcoin protocol library by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Adapted for Tondi by AvatoLabs
//
// Copyright (C) 2019-2024 LNP/BP Standards Association. All rights reserved.
// Copyright (C) 2025 AvatoLabs. All rights reserved.
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

use amplify::{Bytes20, Bytes32, Wrapper};
use commit_verify::TondiBlake3;

use crate::{
    CompressedPk, LegacyPk, RedeemScript, UncompressedPk, WitnessScript, LIB_NAME_TONDI,
};

/// A hash of a public key, used in P2PKH and P2WPKH addresses.
///
/// For Tondi L1, this is computed using BLAKE3 truncated to 20 bytes.
#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, AsSlice, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct PubkeyHash(
    #[from]
    #[from([u8; 20])]
    pub Bytes20,
);

impl From<PubkeyHash> for [u8; 20] {
    fn from(value: PubkeyHash) -> Self { value.0.into_inner() }
}

impl From<CompressedPk> for PubkeyHash {
    /// Compute BLAKE3 pubkey hash truncated to 20 bytes.
    fn from(pk: CompressedPk) -> Self {
        let hash = TondiBlake3::hash_with_context("tondi.pubkey.hash", &pk.to_byte_array());
        let mut result = [0u8; 20];
        result.copy_from_slice(&hash[..20]);
        Self(result.into())
    }
}

impl From<UncompressedPk> for PubkeyHash {
    /// Compute BLAKE3 pubkey hash truncated to 20 bytes.
    fn from(pk: UncompressedPk) -> Self {
        let hash = TondiBlake3::hash_with_context("tondi.pubkey.hash", &pk.to_byte_array());
        let mut result = [0u8; 20];
        result.copy_from_slice(&hash[..20]);
        Self(result.into())
    }
}

impl From<LegacyPk> for PubkeyHash {
    /// Compute BLAKE3 pubkey hash truncated to 20 bytes.
    fn from(pk: LegacyPk) -> Self {
        let hash = TondiBlake3::hash_with_context("tondi.pubkey.hash", &pk.to_vec());
        let mut result = [0u8; 20];
        result.copy_from_slice(&hash[..20]);
        Self(result.into())
    }
}

/// A hash of a redeem script, used in P2SH addresses.
///
/// For Tondi L1, this is computed using BLAKE3 truncated to 20 bytes.
#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, AsSlice, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct ScriptHash(
    #[from]
    #[from([u8; 20])]
    pub Bytes20,
);

impl From<ScriptHash> for [u8; 20] {
    fn from(value: ScriptHash) -> Self { value.0.into_inner() }
}

impl From<&RedeemScript> for ScriptHash {
    /// Compute BLAKE3 script hash truncated to 20 bytes.
    fn from(redeem_script: &RedeemScript) -> Self {
        let hash = TondiBlake3::hash_with_context("tondi.script.hash", redeem_script.as_slice());
        let mut result = [0u8; 20];
        result.copy_from_slice(&hash[..20]);
        Self(result.into())
    }
}

/// A hash of a public key for witness (SegWit) transactions.
///
/// For Tondi L1, this is computed using BLAKE3 truncated to 20 bytes.
#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, AsSlice, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct WPubkeyHash(
    #[from]
    #[from([u8; 20])]
    pub Bytes20,
);

impl From<WPubkeyHash> for [u8; 20] {
    fn from(value: WPubkeyHash) -> Self { value.0.into_inner() }
}

impl From<CompressedPk> for WPubkeyHash {
    /// Compute BLAKE3 witness pubkey hash truncated to 20 bytes.
    fn from(pk: CompressedPk) -> Self {
        let hash = TondiBlake3::hash_with_context("tondi.wpubkey.hash", &pk.to_byte_array());
        let mut result = [0u8; 20];
        result.copy_from_slice(&hash[..20]);
        Self(result.into())
    }
}

/// A hash of a witness script.
///
/// For Tondi L1, this is computed using BLAKE3 (full 32 bytes).
#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, AsSlice, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct WScriptHash(
    #[from]
    #[from([u8; 32])]
    pub Bytes32,
);

impl From<WScriptHash> for [u8; 32] {
    fn from(value: WScriptHash) -> Self { value.0.into_inner() }
}

impl From<&WitnessScript> for WScriptHash {
    /// Compute BLAKE3 witness script hash (full 32 bytes).
    fn from(witness_script: &WitnessScript) -> Self {
        let hash = TondiBlake3::hash_with_context("tondi.wscript.hash", witness_script.as_slice());
        Self(hash.into())
    }
}
