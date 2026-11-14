// Tondi protocol consensus library.
//
// SPDX-License-Identifier: Apache-2.0
//
// Based on Tondi protocol library by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
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

use core::slice;
use std::fmt::{self, Debug, Display, Formatter, LowerHex};
use std::iter::Sum;
use std::num::ParseIntError;
use std::ops::{Div, Rem};
use std::str::FromStr;

use amplify::hex::{self, FromHex, ToHex};
use amplify::{ByteArray, Bytes32, Bytes32StrRev, Wrapper};
use commit_verify::{DigestExt, TondiBlake3};
use strict_encoding::{
    DecodeError, ReadRaw, StrictDecode, StrictEncode, TypedRead, TypedWrite, WriteRaw,
};

use crate::{
    ConsensusDecode, ConsensusDecodeError, ConsensusEncode, LockTime, ScriptPubkey, SeqNo,
    SigScript, VarIntArray, VarIntBytes, Witness, Wtxid, LIB_NAME_TONDI,
};

/// Tondi classic script public key version.
pub const TONDI_CLASSIC_SCRIPT_VERSION: u16 = 0;

/// Tondi-native Pay-to-Merkle-Root script public key version.
pub const TONDI_P2MR_SCRIPT_VERSION: u16 = 4;
pub const TONDI_TX_VERSION: u16 = 1;
pub const MAX_DATUM_SIZE: usize = 4096;
pub type Datum = amplify::confinement::Confined<Vec<u8>, 0, MAX_DATUM_SIZE>;

#[derive(Wrapper, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, From)]
#[wrapper(AsSlice)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
#[wrapper(BorrowSlice, Index, RangeOps, Debug, Hex, Display, FromStr)]
// all-zeros used in coinbase
pub struct Txid(
    #[from]
    #[from([u8; 32])]
    Bytes32StrRev,
);

impl From<Txid> for [u8; 32] {
    fn from(txid: Txid) -> Self {
        txid.to_byte_array()
    }
}

impl Txid {
    #[inline]
    pub const fn coinbase() -> Self {
        Self(Bytes32StrRev::zero())
    }
    #[inline]
    pub fn is_coinbase(&self) -> bool {
        self.to_byte_array() == [0u8; 32]
    }
}

/// Tondi transaction ID using BLAKE3 hash algorithm.
///
/// This type represents a transaction ID computed using BLAKE3 with domain separation,
/// matching the node's native transaction identity and RGB's `WitnessId` requirements.
///
/// # RGB Compatibility
///
/// This type implements all traits required by RGB's `RgbSeal::WitnessId`:
/// - `Copy + Ord + Hash + Debug + Display`
#[derive(Wrapper, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, From)]
#[wrapper(AsSlice)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
#[wrapper(BorrowSlice, Index, RangeOps, Debug, Hex, Display, FromStr)]
pub struct TondiTxid(
    #[from]
    #[from([u8; 32])]
    Bytes32StrRev,
);

impl From<TondiTxid> for [u8; 32] {
    fn from(txid: TondiTxid) -> Self {
        txid.to_byte_array()
    }
}

impl TondiTxid {
    /// Creates a TondiTxid from a byte array.
    #[inline]
    pub fn from_byte_array(bytes: [u8; 32]) -> Self {
        Self(Bytes32StrRev::from_byte_array(bytes))
    }

    /// Converts the TondiTxid to a byte array.
    #[inline]
    pub fn to_byte_array(self) -> [u8; 32] {
        self.0.to_byte_array()
    }

    /// Returns the coinbase transaction ID (all zeros).
    #[inline]
    pub const fn coinbase() -> Self {
        Self(Bytes32StrRev::zero())
    }

    /// Checks if this is a coinbase transaction ID.
    #[inline]
    pub fn is_coinbase(&self) -> bool {
        self.to_byte_array() == [0u8; 32]
    }
}

/// Conversion from `Txid` to `TondiTxid`.
///
/// This allows interoperability between Tondi-compatible `Txid` and Tondi-specific `TondiTxid`.
impl From<Txid> for TondiTxid {
    fn from(txid: Txid) -> Self {
        TondiTxid::from_byte_array(txid.to_byte_array())
    }
}

