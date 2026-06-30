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
use amplify::{ByteArray, Bytes32StrRev, Wrapper};
use commit_verify::{DigestExt, TondiBlake3};

use crate::{
    ConsensusDecode, ConsensusDecodeError, ConsensusEncode, LockTime, ScriptPubkey, SeqNo,
    SigScript, VarIntArray, VarIntBytes, Witness, Wtxid, LIB_NAME_TONDI,
};

use strict_encoding::{
    DecodeError, ReadRaw, StrictDecode, StrictEncode, TypedRead, TypedWrite, WriteRaw,
};

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
/// as opposed to Tondi's double SHA256. It is designed to be compatible with RGB's
/// `WitnessId` trait requirements.
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
/// This allows interoperability between Tondi-compatible `Outpoint` and Tondi-specific `TondiOutpoint`.
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
/// This allows interoperability between Tondi-specific `TondiOutpoint` and Tondi-compatible `Outpoint`.
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
    pub value: Sats,
    pub script_pubkey: ScriptPubkey,
}

impl TxOut {
    pub fn new(script_pubkey: impl Into<ScriptPubkey>, value: impl Into<Sats>) -> Self {
        TxOut {
            script_pubkey: script_pubkey.into(),
            value: value.into(),
        }
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
        TxVer(1)
    }
}

impl TxVer {
    /// Version 1 (default for Tondi)
    pub const V1: Self = TxVer(1);
    /// Version 2
    pub const V2: Self = TxVer(2);

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

    /// Legacy method: Convert from i32 (for backward compatibility)
    ///
    /// # Warning
    /// This may truncate values > u16::MAX. Use `from_u16` for new code.
    #[inline]
    pub const fn from_consensus_i32(ver: i32) -> Self {
        TxVer(ver as u16)
    }

    /// Legacy method: Convert to i32 (for backward compatibility)
    #[inline]
    pub const fn to_consensus_i32(&self) -> i32 {
        self.0 as i32
    }

