// Bitcoin protocol consensus library.
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

use std::cmp::Ordering;
use std::fmt::{self, Display, Formatter};
use std::num::ParseIntError;
use std::str::FromStr;

use chrono::Utc;

use crate::LIB_NAME_TONDI;

/// The Threshold for deciding whether a lock time value is a height or a time.
///
/// `LockTime` values _below_ the threshold are interpreted as block heights,
/// values _above_ (or equal to) the threshold are interpreted as block times
/// (UNIX timestamp, seconds since epoch).
///
/// For Tondi, we use u64 for lock time, allowing much larger values than Bitcoin's u32.
/// The threshold remains the same conceptually, but is now u64.
pub const LOCKTIME_THRESHOLD: u64 = 500_000_000;

pub const SEQ_NO_CSV_DISABLE_MASK: u32 = 0x80000000;
pub const SEQ_NO_CSV_TYPE_MASK: u32 = 0x00400000;

/// Error constructing timelock from the provided value.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display, Error)]
#[display("invalid timelock value {0}")]
pub struct InvalidTimelock(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, From, Display)]
#[display(doc_comments)]
pub enum TimelockParseError {
    /// invalid number in time lock descriptor
    #[from]
    InvalidNumber(ParseIntError),

    /// block height `{0}` is too large for time lock
    InvalidHeight(u32),

    /// timestamp `{0}` is too small for time lock
    InvalidTimestamp(u32),

    /// time lock descriptor `{0}` is not recognized
    InvalidDescriptor(String),

    /// use of randomly-generated RBF sequence numbers requires compilation
    /// with `rand` feature
    NoRand,
}

/// Lock time for Tondi L1 transactions.
///
/// # Critical: Type Compatibility
///
/// Tondi uses `u64` (8 bytes) for lock time, not `u32` (4 bytes) like Bitcoin.
/// This type MUST match Tondi's native `Transaction.lock_time` field exactly
/// to ensure correct Txid calculation.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct LockTime(u64);

impl PartialOrd for LockTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.is_height_based() != other.is_height_based() {
            None
        } else {
            Some(self.0.cmp(&other.0))
        }
    }
}

impl LockTime {
    /// Zero time lock
    pub const ZERO: Self = Self(0);

    /// Create zero time lock
    #[inline]
    #[deprecated(since = "0.10.8", note = "use LockTime::ZERO")]
    pub const fn zero() -> Self { Self(0) }

    /// Creates absolute time lock with the given block height.
    ///
    /// Block height must be strictly less than `LOCKTIME_THRESHOLD`, otherwise
    /// `None` is returned.
    #[inline]
    pub const fn from_height(height: u64) -> Option<Self> {
        if height < LOCKTIME_THRESHOLD {
            Some(Self(height))
        } else {
            None
        }
    }

    /// Creates absolute time lock with the given UNIX timestamp value.
    ///
    /// Timestamp value must be greater or equal to `LOCKTIME_THRESHOLD`, otherwise
    /// `None` is returned.
    #[inline]
    pub const fn from_unix_timestamp(timestamp: u64) -> Option<Self> {
        if timestamp < LOCKTIME_THRESHOLD {
            None
        } else {
            Some(Self(timestamp))
        }
    }

    /// Converts into full u64 representation of `nLockTime` value as it is
    /// serialized in Tondi transaction.
    #[inline]
    pub const fn from_consensus_u64(lock_time: u64) -> Self { LockTime(lock_time) }

    #[inline]
    pub const fn to_consensus_u64(&self) -> u64 { self.0 }

    #[inline]
    pub const fn into_consensus_u64(self) -> u64 { self.0 }

    /// Legacy method: Convert from u32 (for backward compatibility)
    /// 
    /// # Warning
    /// This is safe as u32 can always fit in u64, but use `from_consensus_u64` for new code.
    #[inline]
    pub const fn from_consensus_u32(lock_time: u32) -> Self { LockTime(lock_time as u64) }

    /// Legacy method: Convert to u32 (for backward compatibility)
    /// 
    /// # Warning
    /// This may truncate values > u32::MAX. Use `to_consensus_u64` for new code.
    #[inline]
    pub const fn to_consensus_u32(&self) -> u32 { 
        if self.0 > u32::MAX as u64 {
            u32::MAX
        } else {
            self.0 as u32
        }
    }

    /// Legacy method: Convert to u32 (for backward compatibility)
    #[inline]
    pub const fn into_consensus_u32(self) -> u32 { self.to_consensus_u32() }