/// Conversion from `TondiTxid` to `Txid`.
///
/// This allows interoperability between Tondi-specific `TondiTxid` and Tondi-compatible `Txid`.
impl From<TondiTxid> for Txid {
    fn from(txid: TondiTxid) -> Self {
        Txid::from_byte_array(txid.to_byte_array())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, From)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
#[display(inner)]
// 0xFFFFFFFF used in coinbase
pub struct Vout(u32);

impl Vout {
    pub const fn from_u32(u: u32) -> Self {
        Vout(u)
    }
    #[inline]
    pub const fn into_u32(self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn into_usize(self) -> usize {
        self.0 as usize
    }
    #[inline]
    pub const fn to_u32(&self) -> u32 {
        self.0
    }
    #[inline]
    pub const fn to_usize(&self) -> usize {
        self.0 as usize
    }
}

impl FromStr for Vout {
    type Err = ParseIntError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[display("{txid}:{vout}")]
pub struct Outpoint {
    pub txid: Txid,
    pub vout: Vout,
}

impl Outpoint {
    #[inline]
    pub fn new(txid: Txid, vout: impl Into<Vout>) -> Self {
        Self {
            txid,
            vout: vout.into(),
        }
    }

    #[inline]
    pub const fn coinbase() -> Self {
        Self {
            txid: Txid::coinbase(),
            vout: Vout::from_u32(0),
        }
    }

    #[inline]
    pub fn vout_u32(self) -> u32 {
        self.vout.into_u32()
    }

    #[inline]
    pub fn vout_usize(self) -> usize {
        self.vout.into_usize()
    }

    #[inline]
    pub fn is_coinbase(&self) -> bool {
        self.txid.is_coinbase() && self.vout.into_u32() == 0
    }
}

/// Tondi outpoint using TondiTxid.
///
/// This type represents a transaction output point using Tondi's BLAKE3-based transaction ID,
/// as opposed to Tondi's double SHA256. The serialization format is identical to `Outpoint`:
/// 32 bytes for txid followed by 4 bytes for vout.
///
/// # Serialization Format
///
/// The serialization format is identical to Tondi's Outpoint:
/// - 32 bytes: Transaction ID (TondiTxid)
/// - 4 bytes: Output index (Vout, little-endian u32)
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[display("{txid}:{vout}")]
pub struct TondiOutpoint {
    pub txid: TondiTxid,
    pub vout: Vout,
}

impl TondiOutpoint {
    /// Creates a new TondiOutpoint.
    #[inline]
    pub fn new(txid: TondiTxid, vout: impl Into<Vout>) -> Self {
        Self {
            txid,
            vout: vout.into(),
        }
    }

    /// Returns the coinbase outpoint.
    #[inline]
    pub const fn coinbase() -> Self {
        Self {
            txid: TondiTxid::coinbase(),
            vout: Vout::from_u32(0),
        }
    }

    /// Returns the vout as u32.
    #[inline]
    pub fn vout_u32(self) -> u32 {
        self.vout.into_u32()
    }

    /// Returns the vout as usize.
    #[inline]
    pub fn vout_usize(self) -> usize {
        self.vout.into_usize()
    }

    /// Checks if this is a coinbase outpoint.
    #[inline]
    pub fn is_coinbase(&self) -> bool {
        self.txid.is_coinbase() && self.vout.into_u32() == 0
    }
}

/// Conversion from `Outpoint` to `TondiOutpoint`.
///
/// This allows interoperability between Tondi-compatible `Outpoint` and Tondi-specific
/// `TondiOutpoint`.
impl From<Outpoint> for TondiOutpoint {
    fn from(outpoint: Outpoint) -> Self {
        TondiOutpoint {
            txid: outpoint.txid.into(),
            vout: outpoint.vout,
        }
    }
}

/// Conversion from `TondiOutpoint` to `Outpoint`.
///
/// This allows interoperability between Tondi-specific `TondiOutpoint` and Tondi-compatible
/// `Outpoint`.
impl From<TondiOutpoint> for Outpoint {
    fn from(outpoint: TondiOutpoint) -> Self {
        Outpoint {
            txid: outpoint.txid.into(),
            vout: outpoint.vout,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Display, From, Error)]
#[display(doc_comments)]
pub enum OutpointParseError {
    /// malformed string representation of outoint '{0}' lacking txid and vout
    /// separator ':'
    MalformedSeparator(String),

    /// malformed outpoint output number. Details: {0}
    #[from]
    InvalidVout(ParseIntError),

    /// malformed outpoint txid value. Details: {0}
    #[from]
    InvalidTxid(hex::Error),
}

impl FromStr for Outpoint {
    type Err = OutpointParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (txid, vout) = s
            .split_once(':')
            .ok_or_else(|| OutpointParseError::MalformedSeparator(s.to_owned()))?;
        Ok(Outpoint::new(txid.parse()?, Vout::from_str(vout)?))
    }
}

#[cfg(feature = "serde")]
mod _serde_outpoint {
    use serde::de::{SeqAccess, Visitor};
    use serde::ser::SerializeTuple;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    impl Serialize for Outpoint {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            if serializer.is_human_readable() {
                serializer.serialize_str(&self.to_string())
            } else {
                let mut ser = serializer.serialize_tuple(2)?;
                ser.serialize_element(&self.txid)?;
                ser.serialize_element(&self.vout)?;
                ser.end()
            }
        }
    }

