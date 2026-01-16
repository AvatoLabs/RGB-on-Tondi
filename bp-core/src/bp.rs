// Bitcoin protocol core library.
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

use strict_encoding::{StrictDecode, StrictDumb, StrictEncode};

/// Enumeration over types related to bitcoin protocol-compatible chains.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = dbc::LIB_NAME_BPCORE, tags = custom, dumb = Self::Bitcoin(strict_dumb!()))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum Bp<T>
where T: StrictDumb + StrictEncode + StrictDecode
{
    /// Bitcoin blockchain-based.
    ///
    /// NB: The type does not distinguish mainnet from testnets.
    #[strict_type(tag = 0x00)]
    Bitcoin(T),

    /// Liquid blockchain-based
    ///
    /// NB: The type does not distinguish mainnet from testnets.
    #[strict_type(tag = 0x01)]
    Liquid(T),

    /// Tondi blockchain-based
    ///
    /// Tondi is a DAG-based blockchain using BLAKE3 for hashing instead of double SHA256.
    /// NB: The type does not distinguish mainnet from testnets.
    #[strict_type(tag = 0x02)]
    Tondi(T),
}

impl<T: StrictDumb + StrictEncode + StrictDecode> Bp<T> {
    /// Detects if the variant matches bitcoin blockchain.
    pub fn is_bitcoin(&self) -> bool { matches!(self, Bp::Bitcoin(_)) }
    /// Detects if the variant matches liquid blockchain.
    pub fn is_liquid(&self) -> bool { matches!(self, Bp::Liquid(_)) }
    /// Detects if the variant matches Tondi blockchain.
    pub fn is_tondi(&self) -> bool { matches!(self, Bp::Tondi(_)) }
    /// Returns bitcoin blockchain variant as an optional.
    pub fn as_bitcoin(&self) -> Option<&T> {
        match self {
            Bp::Bitcoin(t) => Some(t),
            Bp::Liquid(_) | Bp::Tondi(_) => None,
        }
    }
    /// Returns liquid blockchain variant as an optional.
    pub fn as_liquid(&self) -> Option<&T> {
        match self {
            Bp::Bitcoin(_) | Bp::Tondi(_) => None,
            Bp::Liquid(t) => Some(t),
        }
    }
    /// Returns Tondi blockchain variant as an optional.
    pub fn as_tondi(&self) -> Option<&T> {
        match self {
            Bp::Bitcoin(_) | Bp::Liquid(_) => None,
            Bp::Tondi(t) => Some(t),
        }
    }
    /// Converts into bitcoin blockchain optional.
    pub fn into_bitcoin(self) -> Option<T> {
        match self {
            Bp::Bitcoin(t) => Some(t),
            Bp::Liquid(_) | Bp::Tondi(_) => None,
        }
    }
    /// Converts into liquid blockchain optional.
    pub fn into_liquid(self) -> Option<T> {
        match self {
            Bp::Bitcoin(_) | Bp::Tondi(_) => None,
            Bp::Liquid(t) => Some(t),
        }
    }
    /// Converts into Tondi blockchain optional.
    pub fn into_tondi(self) -> Option<T> {
        match self {
            Bp::Bitcoin(_) | Bp::Liquid(_) => None,
            Bp::Tondi(t) => Some(t),
        }
    }

    /// Maps the value from one internal type into another.
    pub fn map<U: StrictDumb + StrictEncode + StrictDecode>(self, f: impl FnOnce(T) -> U) -> Bp<U> {
        match self {
            Bp::Bitcoin(t) => Bp::Bitcoin(f(t)),
            Bp::Liquid(t) => Bp::Liquid(f(t)),
            Bp::Tondi(t) => Bp::Tondi(f(t)),
        }
    }

    /// Maps the value from one internal type into another, covering cases which
    /// may error.
    pub fn try_map<U: StrictDumb + StrictEncode + StrictDecode, E>(
        self,
        f: impl FnOnce(T) -> Result<U, E>,
    ) -> Result<Bp<U>, E> {
        match self {
            Bp::Bitcoin(t) => f(t).map(Bp::Bitcoin),
            Bp::Liquid(t) => f(t).map(Bp::Liquid),
            Bp::Tondi(t) => f(t).map(Bp::Tondi),
        }
    }