    /// Checks if the absolute timelock provided by the `nLockTime` value
    /// specifies height-based lock
    #[inline]
    pub const fn is_height_based(self) -> bool { self.0 < LOCKTIME_THRESHOLD }

    /// Checks if the absolute timelock provided by the `nLockTime` value
    /// specifies time-based lock
    #[inline]
    pub const fn is_time_based(self) -> bool { !self.is_height_based() }
}

/// Value for a transaction `nTimeLock` field which is guaranteed to represent a
/// UNIX timestamp which is always either 0 or a greater than or equal to
/// 500000000.
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash, Debug, Default)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct LockTimestamp(u32);

impl From<LockTimestamp> for u32 {
    fn from(lock_timestamp: LockTimestamp) -> Self { lock_timestamp.into_consensus_u32() }
}

impl From<LockTimestamp> for LockTime {
    fn from(lock: LockTimestamp) -> Self { LockTime::from_consensus_u32(lock.into_consensus_u32()) }
}

impl TryFrom<u32> for LockTimestamp {
    type Error = InvalidTimelock;

    fn try_from(value: u32) -> Result<Self, Self::Error> { Self::try_from_consensus_u32(value) }
}

impl TryFrom<LockTime> for LockTimestamp {
    type Error = InvalidTimelock;

    fn try_from(lock_time: LockTime) -> Result<Self, Self::Error> {
        Self::try_from_lock_time(lock_time)
    }
}

impl LockTimestamp {
    /// Create zero time lock
    #[inline]
    pub fn anytime() -> Self { Self(0) }

    #[cfg(feature = "chrono")]
    /// Creates absolute time lock valid since the current timestamp.
    pub fn since_now() -> Self {
        let now = Utc::now();
        LockTimestamp::from_unix_timestamp(now.timestamp() as u32)
            .expect("we are too far in the future")
    }

    /// Creates absolute time lock with the given UNIX timestamp value.
    ///
    /// Timestamp value must be greater or equal to `0x1DCD6500`, otherwise
    /// `None` is returned.
    #[inline]
    pub fn from_unix_timestamp(timestamp: u32) -> Option<Self> {
        if timestamp < LOCKTIME_THRESHOLD {
            None
        } else {
            Some(Self(timestamp))
        }
    }

    #[inline]
    pub const fn try_from_lock_time(lock_time: LockTime) -> Result<Self, InvalidTimelock> {
        Self::try_from_consensus_u32(lock_time.into_consensus_u32())
    }

    #[inline]
    pub const fn try_from_consensus_u32(lock_time: u32) -> Result<Self, InvalidTimelock> {
        if !LockTime::from_consensus_u32(lock_time).is_time_based() {
            return Err(InvalidTimelock(lock_time));
        }
        Ok(Self(lock_time))
    }

    /// Converts into full u32 representation of `nLockTime` value as it is
    /// serialized in bitcoin transaction.
    #[inline]
    pub const fn to_consensus_u32(&self) -> u32 { self.0 }

    /// Converts into full u32 representation of `nLockTime` value as it is
    /// serialized in bitcoin transaction.
    #[inline]
    pub const fn into_consensus_u32(self) -> u32 { self.0 }

    /// Converts into [`LockTime`] representation.
    #[inline]
    pub fn into_lock_time(self) -> LockTime { self.into() }

    /// Converts into [`LockTime`] representation.
    #[inline]
    pub fn to_lock_time(self) -> LockTime { self.into_lock_time() }
}

impl Display for LockTimestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("time(")?;
        Display::fmt(&self.0, f)?;
        f.write_str(")")
    }
}

impl FromStr for LockTimestamp {
    type Err = TimelockParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        if s == "0" || s == "none" {
            Ok(LockTimestamp::anytime())
        } else if s.starts_with("time(") && s.ends_with(')') {
            let no = s[5..].trim_end_matches(')').parse()?;
            LockTimestamp::try_from(no).map_err(|_| TimelockParseError::InvalidTimestamp(no))
        } else {
            Err(TimelockParseError::InvalidDescriptor(s))
        }
    }
}

/// Value for a transaction `nTimeLock` field which is guaranteed to represent a
/// block height number which is always less than 500000000.
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Hash, Debug, Default)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct LockHeight(u32);

impl From<LockHeight> for u32 {
    fn from(lock_height: LockHeight) -> Self { lock_height.into_consensus_u32() }
}

impl From<LockHeight> for LockTime {
    fn from(lock: LockHeight) -> Self { LockTime::from_consensus_u32(lock.into_consensus_u32()) }
}

impl TryFrom<u32> for LockHeight {
    type Error = InvalidTimelock;