    impl<'de> Deserialize<'de> for Outpoint {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            use serde::de::Error;
            if deserializer.is_human_readable() {
                String::deserialize(deserializer).and_then(|string| {
                    Self::from_str(&string)
                        .map_err(|_| D::Error::custom("wrong outpoint string representation"))
                })
            } else {
                struct OutpointVisitor;

                impl<'de> Visitor<'de> for OutpointVisitor {
                    type Value = Outpoint;

                    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                        write!(formatter, "a transaction outpoint")
                    }

                    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                    where
                        A: SeqAccess<'de>,
                    {
                        let mut outpoint = Outpoint::coinbase();
                        outpoint.txid =
                            seq.next_element()?.ok_or_else(|| Error::invalid_length(0, &self))?;
                        outpoint.vout =
                            seq.next_element()?.ok_or_else(|| Error::invalid_length(1, &self))?;
                        Ok(outpoint)
                    }
                }

                deserializer.deserialize_tuple(2, OutpointVisitor)
            }
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct TxIn {
    pub is_reference: bool,
    pub sig_op_count: u8,
    pub prev_output: Outpoint,
    pub sig_script: SigScript,
    pub sequence: SeqNo,
    pub witness: Witness,
}

#[derive(
    Wrapper, WrapperMut, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, From, Default
)]
#[wrapper(Add, Sub, Mul, Div, FromStr)]
#[wrapper_mut(MathAssign)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct Sats(
    #[from]
    #[from(u32)]
    #[from(u16)]
    #[from(u8)]
    pub u64,
);

impl Sats {
    pub const ZERO: Self = Sats(0);
    #[allow(clippy::inconsistent_digit_grouping)]
    pub const TONDI_COIN: Self = Sats(1_000_000_00);
    pub const MAX: Self = Sats(u64::MAX);

    pub const fn from_tondi(coins: u32) -> Self {
        Self(coins as u64 * Self::TONDI_COIN.0)
    }
    pub fn from_sats(sats: impl Into<u64>) -> Self {
        Self(sats.into())
    }

    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }
    pub const fn is_non_zero(&self) -> bool {
        self.0 != 0
    }

    pub const fn coin_round(&self) -> u64 {
        if self.0 == 0 {
            return 0;
        }
        let inc = 2 * self.sats_rem() / Self::TONDI_COIN.0;
        self.0 / Self::TONDI_COIN.0 + inc
    }

    pub const fn coin_ceil(&self) -> u64 {
        if self.0 == 0 {
            return 0;
        }
        let inc = if self.sats_rem() > 0 { 1 } else { 0 };
        self.0 / Self::TONDI_COIN.0 + inc
    }

    pub const fn coin_floor(&self) -> u64 {
        if self.0 == 0 {
            return 0;
        }
        self.0 / Self::TONDI_COIN.0
    }

    pub const fn sats(&self) -> u64 {
        self.0
    }

    pub fn sats_i64(&self) -> i64 {
        i64::try_from(self.0).expect("amount exceeds supported signed range")
    }

    pub const fn sats_rem(&self) -> u64 {
        self.0 % Self::TONDI_COIN.0
    }

    pub const fn coin_sats(&self) -> (u64, u64) {
        (self.coin_floor(), self.sats_rem())
    }

    #[must_use]
    pub fn checked_add(&self, other: impl Into<Self>) -> Option<Self> {
        self.0.checked_add(other.into().0).map(Self)
    }
    #[must_use]
    pub fn checked_sub(&self, other: impl Into<Self>) -> Option<Self> {
        self.0.checked_sub(other.into().0).map(Self)
    }

    #[must_use]
    pub fn checked_add_assign(&mut self, other: impl Into<Self>) -> Option<Self> {
        *self = Self(self.0.checked_add(other.into().0)?);
        Some(*self)
    }

    #[must_use]
    pub fn checked_sub_assign(&mut self, other: impl Into<Self>) -> Option<Self> {
        *self = Self(self.0.checked_sub(other.into().0)?);
        Some(*self)
    }

    #[must_use]
    pub fn saturating_add(&self, other: impl Into<Self>) -> Self {
        self.0.saturating_add(other.into().0).into()
    }

    #[must_use]
    pub fn saturating_sub(&self, other: impl Into<Self>) -> Self {
        self.0.saturating_sub(other.into().0).into()
    }

    pub fn saturating_add_assign(&mut self, other: impl Into<Self>) {
        *self = self.0.saturating_add(other.into().0).into();
    }
    pub fn saturating_sub_assign(&mut self, other: impl Into<Self>) {
        *self = self.0.saturating_sub(other.into().0).into();
    }
}

