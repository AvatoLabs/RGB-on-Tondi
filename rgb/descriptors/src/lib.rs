// Wallet Library for RGB smart contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Adapted for Tondi by AvatoLabs.
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 LNP/BP Laboratories,
//                         Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
// Copyright (C) 2025 RGB Consortium, Switzerland.
// Copyright (C) 2019-2025 Dr Maxim Orlovsky.
// Copyright (C) 2025 AvatoLabs.
// All rights under the above copyrights are reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
// #![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
#[macro_use]
extern crate amplify;
#[cfg(feature = "serde")]
#[macro_use]
extern crate serde;
extern crate core;

use alloc::collections::BTreeSet;
use core::fmt::{self, Display, Formatter};
use core::iter;

use amplify::{Bytes32, Wrapper, WrapperMut};
use bp::seals::WTxoSeal;
use bpstd::{
    tondi_p2mr_single_key_root, CompressedPk, ControlBlock, Derive, DeriveCompr, DeriveKey,
    DeriveLegacy, DeriveSet, DeriveXOnly, DerivedScript, Descriptor, KeyOrigin, Keychain,
    LegacyKeySig, LegacyPk, NormalIndex, RedeemScript, ScriptPubkey, SigScript, SpkClass, StdDescr,
    TapDerivation, TaprootKeySig, Terminal, Witness, WitnessScript, XOnlyPk, XpubAccount,
    XpubDerivable,
};
use indexmap::IndexMap;

pub trait DescriptorRgb<K = XpubDerivable, V = ()>: Descriptor<K, V>
where K: DeriveSet<XOnly = K> + DeriveXOnly
{
    fn add_seal(&self, seal: WTxoSeal);
}

#[derive(Wrapper, WrapperMut, Clone, Eq, PartialEq, Debug, Default, From)]
#[wrapper(Deref)]
#[wrapper_mut(DerefMut)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct SealDescr(BTreeSet<WTxoSeal>);

impl Display for SealDescr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("seals(")?;
        let mut iter = self.0.iter().peekable();
        while let Some(seal) = iter.next() {
            Display::fmt(seal, f)?;
            if iter.peek().is_some() {
                f.write_str(",")?;
            }
        }
        f.write_str(")")
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(
        rename_all = "camelCase",
        bound(serialize = "K: serde::Serialize", deserialize = "K: serde::Deserialize<'de>")
    )
)]
enum RgbDeriver<K: DeriveSet<XOnly = K> + DeriveXOnly = XpubDerivable> {
    TondiP2mr { key: K },
}

impl<K> RgbDeriver<K>
where K: DeriveSet<XOnly = K> + DeriveXOnly
{
    fn p2mr(key: K) -> Self { Self::TondiP2mr { key } }

    fn key(&self) -> &K {
        match self {
            RgbDeriver::TondiP2mr { key } => key,
        }
    }
}

impl<K> Display for RgbDeriver<K>
where K: DeriveSet<XOnly = K> + DeriveXOnly
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RgbDeriver::TondiP2mr { key } => write!(f, "p2mr({key})"),
        }
    }
}

/// Computes the Tondi standard single-key P2MR root.
///
/// This mirrors Tondi's `p2mr_single_key_root`: the committed leaf is
/// `<x-only-pubkey> OP_CHECKSIG`, and the P2MR output payload is the one-leaf
/// TapLeaf hash.
pub fn p2mr_single_key_root(xonly: XOnlyPk) -> [u8; 32] { tondi_p2mr_single_key_root(xonly) }

fn p2mr_script_pubkey(xonly: XOnlyPk) -> ScriptPubkey {
    ScriptPubkey::from_checked(p2mr_single_key_root(xonly).to_vec())
}

