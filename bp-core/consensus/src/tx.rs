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

use std::fmt::{self, Debug, Display, Formatter, LowerHex};
use std::iter::Sum;
use std::num::ParseIntError;
use std::ops::{Div, Rem};
use std::str::FromStr;

use amplify::hex::{self, FromHex, ToHex};
use amplify::{ByteArray, Bytes32, Wrapper};
use strict_encoding::{DecodeError, StrictDecode, StrictDumb, StrictEncode, TypedRead};

use crate::{ConsensusDecode, ConsensusDecodeError, Wtxid, LIB_NAME_TONDI};

pub const TONDI_TX_VERSION: u16 = tondi_vm::FORMAT_VERSION;
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
    Bytes32,
);

impl From<Txid> for [u8; 32] {
    fn from(txid: Txid) -> Self {
        txid.to_byte_array()
    }
}

impl Txid {
    #[inline]
    pub const fn coinbase() -> Self {
        Self(Bytes32::zero())
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
    Bytes32,
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
        Self(Bytes32::from_byte_array(bytes))
    }

    /// Converts the TondiTxid to a byte array.
    #[inline]
    pub fn to_byte_array(self) -> [u8; 32] {
        self.0.to_byte_array()
    }

    /// Returns the coinbase transaction ID (all zeros).
    #[inline]
    pub const fn coinbase() -> Self {
        Self(Bytes32::zero())
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

pub use tondi_vm::{Input as TxIn, StateNote as TxOut};

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

/// Complete native v4 transaction with canonical bytes and an immutable parsed view.
/// Strict storage adds a bounded byte-vector frame, never a second transaction codec.
#[derive(Clone, Debug, StrictType, StrictEncode)]
#[strict_type(lib = LIB_NAME_TONDI, rename = "NativeTransactionV4")]
pub struct Tx {
    wire: amplify::confinement::Confined<Vec<u8>, 0, 1048576>,
    #[strict_type(skip)]
    native: tondi_vm::Transition,
}
impl strict_encoding::StrictSerialize for Tx {}
impl strict_encoding::StrictDeserialize for Tx {}
impl PartialEq for Tx {
    fn eq(&self, other: &Self) -> bool {
        self.wire == other.wire
    }
}
impl Eq for Tx {}
impl std::hash::Hash for Tx {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.wire, state)
    }
}
impl std::ops::Deref for Tx {
    type Target = tondi_vm::Transition;
    fn deref(&self) -> &Self::Target {
        &self.native
    }
}
impl Tx {
    pub fn from_native(native: tondi_vm::Transition) -> Result<Self, tondi_vm::Error> {
        let bytes = tondi_vm::codec::encode(&native);
        tondi_vm::Transition::decode(&bytes)?;
        let wire = amplify::confinement::Confined::try_from(bytes)
            .map_err(|_| tondi_vm::Error::Limit("native transaction bytes"))?;
        Ok(Self { wire, native })
    }
    pub fn native(&self) -> &tondi_vm::Transition {
        &self.native
    }
    pub fn into_native(self) -> tondi_vm::Transition {
        self.native
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, tondi_vm::Error> {
        Self::from_native(tondi_vm::Transition::decode(bytes)?)
    }
    pub fn wire_bytes(&self) -> &[u8] {
        self.wire.as_slice()
    }
    pub fn inputs(&self) -> std::slice::Iter<'_, TxIn> {
        self.native.inputs.iter()
    }
    pub fn outputs(&self) -> std::slice::Iter<'_, TxOut> {
        self.native.outputs.iter()
    }
    pub fn txid(&self) -> Txid {
        Txid::from_byte_array(self.native.id().as_bytes())
    }
    pub fn tondi_txid(&self) -> TondiTxid {
        self.txid().into()
    }
    pub fn ntxid(&self) -> [u8; 32] {
        self.native.id().as_bytes()
    }
    pub fn wtxid(&self) -> Wtxid {
        Wtxid::from_byte_array(self.native.witness_id().as_bytes())
    }
    pub fn to_unsigned_tx(&self) -> Self {
        let mut tx = self.native.clone();
        for data in &mut tx.witness.data {
            data.signatures.clear();
        }
        Self::from_native(tx).expect("bounded existing native transaction")
    }
}
impl StrictDumb for Tx {
    fn strict_dumb() -> Self {
        let entry = tondi_vm::ResolvedInput {
            note: tondi_vm::StateNote::plain(100_000, tondi_vm::public_auth().0),
            birth_daa_score: 0,
            is_coinbase: false,
        };
        let draft = tondi_vm::builder::NativeDraft {
            network: tondi_vm::Hash32::default(),
            inputs: vec![tondi_vm::builder::DraftInput {
                outpoint: tondi_vm::OutPoint::new(1.into(), 0),
                entry,
                auth: None,
                since: 0,
                consume: true,
            }],
            outputs: vec![tondi_vm::StateNote::plain(87_000, tondi_vm::public_auth().0)],
            programs: vec![],
            payload: vec![],
            cycles_limit: 1_000_000,
            invocation_cycles_limit: 1_000_000,
        };
        Self::from_native(draft.prepare().expect("native dumb template").transaction)
            .expect("native dumb encoding")
    }
}
impl StrictDecode for Tx {
    fn strict_decode(reader: &mut impl TypedRead) -> Result<Self, DecodeError> {
        use strict_encoding::{fname, ReadStruct};
        reader.read_struct::<Self>(|r| {
            let wire: amplify::confinement::Confined<Vec<u8>, 0, 1048576> =
                r.read_field(fname!("wire"))?;
            Self::from_bytes(wire.as_slice())
                .map_err(|e| DecodeError::DataIntegrityError(e.to_string()))
        })
    }
}
impl LowerHex for Tx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire.to_hex())
    }
}
impl Display for Tx {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(self, f)
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
        Self::consensus_deserialize(Vec::<u8>::from_hex(s)?).map_err(BlockDataParseError::from)
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for Tx {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.native.serialize(s)
    }
}
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Tx {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let native = tondi_vm::Transition::deserialize(d)?;
        Self::from_native(native).map_err(serde::de::Error::custom)
    }
}
impl From<Outpoint> for tondi_vm::OutPoint {
    fn from(p: Outpoint) -> Self {
        Self::new(tondi_vm::Hash32::from_bytes(p.txid.to_byte_array()), p.vout.to_u32())
    }
}
impl From<tondi_vm::OutPoint> for Outpoint {
    fn from(p: tondi_vm::OutPoint) -> Self {
        Self::new(Txid::from_byte_array(p.transaction_id.as_bytes()), p.index)
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
        assert_eq!(from_str[0], 0xed);
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
        let tx = Tx::strict_dumb();
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