impl PartialEq<u64> for Sats {
    fn eq(&self, other: &u64) -> bool {
        self.0.eq(other)
    }
}

impl Sum for Sats {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Sats::ZERO, |sum, value| sum.saturating_add(value))
    }
}

impl Sum<u64> for Sats {
    fn sum<I: Iterator<Item = u64>>(iter: I) -> Self {
        iter.fold(Sats::ZERO, |sum, value| sum.saturating_add(value))
    }
}

impl Div<usize> for Sats {
    type Output = Sats;
    fn div(self, rhs: usize) -> Self::Output {
        Sats(self.0 / rhs as u64)
    }
}

impl Rem<usize> for Sats {
    type Output = Sats;
    fn rem(self, rhs: usize) -> Self::Output {
        Sats(self.0 % rhs as u64)
    }
}

impl Display for Sats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct TxOut {
    pub datum: Option<Datum>,
    pub value: Sats,
    pub script_pubkey_version: u16,
    pub script_pubkey: ScriptPubkey,
}

impl TxOut {
    pub fn new(script_pubkey: impl Into<ScriptPubkey>, value: impl Into<Sats>) -> Self {
        let script_pubkey = script_pubkey.into();
        let script_pubkey_version = inferred_tondi_script_version(&script_pubkey);
        TxOut {
            datum: None,
            value: value.into(),
            script_pubkey_version,
            script_pubkey,
        }
    }

    pub fn with_script_version(
        script_pubkey_version: u16,
        script_pubkey: impl Into<ScriptPubkey>,
        value: impl Into<Sats>,
    ) -> Self {
        TxOut {
            datum: None,
            value: value.into(),
            script_pubkey_version,
            script_pubkey: script_pubkey.into(),
        }
    }

    pub fn p2mr(script_pubkey: impl Into<ScriptPubkey>, value: impl Into<Sats>) -> Self {
        Self::with_script_version(TONDI_P2MR_SCRIPT_VERSION, script_pubkey, value)
    }
}

fn inferred_tondi_script_version(script_pubkey: &ScriptPubkey) -> u16 {
    let script = script_pubkey.as_slice();
    if script.len() == 32 && script.iter().any(|byte| *byte != 0) {
        TONDI_P2MR_SCRIPT_VERSION
    } else {
        TONDI_CLASSIC_SCRIPT_VERSION
    }
}

/// Transaction version for Tondi L1.
///
/// # Critical: Type Compatibility
///
/// Tondi uses `u16` (2 bytes) for transaction version, not `i32` (4 bytes) like Tondi.
/// This type MUST match Tondi's native `Transaction.version` field exactly to ensure
/// correct Txid calculation.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TxVer(u16);

impl Default for TxVer {
    fn default() -> Self {
        Self::CURRENT
    }
}

impl TxVer {
    /// The only supported development transaction version.
    pub const CURRENT: Self = TxVer(TONDI_TX_VERSION);

    /// Create from u16 (Tondi's native format)
    #[inline]
    pub const fn from_consensus_u16(ver: u16) -> Self {
        TxVer(ver)
    }

    /// Create from u16 (alias for from_consensus_u16)
    #[inline]
    pub const fn from_u16(ver: u16) -> Self {
        TxVer(ver)
    }

    /// Convert to u16 (Tondi's native format)
    #[inline]
    pub const fn to_consensus_u16(&self) -> u16 {
        self.0
    }

    /// Convert to u16 (alias for to_consensus_u16)
    #[inline]
    pub const fn to_u16(&self) -> u16 {
        self.0
    }

