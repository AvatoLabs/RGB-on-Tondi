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

#[macro_use]
extern crate amplify;
#[macro_use]
extern crate indexmap;
#[cfg(feature = "strict_encoding")]
#[macro_use]
extern crate strict_encoding;
#[cfg(feature = "serde")]
#[macro_use]
extern crate serde;

mod data;
mod keys;
mod maps;
mod coders;
#[cfg(feature = "client-side-validation")]
mod csval;
pub mod constructor;
mod sign;
mod tondi;

pub use coders::{Decode, DecodeError, Encode, PsttError};
pub use constructor::{
    Beneficiary, BeneficiaryParseError, ChangeInfo, ConstructionError, Payment, PsttConstructor,
    PsttMeta, TxParams, Utxo,
};
#[cfg(feature = "client-side-validation")]
pub use csval::*;
pub use data::{
    Input, ModifiableFlags, Output, PayloadError, Prevout, Pstt, PsttParseError, UnfinalizedInputs,
    Unmodifiable, UnsignedTx, UnsignedTxIn,
};
pub use keys::{GlobalKey, InputKey, KeyPair, KeyType, OutputKey, PropKey};
pub use maps::{KeyAlreadyPresent, KeyData, KeyMap, Map, MapName, ValueData};
pub use sign::{Rejected, SignError, Signer};

#[cfg(feature = "strict_encoding")]
pub const LIB_NAME_PSTT: &str = "Pstt";

/// Unsupported PSTT (Partially Signed Tondi Transaction) version.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Display, Error)]
#[display("unsupported version of PSTT v{0}")]
pub struct PsttUnsupportedVer(u32);

/// PSTT (Partially Signed Tondi Transaction) version.
///
/// Version compatibility with Tondi PSTT:
/// - V0 (0): Base version, compatible with Tondi Version::Zero
/// - V1 (1): Tondi extended version with payload support (Tondi Version::One)
/// - V2 (2): BIP-174 compatible version (for legacy tools)
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
pub enum PsttVer {
    /// Base PSTT version (Tondi Version::Zero compatible)
    #[display("v0")]
    V0 = 0,
    /// Tondi extended version with payload support (Tondi Version::One)
    #[display("v1")]
    V1 = 1,
    /// BIP-174 compatible version
    #[display("v2")]
    V2 = 2,
}

impl PsttVer {
    pub const fn try_from_standard_u32(v: u32) -> Result<Self, PsttUnsupportedVer> {
        Ok(match v {
            0 => Self::V0,
            1 => Self::V1,
            2 => Self::V2,
            wrong => return Err(PsttUnsupportedVer(wrong)),
        })
    }

    pub const fn to_standard_u32(&self) -> u32 {
        *self as u32
    }

    pub const fn max() -> Self {
        // this is a special syntax construct to get compiler error each time we add a new version
        // and not to forget upgrade the result of this method
        match Self::V0 {
            PsttVer::V0 | PsttVer::V1 | PsttVer::V2 => PsttVer::V2,
        }
    }

    /// Returns true if this version supports payload field (V1+)
    pub const fn supports_payload(&self) -> bool {
        matches!(self, PsttVer::V1 | PsttVer::V2)
    }
}

impl TryFrom<usize> for PsttVer {
    type Error = PsttUnsupportedVer;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::V0),
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            _ => Err(PsttUnsupportedVer(value as u32)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pstt_try_from_numbers() {
        assert_eq!(PsttVer::try_from(0), Ok(PsttVer::V0));
        assert_eq!(PsttVer::try_from(1), Ok(PsttVer::V1));
        assert_eq!(PsttVer::try_from(2), Ok(PsttVer::V2));
        assert_eq!(PsttVer::try_from(3), Err(PsttUnsupportedVer(3)));
    }

    #[test]
    fn pstt_payload_support() {
        assert!(!PsttVer::V0.supports_payload());
        assert!(PsttVer::V1.supports_payload());
        assert!(PsttVer::V2.supports_payload());
    }
}