impl<K> Derive<DerivedScript> for RgbDeriver<K>
where K: DeriveSet<XOnly = K> + DeriveXOnly
{
    fn default_keychain(&self) -> Keychain { self.key().default_keychain() }

    fn keychains(&self) -> BTreeSet<Keychain> { self.key().keychains() }

    fn derive(
        &self,
        keychain: impl Into<Keychain>,
        index: impl Into<NormalIndex>,
    ) -> impl Iterator<Item = DerivedScript> {
        let keychain = keychain.into();
        let index = index.into();
        match self {
            RgbDeriver::TondiP2mr { key } => <K as Derive<XOnlyPk>>::derive(key, keychain, index)
                .map(|xonly| DerivedScript::Bare(p2mr_script_pubkey(xonly)))
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }
}

impl<K> Descriptor<K> for RgbDeriver<K>
where K: DeriveSet<XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly
{
    fn class(&self) -> SpkClass { SpkClass::Bare }

    fn keys<'a>(&'a self) -> impl Iterator<Item = &'a K>
    where K: 'a {
        iter::once(self.key())
    }

    fn vars<'a>(&'a self) -> impl Iterator<Item = &'a ()>
    where (): 'a {
        iter::empty()
    }

    fn xpubs(&self) -> impl Iterator<Item = &XpubAccount> {
        iter::once(<K as DeriveKey<XOnlyPk>>::xpub_spec(self.key()))
    }

    fn legacy_keyset(&self, _terminal: Terminal) -> IndexMap<LegacyPk, KeyOrigin> {
        IndexMap::new()
    }

    fn xonly_keyset(&self, terminal: Terminal) -> IndexMap<XOnlyPk, TapDerivation> {
        <K as Derive<XOnlyPk>>::derive(self.key(), terminal.keychain, terminal.index)
            .map(|key| {
                (
                    key,
                    TapDerivation::with_internal_pk(
                        <K as DeriveKey<XOnlyPk>>::xpub_spec(self.key())
                            .origin()
                            .clone(),
                        terminal,
                    ),
                )
            })
            .collect()
    }

    fn legacy_witness(
        &self,
        _keysigs: IndexMap<&KeyOrigin, LegacyKeySig>,
        _redeem_script: Option<RedeemScript>,
        _witness_script: Option<WitnessScript>,
    ) -> Option<(SigScript, Option<Witness>)> {
        None
    }

    fn taproot_witness(
        &self,
        _cb: Option<&ControlBlock>,
        _keysigs: IndexMap<&KeyOrigin, TaprootKeySig>,
    ) -> Option<Witness> {
        None
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(
        rename_all = "camelCase",
        bound(serialize = "K: serde::Serialize", deserialize = "K: serde::Deserialize<'de>")
    )
)]
pub struct RgbDescr<K: DeriveSet<XOnly = K> + DeriveXOnly = XpubDerivable> {
    deriver: RgbDeriver<K>,
    seals: SealDescr,
    noise: Bytes32,
    nonce: u64,
}

impl<K> Display for RgbDescr<K>
where K: DeriveSet<XOnly = K> + DeriveXOnly
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "rgb({},{},noise({:x}))", self.deriver, self.seals, self.noise)
    }
}

impl<K> RgbDescr<K>
where K: DeriveSet<XOnly = K> + DeriveXOnly
{
    pub fn p2mr_unfunded(key: K, noise: impl Into<[u8; 32]>) -> Self {
        Self {
            deriver: RgbDeriver::p2mr(key),
            seals: empty!(),
            noise: noise.into().into(),
            nonce: 0,
        }
    }

    pub fn new_unfunded(_deriver: impl Into<StdDescr<K>>, _noise: impl Into<[u8; 32]>) -> Self {
        panic!("legacy RGB descriptor paths are disabled; use RgbDescr::p2mr_unfunded")
    }

    pub fn key_only_unfunded(internal_key: K, noise: impl Into<[u8; 32]>) -> Self {
        Self::p2mr_unfunded(internal_key, noise)
    }

    pub fn next_nonce(&mut self) -> u64 {
        let nonce = self.nonce;
        self.nonce += 1;
        nonce
    }

    pub fn noise(&self) -> Bytes32 { self.noise }

    pub fn derive_p2mr_compressed_key(
        &self,
        keychain: impl Into<Keychain>,
        index: impl Into<NormalIndex>,
    ) -> Option<CompressedPk>
    where
        K: Derive<CompressedPk> + DeriveCompr,
    {
        match &self.deriver {
            RgbDeriver::TondiP2mr { key } => {
                <K as Derive<CompressedPk>>::derive(key, keychain, index).next()
            }
        }
    }

    pub fn seals(&self) -> impl Iterator<Item = WTxoSeal> + use<'_, K> {
        self.seals.0.iter().copied()
    }

    pub fn add_seal(&mut self, seal: WTxoSeal) { self.seals.0.insert(seal); }
}