    /// Whether this is the current development transaction version.
    #[inline]
    pub const fn is_standard(self) -> bool {
        self.0 == TONDI_TX_VERSION
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Display)]
#[derive(StrictType, StrictDumb)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
#[display(LowerHex)]
pub struct Tx {
    pub version: TxVer,
    pub inputs: VarIntArray<TxIn>,
    pub outputs: VarIntArray<TxOut>,
    pub lock_time: LockTime,
    /// Subnetwork ID (20 bytes) - Required for Tondi consensus
    ///
    /// This field is part of Tondi's transaction structure and must be included
    /// in the transaction hash calculation to match Tondi's native implementation.
    ///
    /// Tondi uses 20 bytes for subnetwork ID (see Tondi/consensus/core/src/subnets.rs:
    /// SUBNETWORK_ID_SIZE = 20).
    pub subnetwork_id: [u8; 20],
    /// Gas fee (in saus) - Required for Tondi consensus
    ///
    /// This field is part of Tondi's transaction structure and must be included
    /// in the transaction hash calculation to match Tondi's native implementation.
    pub gas: u64,
    /// Payload (variable-length bytes) - Required for Tondi consensus
    ///
    /// This field typically contains RGB commitment data (Annex) and must be included
    /// in the transaction hash calculation to match Tondi's native implementation.
    ///
    /// In Tondi, payload is serialized as u32-prefixed bytes (not VarInt).
    pub payload: VarIntBytes,
    /// Transaction mass (u64) - Required for Tondi consensus
    ///
    /// This field represents the transaction mass (weight) calculated by the node.
    /// Note: This field is NOT included in Txid calculation, only in full transaction
    /// serialization.
    pub mass: u64,
}

impl LowerHex for Tx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.consensus_serialize().to_hex())
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Display, Error, From)]
#[display(inner)]
pub enum BlockDataParseError {
    #[from]
    Hex(hex::Error),
    #[from]
    Consensus(ConsensusDecodeError),
}

impl FromStr for Tx {
    type Err = BlockDataParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let data = Vec::<u8>::from_hex(s)?;
        Tx::consensus_deserialize(data).map_err(BlockDataParseError::from)
    }
}

// Manual implementation of StrictEncode and StrictDecode for Tx.
//
// The payload remains the Tondi consensus transaction encoding, but strict encoding must frame the
// variable-sized transaction. Append-only stores such as aora keep multiple strict values in a
// single log file and therefore need the length prefix to read exactly one transaction back.
impl StrictEncode for Tx {
    fn strict_encode<W: TypedWrite>(&self, mut writer: W) -> std::io::Result<W> {
        let data = self.consensus_serialize();
        const MAX_TX_STRICT_SIZE: usize = u32::MAX as usize;
        #[allow(unsafe_code)]
        unsafe {
            writer.raw_writer().write_raw_len::<MAX_TX_STRICT_SIZE>(data.len())?;
            writer.raw_writer().write_raw::<MAX_TX_STRICT_SIZE>(&data)?;
        }
        Ok(writer)
    }
}

impl StrictDecode for Tx {
    fn strict_decode(reader: &mut impl TypedRead) -> Result<Self, DecodeError> {
        const MAX_TX_STRICT_SIZE: usize = u32::MAX as usize;
        #[allow(unsafe_code)]
        unsafe {
            let raw_reader = reader.raw_reader();
            let len = raw_reader.read_raw_len::<MAX_TX_STRICT_SIZE>()?;
            let data = raw_reader.read_raw::<MAX_TX_STRICT_SIZE>(len)?;
            // Deserialize using consensus decoding (Tondi format)
            Tx::consensus_deserialize(data).map_err(|e| {
                DecodeError::DataIntegrityError(format!("Consensus decode error: {}", e))
            })
        }
    }
}