    /// Maps the value from one internal type into another, covering cases which
    /// may result in an optional value.
    pub fn maybe_map<U: StrictDumb + StrictEncode + StrictDecode>(
        self,
        f: impl FnOnce(T) -> Option<U>,
    ) -> Option<Bp<U>> {
        match self {
            Bp::Bitcoin(t) => f(t).map(Bp::Bitcoin),
            Bp::Liquid(t) => f(t).map(Bp::Liquid),
            Bp::Tondi(t) => f(t).map(Bp::Tondi),
        }
    }
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use super::*;
    use strict_encoding::{StrictEncode, StrictDecode, StrictType};

    #[derive(Copy, Clone, PartialEq, Eq, Debug, Default, StrictType, StrictEncode, StrictDecode)]
    #[strict_type(lib = dbc::LIB_NAME_BPCORE)]
    struct TestType(u32);

    #[test]
    fn test_bp_tondi_variant() {
        let value = TestType(42);
        let bp_tondi = Bp::Tondi(value);

        // Test is_tondi
        assert!(bp_tondi.is_tondi());
        assert!(!bp_tondi.is_bitcoin());
        assert!(!bp_tondi.is_liquid());

        // Test as_tondi
        assert_eq!(bp_tondi.as_tondi(), Some(&value));
        assert_eq!(bp_tondi.as_bitcoin(), None);
        assert_eq!(bp_tondi.as_liquid(), None);

        // Test into_tondi
        let extracted = bp_tondi.into_tondi();
        assert_eq!(extracted, Some(value));
    }

    #[test]
    fn test_bp_tondi_map() {
        let bp_tondi = Bp::Tondi(TestType(10));

        // Test map
        let mapped = bp_tondi.map(|t| TestType(t.0 * 2));
        assert!(mapped.is_tondi());
        assert_eq!(mapped.as_tondi(), Some(&TestType(20)));

        // Test try_map
        let try_mapped = bp_tondi.try_map(|t| Ok::<TestType, ()>(TestType(t.0 + 5)));
        assert!(try_mapped.is_ok());
        let result = try_mapped.unwrap();
        assert!(result.is_tondi());
        assert_eq!(result.as_tondi(), Some(&TestType(15)));

        // Test maybe_map
        let maybe_mapped = bp_tondi.maybe_map(|t| Some(TestType(t.0 - 3)));
        assert!(maybe_mapped.is_some());
        let result = maybe_mapped.unwrap();
        assert!(result.is_tondi());
        assert_eq!(result.as_tondi(), Some(&TestType(7)));
    }

    #[test]
    fn test_bp_tondi_serialization() {
        use strict_encoding::{StrictEncode, StrictDecode, StreamReader, StreamWriter};
        
        let bp_tondi = Bp::Tondi(TestType(123));
        
        // Serialize
        let mut encoded = Vec::new();
        {
            let mut writer = StreamWriter::new::<1024>(&mut encoded);
            bp_tondi.strict_write(&mut writer).unwrap();
        }
        
        // Deserialize
        let mut reader = StreamReader::new::<1024>(encoded.as_slice());
        let decoded: Bp<TestType> = Bp::strict_read(&mut reader).unwrap();
        
        assert_eq!(decoded, bp_tondi);
        assert!(decoded.is_tondi());
    }

    #[test]
    fn test_bp_all_variants() {
        let bitcoin = Bp::Bitcoin(TestType(1));
        let liquid = Bp::Liquid(TestType(2));
        let tondi = Bp::Tondi(TestType(3));

        // Verify each variant identifies correctly
        assert!(bitcoin.is_bitcoin() && !bitcoin.is_liquid() && !bitcoin.is_tondi());
        assert!(!liquid.is_bitcoin() && liquid.is_liquid() && !liquid.is_tondi());
        assert!(!tondi.is_bitcoin() && !tondi.is_liquid() && tondi.is_tondi());

        // Verify extraction works
        assert_eq!(bitcoin.into_bitcoin(), Some(TestType(1)));
        assert_eq!(liquid.into_liquid(), Some(TestType(2)));
        assert_eq!(tondi.into_tondi(), Some(TestType(3)));
    }
}