impl<K> Derive<DerivedScript> for RgbDescr<K>
where K: DeriveSet<XOnly = K> + DeriveXOnly
{
    fn default_keychain(&self) -> Keychain { self.deriver.default_keychain() }

    fn keychains(&self) -> BTreeSet<Keychain> { self.deriver.keychains() }

    fn derive(
        &self,
        keychain: impl Into<Keychain>,
        index: impl Into<NormalIndex>,
    ) -> impl Iterator<Item = DerivedScript> {
        self.deriver.derive(keychain, index)
    }
}

impl<K> Descriptor<K> for RgbDescr<K>
where K: DeriveSet<XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly
{
    fn class(&self) -> SpkClass { self.deriver.class() }

    fn keys<'a>(&'a self) -> impl Iterator<Item = &'a K>
    where K: 'a {
        self.deriver.keys()
    }

    fn vars<'a>(&'a self) -> impl Iterator<Item = &'a ()>
    where (): 'a {
        self.deriver.vars()
    }

    fn xpubs(&self) -> impl Iterator<Item = &XpubAccount> { self.deriver.xpubs() }

    fn legacy_keyset(&self, terminal: Terminal) -> IndexMap<LegacyPk, KeyOrigin> {
        self.deriver.legacy_keyset(terminal)
    }

    fn xonly_keyset(&self, terminal: Terminal) -> IndexMap<XOnlyPk, TapDerivation> {
        self.deriver.xonly_keyset(terminal)
    }

    fn legacy_witness(
        &self,
        keysigs: IndexMap<&KeyOrigin, LegacyKeySig>,
        redeem_script: Option<RedeemScript>,
        witness_script: Option<WitnessScript>,
    ) -> Option<(SigScript, Option<Witness>)> {
        self.deriver
            .legacy_witness(keysigs, redeem_script, witness_script)
    }

    fn taproot_witness(
        &self,
        cb: Option<&ControlBlock>,
        keysigs: IndexMap<&KeyOrigin, TaprootKeySig>,
    ) -> Option<Witness> {
        self.deriver.taproot_witness(cb, keysigs)
    }
}

#[cfg(test)]
mod p2mr_tests {
    use bpstd::secp256k1;

    use super::*;

    #[test]
    fn p2mr_single_key_root_delegates_to_protocol_helper() {
        let secret_key = secp256k1::SecretKey::from_slice(&[11u8; 32]).unwrap();
        let public_key = secp256k1::PublicKey::from_secret_key(secp256k1::SECP256K1, &secret_key);
        let xonly = XOnlyPk::from_byte_array(public_key.x_only_public_key().0.serialize()).unwrap();

        assert_eq!(p2mr_single_key_root(xonly), tondi_p2mr_single_key_root(xonly));
    }
}

#[cfg(all(test, feature = "serde"))]
mod test {
    use bpstd::{h, Derive, Idx, Keychain, NormalIndex, XprivAccount};

    use super::*;

    #[test]
    fn descr_serde() {
        let seed = [7u8; 64];
        let xpriv = XprivAccount::with_seed(true, &seed).derive(h![86, 1, 0]);
        let xpub = XpubDerivable::from(xpriv.to_xpub_account());
        let descr = RgbDescr::<XpubDerivable>::p2mr_unfunded(xpub, [0u8; 32]);

        let toml = toml::to_string(&descr).unwrap();
        let descr2: RgbDescr<XpubDerivable> = toml::from_str(&toml).unwrap();

        assert_eq!(descr.to_string(), descr2.to_string());

        let script = descr
            .derive(Keychain::OUTER, NormalIndex::ZERO)
            .next()
            .expect("P2MR descriptor must derive")
            .to_script_pubkey();
        assert_eq!(script.as_slice().len(), 32);
        assert!(script.as_slice().iter().any(|byte| *byte != 0));
    }
}