impl Tx {
    #[inline]
    pub fn inputs(&self) -> slice::Iter<'_, TxIn> {
        self.inputs.iter()
    }

    #[inline]
    pub fn outputs(&self) -> slice::Iter<'_, TxOut> {
        self.outputs.iter()
    }

    #[inline]
    pub fn is_segwit(&self) -> bool {
        self.inputs().any(|txin| !txin.witness.is_empty())
    }

    #[inline]
    pub fn to_unsigned_tx(&self) -> Tx {
        let mut tx = self.clone();
        for input in &mut tx.inputs {
            input.sig_script = SigScript::empty();
            input.witness = empty!();
        }
        tx
    }

    /// Computes a "normalized TXID" which does not include any signatures.
    ///
    /// This gives a way to identify a transaction that is "the same" as
    /// another in the sense of having same inputs and outputs.
    pub fn ntxid(&self) -> [u8; 32] {
        self.to_unsigned_tx().txid().to_byte_array()
    }

    /// Computes the [`Txid`].
    ///
    /// For Tondi L1: Uses BLAKE3 with Prefix Mode (b"TransactionID" prefix).
    /// Commits input kinds and optional output Datum, excluding mass and spending witnesses.
    ///
    /// ⚠️ CRITICAL: This method uses Tondi's hash format (different from Borsh serialization).
    /// Hash format rules:
    /// - u64 lengths (NOT u32)
    /// - One input kind byte: consuming 0, reference 1
    /// - EXCLUDE Mass field
    /// - EXCLUDE SigScript (write length 0, no bytes)
    pub fn txid(&self) -> Txid {
        // 1. Hasher (Prefix: b"TransactionID")
        let mut hasher = TondiBlake3::for_transaction();

        // 2. Version: u16 (2 bytes)
        hasher.update(&self.version.to_consensus_u16().to_le_bytes());

        // 3. Inputs with explicit kind tags
        // Count(u64) followed by kind-tagged inputs
        hasher.update(&(self.inputs.len() as u64).to_le_bytes());
        for input in self.inputs.iter() {
            hasher.update(&[u8::from(input.is_reference)]);
            // 3a. Previous Outpoint
            hasher.update(&tondi_native_txid_bytes(input.prev_output.txid));
            hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());

            // 3b. Signature Script: TxID calculation EXCLUDES SigScript
            // Write length 0 (u64), no bytes
            if !input.is_reference {
                hasher.update(&0u64.to_le_bytes());
            }

            // 3c. Sequence: u64
            hasher.update(&tondi_sequence_from_bp(input.sequence).to_le_bytes());

            // Note: SigOpCount is NOT included in hash calculation
        }

        // 4. Outputs (u64 len, NO enum tag)
        // Tondi hash format: Count(u64) + [Output...] (NO u8 tag)
        hasher.update(&(self.outputs.len() as u64).to_le_bytes());
        for output in self.outputs.iter() {
            // 4a. Value: u64
            hasher.update(&output.value.sats().to_le_bytes());

            // 4b. ScriptPublicKey
            // Tondi hash format: Version(u16) + ScriptLength(u64) + ScriptBytes
            hasher.update(&output.script_pubkey_version.to_le_bytes());

            let script_bytes = output.script_pubkey.as_slice();
            hasher.update(&(script_bytes.len() as u64).to_le_bytes()); // u64 length
            hasher.update(script_bytes);
            hasher.update(&[u8::from(output.datum.is_some())]);
            if let Some(datum) = &output.datum {
                hasher.update(&(datum.len() as u64).to_le_bytes());
                hasher.update(datum.as_slice());
            }
        }

        // 5. LockTime: u64 (8 bytes)
        hasher.update(&self.lock_time.to_consensus_u64().to_le_bytes());

        // 6. Subnetwork ID (20 bytes)
        hasher.update(&self.subnetwork_id);

        // 7. Gas: u64
        hasher.update(&self.gas.to_le_bytes());

        // 8. Payload: u64 Len + Bytes
        hasher.update(&(self.payload.len() as u64).to_le_bytes());
        hasher.update(&self.payload);

        // ❌ Note: Mass is NOT included in TxID calculation!

        // Get the raw hash result
        let raw_hash = hasher.finish();

        // ⚠️ CRITICAL: Bytes32StrRev reverses byte order when converting from byte array
        // So we need to reverse the hash bytes before passing to Txid::from_byte_array()
        // This ensures that after Bytes32StrRev reverses it again, we get the correct order
        let mut reversed_hash = raw_hash;
        reversed_hash.reverse();

        Txid::from_byte_array(reversed_hash)
    }

    /// Computes the Tondi transaction ID using BLAKE3.
    ///
    /// This is the canonical method for Tondi L1 transactions.
    ///
    /// Uses Prefix Mode with `b"TransactionID"` prefix to match Tondi's native implementation
    /// in `crypto/hashes/src/hashers.rs`. The hash is computed as `blake3("TransactionID" +
    /// tx_data)`.
    ///
    /// Returns a `TondiTxid` which uses BLAKE3 hash algorithm, compatible with RGB's
    /// `WitnessId` trait requirements.
    ///
    /// This method uses the same logic as `txid()` but returns a `TondiTxid` type.
    pub fn tondi_txid(&self) -> TondiTxid {
        // Use the same serialization logic as txid()
        TondiTxid::from_byte_array(self.txid().to_byte_array())
    }

    /// Computes the segwit version of the transaction id.
    ///
    /// For Tondi L1: Uses BLAKE3 with "tondi.wtxid" domain separation.
    ///
    /// Hashes the transaction **including** all segwit data (i.e. the marker,
    /// flag bytes, and the witness fields themselves). For non-segwit
    /// transactions which do not have any segwit data, this will be equal
    /// to [`Transaction::txid()`].
    pub fn wtxid(&self) -> Wtxid {
        // Tondi L1: Use BLAKE3 with domain separation
        let mut hasher = TondiBlake3::from_context("tondi.wtxid");
        self.consensus_encode(&mut hasher).expect("engines don't error");
        Wtxid::from_byte_array(hasher.finish())
    }
}