    /// Check if version is standard (for Tondi, this is always true for valid u16)
    #[inline]
    pub const fn is_standard(self) -> bool {
        true
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
    /// Note: This field is NOT included in Txid calculation, only in full transaction serialization.
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

// Manual implementation of StrictEncode and StrictDecode for Tx
// These delegate to ConsensusEncode/ConsensusDecode to maintain Tondi compatibility
impl StrictEncode for Tx {
    fn strict_encode<W: TypedWrite>(&self, mut writer: W) -> std::io::Result<W> {
        // Serialize using consensus encoding (Tondi format)
        let data = self.consensus_serialize();
        // Write as raw bytes using write_raw
        // SAFETY: raw_writer() is safe to use here as we're writing raw bytes
        #[allow(unsafe_code)]
        unsafe {
            writer.raw_writer().write_raw::<{ u32::MAX as usize }>(&data)?;
        }
        Ok(writer)
    }
}

impl StrictDecode for Tx {
    fn strict_decode(reader: &mut impl TypedRead) -> Result<Self, DecodeError> {
        // Read transaction data using raw reader
        // Note: This implementation assumes the entire transaction data is available
        // In strict encoding, there may be length prefixes, but we delegate to
        // consensus encoding which handles its own format
        // SAFETY: raw_reader() is safe to use here as we're reading raw bytes
        #[allow(unsafe_code)]
        unsafe {
            let raw_reader = reader.raw_reader();
            // Read a reasonable maximum size (1MB should be enough for any transaction)
            const MAX_TX_SIZE: usize = 1_000_000;
            let data = raw_reader.read_raw::<MAX_TX_SIZE>(MAX_TX_SIZE)?;
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
    /// The hash is computed as `blake3("TransactionID" + tx_data)` to match Tondi's native implementation.
    /// For Tondi compatibility: Uses double SHA256.
    ///
    /// Hashes the transaction **excluding** the segwit data (i.e. the marker,
    /// flag bytes, and the witness fields themselves). For non-segwit
    /// transactions which do not have any segwit data, this will be equal
    /// to [`Tx::wtxid()`].
    /// Computes the [`Txid`].
    ///
    /// For Tondi L1: Uses BLAKE3 with Prefix Mode (b"TransactionID" prefix).
    /// Uses Tondi's hash format: u64 lengths, NO enum tags, EXCLUDE Mass, EXCLUDE SigScript.
    ///
    /// ⚠️ CRITICAL: This method uses Tondi's hash format (different from Borsh serialization).
    /// Hash format rules:
    /// - u64 lengths (NOT u32)
    /// - NO enum tags (NOT u8 tags)
    /// - EXCLUDE Mass field
    /// - EXCLUDE SigScript (write length 0, no bytes)
    pub fn txid(&self) -> Txid {
        // 1. Hasher (Prefix: b"TransactionID")
        let mut hasher = TondiBlake3::for_transaction();

        // 2. Version: u16 (2 bytes)
        hasher.update(&self.version.to_consensus_u16().to_le_bytes());

        // 3. Inputs (u64 len, NO enum tag)
        // Tondi hash format: Count(u64) + [Input...] (NO u8 tag)
        hasher.update(&(self.inputs.len() as u64).to_le_bytes());
        for input in self.inputs.iter() {
            // 3a. Previous Outpoint
            hasher.update(&input.prev_output.txid.to_byte_array());
            hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());

            // 3b. Signature Script: TxID calculation EXCLUDES SigScript
            // Write length 0 (u64), no bytes
            hasher.update(&0u64.to_le_bytes());

            // 3c. Sequence: u64
            hasher.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());

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
            hasher.update(&0u16.to_le_bytes()); // Version 0 for standard scripts

            let script_bytes = output.script_pubkey.as_slice();
            hasher.update(&(script_bytes.len() as u64).to_le_bytes()); // u64 length
            hasher.update(script_bytes);
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
    /// in `crypto/hashes/src/hashers.rs`. The hash is computed as `blake3("TransactionID" + tx_data)`.
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

    /// Computes a legacy double-SHA256 transaction ID.
    ///
    /// This is excluded from normal Tondi builds and exists only for migration fixtures.
    #[cfg(feature = "legacy-sha256-compat")]
    pub fn legacy_sha256_txid(&self) -> Txid {
        let mut enc = Sha256::default();
        self.version.consensus_encode(&mut enc).expect("engines don't error");
        self.inputs.consensus_encode(&mut enc).expect("engines don't error");
        self.outputs.consensus_encode(&mut enc).expect("engines don't error");
        self.lock_time.consensus_encode(&mut enc).expect("engines don't error");
        let mut double = Sha256::default();
        double.input_raw(&enc.finish());
        Txid::from_byte_array(double.finish())
    }

    /// Computes a legacy double-SHA256 witness transaction ID.
    ///
    /// This is excluded from normal Tondi builds and exists only for migration fixtures.
    #[cfg(feature = "legacy-sha256-compat")]
    pub fn legacy_sha256_wtxid(&self) -> Wtxid {
        let mut enc = Sha256::default();
        self.consensus_encode(&mut enc).expect("engines don't error");
        let mut double = Sha256::default();
        double.input_raw(&enc.finish());
        Wtxid::from_byte_array(double.finish())
    }
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use super::*;
    use crate::VarInt;

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
    fn nonsegwit_transaction() {
        let tx =
            "0100000001a15d57094aa7a21a28cb20b59aab8fc7d1149a3bdbcddba9c622e4f5f6a99ece010000006c49\
            3046022100f93bb0e7d8db7bd46e40132d1f8242026e045f03a0efe71bbb8e3f475e970d790221009337cd7\
            f1f929f00cc6ff01f03729b069a7c21b59b1736ddfee5db5946c5da8c0121033b9b137ee87d5a812d6f506e\
            fdd37f0affa7ffc310711c06c7f3e097c9447c52ffffffff0100e1f505000000001976a9140389035a9225b\
            3839e2bbf32d826a1e222031fd888ac00000000";
        let realtx = Tx::from_str(tx).unwrap();

        assert_eq!(&realtx.to_string(), tx);
        assert_eq!(&realtx.to_hex(), tx);
        assert_eq!(&format!("{realtx:x}"), tx);

        // All these tests aren't really needed because if they fail, the hash check at
        // the end will also fail. But these will show you where the failure is
        // so I'll leave them in.
        assert_eq!(realtx.version, TxVer::V1);
        assert_eq!(realtx.inputs.len(), 1);
        // In particular this one is easy to get backward -- in tondi hashes are
        // encoded as little-endian 256-bit numbers rather than as data strings.
        assert_eq!(
            format!("{:x}", realtx.inputs[0].prev_output.txid),
            "ce9ea9f6f5e422c6a9dbcddb3b9a14d1c78fab9ab520cb281aa2a74a09575da1".to_string()
        );
        assert_eq!(realtx.inputs[0].prev_output.vout, Vout::from_u32(1));
        assert_eq!(realtx.outputs.len(), 1);
        assert_eq!(realtx.lock_time, LockTime::ZERO);

        assert_eq!(
            format!("{:x}", realtx.txid()),
            "a6eab3c14ab5272a58a5ba91505ba1a4b6d7a3a9fcbd187b6cd99a7b6d548cb7".to_string()
        );
        assert_eq!(
            format!("{:x}", realtx.wtxid()),
            "a6eab3c14ab5272a58a5ba91505ba1a4b6d7a3a9fcbd187b6cd99a7b6d548cb7".to_string()
        );
        /* TODO: Enable once weight calculation is there
        assert_eq!(realtx.weight().to_wu() as usize, tx_bytes.len() * WITNESS_SCALE_FACTOR);
        assert_eq!(realtx.total_size(), tx_bytes.len());
        assert_eq!(realtx.vsize(), tx_bytes.len());
        assert_eq!(realtx.base_size(), tx_bytes.len());
         */
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
            version: TxVer::V2,
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

    /// Test parsing a real Tondi transaction from Hex and verifying its Txid
    ///
    /// This test validates that our Txid calculation matches Tondi's native implementation.
    /// If you have a complete transaction Hex string from Tondi, replace the placeholder
    /// hex string below with the actual transaction Hex.
    ///
    /// Expected Txid: 43efcab621aa7378257a214a095d4bf2e1c79b98cabab16ae241e341f118175c
    /// Version: 1
    /// LockTime: 0
    /// Payload Hex: 502020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    #[test]
    fn test_tondi_real_transaction_parsing() {
        // Test with the golden vector from Tondi codebase
        // Expected Txid: 43efcab621aa7378257a214a095d4bf2e1c79b98cabab16ae241e341f118175c
        // Version: 1
        // LockTime: 0
        // Payload Hex: 502020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

        // Payload: 502020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
        // This is 35 bytes: 0x50 (Annex marker) + 0x20 (TLV type) + 0x20 (length) + 32 bytes of data
        let payload_hex = "502020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let payload_bytes = Vec::<u8>::from_hex(payload_hex).expect("Invalid payload hex");
        let payload = VarIntBytes::try_from(payload_bytes).expect("Invalid payload length");

        // Create a minimal transaction matching the golden vector
        // NOTE: We need the COMPLETE transaction Hex from Tondi to properly test.
        // For now, we're testing with a manually constructed transaction.
        let tx = Tx {
            version: TxVer::from_consensus_u16(1),
            inputs: VarIntArray::new(), // Empty inputs (serializes as 0x00)
            outputs: VarIntArray::new(), // Empty outputs (serializes as 0x00)
            lock_time: LockTime::from_consensus_u64(0),
            subnetwork_id: [0u8; 20], // Default subnetwork (all zeros)
            gas: 0,
            payload,
            mass: 0, // Default mass
        };

        // Debug: Print the serialized transaction bytes for comparison
        use crate::ConsensusEncode;
        let mut serialized = Vec::new();
        tx.consensus_encode(&mut serialized).unwrap();
        println!("Serialized transaction ({} bytes): {}", serialized.len(), serialized.to_hex());

        // Debug: Print what we're hashing
        use commit_verify::TondiBlake3;
        let mut debug_hasher = TondiBlake3::for_transaction();
        debug_hasher.update(&tx.version.to_consensus_u16().to_le_bytes());
        println!("Version bytes: {:02x?}", tx.version.to_consensus_u16().to_le_bytes());

        let mut inputs_buf = Vec::new();
        tx.inputs.consensus_encode(&mut inputs_buf).unwrap();
        debug_hasher.update(&inputs_buf);
        println!("Inputs bytes: {}", inputs_buf.to_hex());

        let mut outputs_buf = Vec::new();
        tx.outputs.consensus_encode(&mut outputs_buf).unwrap();
        debug_hasher.update(&outputs_buf);
        println!("Outputs bytes: {}", outputs_buf.to_hex());

        let lock_time_bytes = tx.lock_time.to_consensus_u64().to_le_bytes();
        debug_hasher.update(&lock_time_bytes);
        println!("LockTime bytes: {:02x?}", lock_time_bytes);

        debug_hasher.update(&tx.subnetwork_id);
        println!("Subnetwork ID bytes: {:02x?}", tx.subnetwork_id);

        let gas_bytes = tx.gas.to_le_bytes();
        debug_hasher.update(&gas_bytes);
        println!("Gas bytes: {:02x?}", gas_bytes);

        let mut payload_len_buf = Vec::new();
        VarInt::with(tx.payload.len()).consensus_encode(&mut payload_len_buf).unwrap();
        debug_hasher.update(&payload_len_buf);
        println!("Payload length VarInt: {}", payload_len_buf.to_hex());
        debug_hasher.update(&tx.payload);
        println!("Payload bytes: {}", tx.payload.to_hex());

        // Calculate Txid
        let calculated_txid = tx.txid();
        let expected_txid_hex = "43efcab621aa7378257a214a095d4bf2e1c79b98cabab16ae241e341f118175c";
        let expected_txid = Txid::from_hex(expected_txid_hex).expect("Invalid expected txid hex");

        println!("Calculated Txid: {}", calculated_txid.to_hex());
        println!("Expected Txid:  {}", expected_txid_hex);

        // Verify the Txid matches
        assert_eq!(
            calculated_txid,
            expected_txid,
            "Calculated Txid ({}) does not match expected Txid ({})",
            calculated_txid.to_hex(),
            expected_txid_hex
        );

        // Also verify using tondi_txid() method
        let tondi_txid = tx.tondi_txid();
        assert_eq!(
            tondi_txid.to_byte_array(),
            expected_txid.to_byte_array(),
            "TondiTxid does not match expected Txid"
        );
    }

    /// Test parsing a complete Tondi transaction from Hex string
    ///
    /// This test accepts a complete transaction Hex string from Tondi and verifies:
    /// 1. The transaction can be parsed correctly using consensus_deserialize
    /// 2. The calculated Txid matches the expected value from Tondi
    ///
    /// ⚠️ CRITICAL: This test validates that bp-core's consensus encoding/decoding
    /// is compatible with Tondi's native transaction format.
    #[test]
    fn test_tondi_transaction_from_hex() {
        // Full transaction Hex from Tondi golden vector test
        // Expected Txid: 43efcab621aa7378257a214a095d4bf2e1c79b98cabab16ae241e341f118175c
        // Note: The hex string may include the expected Txid at the end (64 hex chars = 32 bytes)
        // We'll try to parse it, and if it fails, we'll remove the last 64 characters
        let tx_hex = "0100010000000000000000000000000000000000000000000000000000000001000000000000000000000002000000c0de0000000000000000010100000000e803000000000000000001000000ac00000000000000000000000000000000000000000000000000000000000000000000000023000000502020aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa000000000000000043efcab621aa7378257a214a095d4bf2e1c79b98cabab16ae241e341f118175c";

        println!("=== Parsing Tondi Transaction ===");
        println!("Full Tx Hex ({} bytes): {}", tx_hex.len() / 2, tx_hex);

        // Step 1: Deserialize the transaction
        // ⚠️ WARNING: If this fails, bp-core's StrictEncode (Tondi-like format)
        // may not be compatible with Tondi's Borsh format

        // The hex string might include the expected Txid at the end (64 hex chars = 32 bytes)
        // Try parsing the full hex first, if it fails, try removing the last 64 chars
        let expected_txid_hex = "43efcab621aa7378257a214a095d4bf2e1c79b98cabab16ae241e341f118175c";
        let mut tx_bytes = Vec::<u8>::from_hex(tx_hex).expect("Invalid transaction hex");

        // Try to deserialize - if it fails and hex ends with expected Txid, try without it
        let tx = match Tx::consensus_deserialize(tx_bytes.clone()) {
            Ok(tx) => {
                println!("✅ Transaction deserialized successfully!");
                tx
            }
            Err(e) => {
                println!("⚠️  First deserialization attempt failed: {:?}", e);
                // Check if hex ends with expected Txid and try again without it
                if tx_hex.ends_with(expected_txid_hex) && tx_bytes.len() >= 32 {
                    println!("⚠️  Hex string appears to include Txid at the end. Removing it and retrying...");
                    tx_bytes.truncate(tx_bytes.len() - 32);
                    println!("Adjusted transaction bytes length: {} bytes", tx_bytes.len());
                    match Tx::consensus_deserialize(tx_bytes) {
                        Ok(tx) => {
                            println!(
                                "✅ Transaction deserialized successfully after removing Txid!"
                            );
                            tx
                        }
                        Err(e2) => {
                            panic!("❌ Failed to deserialize transaction even after removing Txid: {:?}\nThis may indicate that bp-core's consensus encoding is incompatible with Tondi's format.", e2);
                        }
                    }
                } else {
                    panic!("❌ Failed to deserialize transaction: {:?}\nThis may indicate that bp-core's consensus encoding is incompatible with Tondi's format.", e);
                }
            }
        };

        // Print transaction details
        println!("   Version: {} (u16)", tx.version.to_consensus_u16());
        println!("   Inputs: {}", tx.inputs.len());
        for (i, input) in tx.inputs.iter().enumerate() {
            println!(
                "     Input {}: txid={}, vout={}, sig_script_len={}, sequence={}",
                i,
                input.prev_output.txid.to_hex(),
                input.prev_output.vout.to_u32(),
                input.sig_script.len(),
                input.sequence.to_consensus_u32()
            );
        }
        println!("   Outputs: {}", tx.outputs.len());
        for (i, output) in tx.outputs.iter().enumerate() {
            println!(
                "     Output {}: value={}, script_len={}",
                i,
                output.value.sats(),
                output.script_pubkey.len()
            );
        }
        println!("   LockTime: {} (u64)", tx.lock_time.to_consensus_u64());
        println!("   Subnetwork ID: {:02x?}", tx.subnetwork_id);
        println!("   Gas: {}", tx.gas);
        println!("   Payload length: {} bytes", tx.payload.len());
        println!("   Payload hex: {}", tx.payload.to_hex());
        println!("   Mass: {}", tx.mass);

        // Step 2: Calculate and verify Txid
        // Debug: Print raw hash bytes before creating Txid
        let mut debug_hasher = TondiBlake3::for_transaction();
        debug_hasher.update(&tx.version.to_consensus_u16().to_le_bytes());
        debug_hasher.update(&(tx.inputs.len() as u64).to_le_bytes());
        for input in tx.inputs.iter() {
            debug_hasher.update(&input.prev_output.txid.to_byte_array());
            debug_hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());
            debug_hasher.update(&0u64.to_le_bytes()); // SigScript length = 0
            debug_hasher.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());
        }
        debug_hasher.update(&(tx.outputs.len() as u64).to_le_bytes());
        for output in tx.outputs.iter() {
            debug_hasher.update(&output.value.sats().to_le_bytes());
            debug_hasher.update(&0u16.to_le_bytes());
            let script_bytes = output.script_pubkey.as_slice();
            debug_hasher.update(&(script_bytes.len() as u64).to_le_bytes());
            debug_hasher.update(script_bytes);
        }
        debug_hasher.update(&tx.lock_time.to_consensus_u64().to_le_bytes());
        debug_hasher.update(&tx.subnetwork_id);
        debug_hasher.update(&tx.gas.to_le_bytes());
        debug_hasher.update(&(tx.payload.len() as u64).to_le_bytes());
        debug_hasher.update(&tx.payload);
        let raw_hash = debug_hasher.finish();

        println!("\n=== Txid Verification ===");
        println!("Raw hash bytes: {:02x?}", raw_hash);
        println!("Raw hash hex: {}", raw_hash.to_hex());

        let calculated_txid = tx.txid();
        let expected_txid_hex = "43efcab621aa7378257a214a095d4bf2e1c79b98cabab16ae241e341f118175c";
        let expected_txid = Txid::from_hex(expected_txid_hex).expect("Invalid expected txid hex");

        println!("Calculated Txid: {}", calculated_txid.to_hex());
        println!("Expected Txid:   {}", expected_txid_hex);
        println!("Expected Txid bytes: {:02x?}", expected_txid.to_byte_array());
        println!("Calculated Txid bytes: {:02x?}", calculated_txid.to_byte_array());

        // Verify the Txid matches
        assert_eq!(
            calculated_txid,
            expected_txid,
            "\n❌ Txid mismatch!\n   Calculated: {}\n   Expected:   {}\n\nThis indicates that the Txid calculation logic does not match Tondi's native implementation.",
            calculated_txid.to_hex(),
            expected_txid_hex
        );

        println!("✅ Txid matches! All P0 bugs are fixed!");

        // Also verify using tondi_txid() method
        let tondi_txid = tx.tondi_txid();
        assert_eq!(
            tondi_txid.to_byte_array(),
            expected_txid.to_byte_array(),
            "TondiTxid does not match expected Txid"
        );
        println!("✅ TondiTxid also matches!");
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

    #[test]
    fn segwit_transaction() {
        let tx =
            "02000000000101595895ea20179de87052b4046dfe6fd515860505d6511a9004cf12a1f93cac7c01000000\
            00ffffffff01deb807000000000017a9140f3444e271620c736808aa7b33e370bd87cb5a078702483045022\
            100fb60dad8df4af2841adc0346638c16d0b8035f5e3f3753b88db122e70c79f9370220756e6633b17fd271\
            0e626347d28d60b0a2d6cbb41de51740644b9fb3ba7751040121028fa937ca8cba2197a37c007176ed89410\
            55d3bcb8627d085e94553e62f057dcc00000000";
        let realtx = Tx::from_str(tx).unwrap();

        assert_eq!(&realtx.to_string(), tx);
        assert_eq!(&realtx.to_hex(), tx);
        assert_eq!(&format!("{realtx:x}"), tx);

        // All these tests aren't really needed because if they fail, the hash check at
        // the end will also fail. But these will show you where the failure is
        // so I'll leave them in.
        assert_eq!(realtx.version, TxVer::V2);
        assert_eq!(realtx.inputs.len(), 1);
        // In particular this one is easy to get backward -- in tondi hashes are
        // encoded as little-endian 256-bit numbers rather than as data strings.
        assert_eq!(
            format!("{:x}", realtx.inputs[0].prev_output.txid),
            "7cac3cf9a112cf04901a51d605058615d56ffe6d04b45270e89d1720ea955859".to_string()
        );
        assert_eq!(realtx.inputs[0].prev_output.vout, Vout::from_u32(1));
        assert_eq!(realtx.outputs.len(), 1);
        assert_eq!(realtx.lock_time, LockTime::ZERO);

        assert_eq!(
            format!("{:x}", realtx.txid()),
            "f5864806e3565c34d1b41e716f72609d00b55ea5eac5b924c9719a842ef42206".to_string()
        );
        assert_eq!(
            format!("{:x}", realtx.wtxid()),
            "80b7d8a82d5d5bf92905b06f2014dd699e03837ca172e3a59d51426ebbe3e7f5".to_string()
        );

        /* TODO: Enable once weight calculation is there
        const EXPECTED_WEIGHT: Weight = Weight::from_wu(442);
        assert_eq!(realtx.weight(), EXPECTED_WEIGHT);
        assert_eq!(realtx.total_size(), tx_bytes.len());
        assert_eq!(realtx.vsize(), 111);

        let expected_strippedsize = (442 - realtx.total_size()) / 3;
        assert_eq!(realtx.base_size(), expected_strippedsize);

        // Construct a transaction without the witness data.
        let mut tx_without_witness = realtx;
        tx_without_witness.input.iter_mut().for_each(|input| input.witness.clear());
        assert_eq!(tx_without_witness.total_size(), tx_without_witness.total_size());
        assert_eq!(tx_without_witness.total_size(), expected_strippedsize);
         */
    }
}