    fn try_from(value: u32) -> Result<Self, Self::Error> { Self::try_from_consensus_u32(value) }
}

impl TryFrom<LockTime> for LockHeight {
    type Error = InvalidTimelock;

    fn try_from(lock_time: LockTime) -> Result<Self, Self::Error> {
        Self::try_from_lock_time(lock_time)
    }
}

impl LockHeight {
    /// Create zero time lock
    #[inline]
    pub fn anytime() -> Self { Self(0) }

    /// Creates absolute time lock with the given block height.
    ///
    /// Block height must be strictly less than `0x1DCD6500`, otherwise
    /// `None` is returned.
    #[inline]
    pub fn from_height(height: u32) -> Option<Self> {
        if height < LOCKTIME_THRESHOLD {
            Some(Self(height))
        } else {
            None
        }
    }

    #[inline]
    pub const fn try_from_lock_time(lock_time: LockTime) -> Result<Self, InvalidTimelock> {
        Self::try_from_consensus_u32(lock_time.into_consensus_u32())
    }

    #[inline]
    pub const fn try_from_consensus_u32(lock_time: u32) -> Result<Self, InvalidTimelock> {
        if !LockTime::from_consensus_u32(lock_time).is_height_based() {
            return Err(InvalidTimelock(lock_time));
        }
        Ok(Self(lock_time))
    }

    /// Converts into full u32 representation of `nLockTime` value as it is
    /// serialized in bitcoin transaction.
    #[inline]
    pub const fn to_consensus_u32(&self) -> u32 { self.0 }

    /// Converts into full u32 representation of `nLockTime` value as it is
    /// serialized in bitcoin transaction.
    #[inline]
    pub const fn into_consensus_u32(self) -> u32 { self.0 }

    /// Converts into [`LockTime`] representation.
    #[inline]
    pub fn to_lock_time(&self) -> LockTime { self.into_lock_time() }

    /// Converts into [`LockTime`] representation.
    #[inline]
    pub fn into_lock_time(self) -> LockTime { self.into() }
}

impl Display for LockHeight {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("height(")?;
        Display::fmt(&self.0, f)?;
        f.write_str(")")
    }
}

impl FromStr for LockHeight {
    type Err = TimelockParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        if s == "0" || s == "none" {
            Ok(LockHeight::anytime())
        } else if s.starts_with("height(") && s.ends_with(')') {
            let no = s[7..].trim_end_matches(')').parse()?;
            LockHeight::try_from(no).map_err(|_| TimelockParseError::InvalidHeight(no))
        } else {
            Err(TimelockParseError::InvalidDescriptor(s))
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct SeqNo(u32);

impl SeqNo {
    pub const ZERO: SeqNo = SeqNo(0);

    #[inline]
    pub const fn from_consensus_u32(lock_time: u32) -> Self { SeqNo(lock_time) }

    #[inline]
    pub const fn to_consensus_u32(&self) -> u32 { self.0 }

    /// Creates relative time lock measured in number of blocks (implies RBF).
    #[inline]
    pub const fn from_height(blocks: u16) -> SeqNo { SeqNo(blocks as u32) }

    /// Creates relative time lock measured in number of 512-second intervals
    /// (implies RBF).
    #[inline]
    pub const fn from_intervals(intervals: u16) -> SeqNo {
        SeqNo(intervals as u32 | SEQ_NO_CSV_TYPE_MASK)
    }

    /// Gets structured relative time lock information from the `nSeq` value.
    /// See [`TimeLockInterval`].
    pub const fn time_lock_interval(self) -> Option<TimeLockInterval> {
        if self.0 & SEQ_NO_CSV_DISABLE_MASK != 0 {
            None
        } else if self.0 & SEQ_NO_CSV_TYPE_MASK != 0 {
            Some(TimeLockInterval::Time((self.0 & 0xFFFF) as u16))
        } else {
            Some(TimeLockInterval::Height((self.0 & 0xFFFF) as u16))
        }
    }

    pub const fn is_timelock(self) -> bool { self.0 & SEQ_NO_CSV_DISABLE_MASK > 1 }
}

/// Time lock interval describing both relative (OP_CHECKSEQUENCEVERIFY) and
/// absolute (OP_CHECKTIMELOCKVERIFY) timelocks.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI, tags = order)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum TimeLockInterval {
    /// Describes number of blocks for the timelock
    #[display("height({0})")]
    Height(u16),

    /// Describes number of 512-second intervals for the timelock
    #[display("time({0})")]
    Time(u16),
}

impl Default for TimeLockInterval {
    fn default() -> Self { TimeLockInterval::Height(default!()) }
}