pub(crate) fn tondi_native_txid_bytes(txid: Txid) -> [u8; 32] {
    txid.to_string()
        .parse::<Bytes32>()
        .expect("BP txid display must be valid 32-byte Tondi txid hex")
        .to_byte_array()
}

pub(crate) fn bp_txid_from_tondi_native_bytes(bytes: [u8; 32]) -> Txid {
    Bytes32::from_byte_array(bytes)
        .to_string()
        .parse::<Txid>()
        .expect("Tondi txid bytes must be valid BP txid display hex")
}

pub(crate) fn tondi_sequence_from_bp(sequence: SeqNo) -> u64 {
    match sequence.to_consensus_u32() {
        u32::MAX => u64::MAX,
        sequence => sequence as u64,
    }
}

pub(crate) fn bp_sequence_from_tondi(sequence: u64) -> Option<SeqNo> {
    match sequence {
        u64::MAX => Some(SeqNo::from_consensus_u32(u32::MAX)),
        sequence if sequence < u32::MAX as u64 => Some(SeqNo::from_consensus_u32(sequence as u32)),
        _ => None,
    }
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use super::*;

    #[test]
    fn txid_byteorder() {
        let hex = "ed9f6388c0360c1861d331a0388d5a54815dd720cc67fa783c348217a0e943ca";
        let from_str = Txid::from_str(hex).unwrap();
        let from_hex = Txid::from_hex(hex).unwrap();
        assert_eq!(from_str, from_hex);
        assert_eq!(from_str.to_string(), from_str.to_hex());
        assert_eq!(from_str.to_string(), hex);
        assert_eq!(format!("{from_str:x}"), hex);
        assert_eq!(from_str[0], 0xca);
    }

    #[test]
    fn sats() {
        assert_eq!(Sats(0).0, 0);
        assert_eq!(Sats(0).coin_round(), 0);
        assert_eq!(Sats(0).coin_ceil(), 0);
        assert_eq!(Sats(0).coin_floor(), 0);
        assert_eq!(Sats(0).sats(), 0);
        assert_eq!(Sats(0).sats_rem(), 0);

        assert_eq!(Sats(1000).0, 1000);
        assert_eq!(Sats(1000).coin_round(), 0);
        assert_eq!(Sats(1000).coin_ceil(), 1);
        assert_eq!(Sats(1000).coin_floor(), 0);
        assert_eq!(Sats(1000).sats(), 1000);
        assert_eq!(Sats(1000).sats_rem(), 1000);

        assert_eq!(Sats(49_999_999).coin_round(), 0);
        assert_eq!(Sats(49_999_999).coin_ceil(), 1);
        assert_eq!(Sats(49_999_999).coin_floor(), 0);
        assert_eq!(Sats(50_000_000).0, 50_000_000);
        assert_eq!(Sats(50_000_000).coin_round(), 1);
        assert_eq!(Sats(50_000_000).coin_ceil(), 1);
        assert_eq!(Sats(50_000_000).coin_floor(), 0);
        assert_eq!(Sats(50_000_000).sats(), 50_000_000);
        assert_eq!(Sats(50_000_000).sats_rem(), 50_000_000);

        assert_eq!(Sats(99_999_999).coin_round(), 1);
        assert_eq!(Sats(99_999_999).coin_ceil(), 1);
        assert_eq!(Sats(99_999_999).coin_floor(), 0);
        assert_eq!(Sats(100_000_000), Sats::from_tondi(1));
        assert_eq!(Sats(100_000_000).0, 100_000_000);
        assert_eq!(Sats(100_000_000).coin_round(), 1);
        assert_eq!(Sats(100_000_000).coin_ceil(), 1);
        assert_eq!(Sats(100_000_000).coin_floor(), 1);
        assert_eq!(Sats(100_000_000).sats(), 100_000_000);
        assert_eq!(Sats(100_000_000).sats_rem(), 0);
        assert_eq!(Sats(100_000_001).sats(), 100_000_001);
        assert_eq!(Sats(100_000_001).sats_rem(), 1);
        assert_eq!(Sats(110_000_000).sats(), 110_000_000);
        assert_eq!(Sats(110_000_000).sats_rem(), 10_000_000);
    }
    #[test]
    fn tondi_txid_basic() {
        // Test basic TondiTxid creation and conversion
        let bytes = [0x42u8; 32];
        let tondi_txid = TondiTxid::from_byte_array(bytes);
        assert_eq!(tondi_txid.to_byte_array(), bytes);

        // Test conversion from Txid
        let txid = Txid::from_byte_array(bytes);
        let tondi_txid_from_txid: TondiTxid = txid.into();
        assert_eq!(tondi_txid, tondi_txid_from_txid);

        // Test conversion to Txid
        let txid_from_tondi: Txid = tondi_txid.into();
        assert_eq!(txid, txid_from_tondi);

        // Test coinbase
        let coinbase = TondiTxid::coinbase();
        assert!(coinbase.is_coinbase());
        assert_eq!(coinbase.to_byte_array(), [0u8; 32]);
    }

    #[test]
    fn tondi_txid_from_tx() {
        // Test that tondi_txid() method works
        let tx = Tx {
            version: TxVer::CURRENT,
            inputs: VarIntArray::new(),
            outputs: VarIntArray::new(),
            lock_time: LockTime::ZERO,
            subnetwork_id: [0u8; 20], // Default subnetwork
            gas: 0,
            payload: VarIntBytes::new(),
            mass: 0, // Default mass
        };
        let tondi_txid = tx.tondi_txid();
        // Just verify it doesn't panic and returns a valid TondiTxid
        assert!(!tondi_txid.is_coinbase() || tx.inputs.is_empty());
    }

    #[test]
    fn tondi_outpoint_basic() {
        // Test basic TondiOutpoint creation
        let txid_bytes = [0x42u8; 32];
        let tondi_txid = TondiTxid::from_byte_array(txid_bytes);
        let vout = Vout::from_u32(5);
        let tondi_outpoint = TondiOutpoint::new(tondi_txid, vout);

        assert_eq!(tondi_outpoint.txid, tondi_txid);
        assert_eq!(tondi_outpoint.vout, vout);
        assert_eq!(tondi_outpoint.vout_u32(), 5);
        assert!(!tondi_outpoint.is_coinbase());
    }

    #[test]
    fn tondi_outpoint_coinbase() {
        // Test coinbase outpoint
        let coinbase = TondiOutpoint::coinbase();
        assert!(coinbase.is_coinbase());
        assert_eq!(coinbase.txid.to_byte_array(), [0u8; 32]);
        assert_eq!(coinbase.vout_u32(), 0);
    }

    #[test]
    fn tondi_outpoint_conversion() {
        // Test conversion between Outpoint and TondiOutpoint
        let txid_bytes = [0xABu8; 32];
        let txid = Txid::from_byte_array(txid_bytes);
        let vout = Vout::from_u32(3);
        let outpoint = Outpoint::new(txid, vout);

        // Convert to TondiOutpoint
        let tondi_outpoint: TondiOutpoint = outpoint.into();
        assert_eq!(tondi_outpoint.txid.to_byte_array(), txid_bytes);
        assert_eq!(tondi_outpoint.vout, vout);

        // Convert back to Outpoint
        let outpoint_back: Outpoint = tondi_outpoint.into();
        assert_eq!(outpoint_back.txid.to_byte_array(), txid_bytes);
        assert_eq!(outpoint_back.vout, vout);
    }

    #[test]
    fn tondi_outpoint_serialization_format() {
        // Test that TondiOutpoint serialization format matches Outpoint
        use crate::coding::{ConsensusDecode, ConsensusEncode};

        let txid_bytes = [0xCDu8; 32];
        let txid = Txid::from_byte_array(txid_bytes);
        let tondi_txid = TondiTxid::from_byte_array(txid_bytes);
        let vout = Vout::from_u32(7);

        let outpoint = Outpoint::new(txid, vout);
        let tondi_outpoint = TondiOutpoint::new(tondi_txid, vout);

        // Serialize both
        let outpoint_bytes = outpoint.consensus_serialize();
        let tondi_outpoint_bytes = tondi_outpoint.consensus_serialize();

        // Both should have the same format: 32 bytes txid + 4 bytes vout = 36 bytes
        assert_eq!(outpoint_bytes.len(), 36);
        assert_eq!(tondi_outpoint_bytes.len(), 36);

        // The format should be identical (same txid bytes + same vout)
        assert_eq!(outpoint_bytes, tondi_outpoint_bytes);

        // Deserialize TondiOutpoint from Outpoint bytes (should work due to same format)
        let deserialized: TondiOutpoint =
            TondiOutpoint::consensus_deserialize(outpoint_bytes.clone()).unwrap();
        assert_eq!(deserialized.txid.to_byte_array(), txid_bytes);
        assert_eq!(deserialized.vout, vout);

        // Deserialize Outpoint from TondiOutpoint bytes (should work due to same format)
        let deserialized_outpoint: Outpoint =
            Outpoint::consensus_deserialize(tondi_outpoint_bytes.clone()).unwrap();
        assert_eq!(deserialized_outpoint.txid.to_byte_array(), txid_bytes);
        assert_eq!(deserialized_outpoint.vout, vout);
    }
}
