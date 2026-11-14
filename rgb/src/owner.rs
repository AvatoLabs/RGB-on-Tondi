// Wallet Library for RGB smart contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 LNP/BP Laboratories,
//                         Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
// Copyright (C) 2025 RGB Consortium, Switzerland.
// Copyright (C) 2019-2025 Dr Maxim Orlovsky.
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

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use amplify::Bytes32;
use bpstd::pstt::{KeyMap, Pstt, PsttConstructor, Utxo};
use bpstd::seals::WTxoSeal;
use bpstd::{
    Derive, DeriveCompr, DeriveLegacy, DeriveSet, DeriveXOnly, DescrId, Idx, Keychain, Network,
    NormalIndex, Outpoint, Terminal, Tx, Txid, Vout, XpubDerivable,
};
use rgb::popls::bp::WalletProvider;
use rgb::{AuthToken, RgbSealDef, WitnessStatus};
use rgbdescr::RgbDescr;

use crate::resolvers::{Resolver, ResolverError};
use crate::tondi_address::encode_p2mr_for_network;
use crate::{MemUtxos, UtxoSet};

pub trait OwnerProvider {
    type Key: DeriveSet<Legacy = Self::Key, Compr = Self::Key, XOnly = Self::Key>
        + DeriveLegacy
        + DeriveCompr
        + DeriveXOnly;
    type UtxoSet: UtxoSet;

    fn descriptor(&self) -> &RgbDescr<Self::Key>;
    fn utxos(&self) -> &Self::UtxoSet;
    fn descriptor_mut(&mut self) -> &mut RgbDescr<Self::Key>;
    fn utxos_mut(&mut self) -> &mut Self::UtxoSet;
}

#[derive(Clone)]
pub struct Holder<K = XpubDerivable, U = MemUtxos>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    U: UtxoSet,
{
    descriptor: RgbDescr<K>,
    utxos: U,
}

impl<K, U> Holder<K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    U: UtxoSet,
{
    #[inline]
    pub fn with_components(descriptor: RgbDescr<K>, utxos: U) -> Self {
        Self { descriptor, utxos }
    }
    #[inline]
    pub fn into_components(self) -> (RgbDescr<K>, U) {
        (self.descriptor, self.utxos)
    }
}

impl<K, U> OwnerProvider for Holder<K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    U: UtxoSet,
{
    type Key = K;
    type UtxoSet = U;

    #[inline]
    fn descriptor(&self) -> &RgbDescr<K> {
        &self.descriptor
    }
    #[inline]
    fn utxos(&self) -> &U {
        &self.utxos
    }
    #[inline]
    fn descriptor_mut(&mut self) -> &mut RgbDescr<K> {
        &mut self.descriptor
    }
    #[inline]
    fn utxos_mut(&mut self) -> &mut U {
        &mut self.utxos
    }
}

#[derive(Clone, Default)]
pub struct MultiHolder<K = XpubDerivable, U = MemUtxos>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    U: UtxoSet,
{
    current: Option<DescrId>,
    holders: HashMap<DescrId, Holder<K, U>>,
}

impl<K, U> MultiHolder<K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    U: UtxoSet,
{
    pub fn new() -> Self {
        Self { current: None, holders: none!() }
    }

    pub fn with(id: DescrId, holder: Holder<K, U>) -> Self {
        Self { current: Some(id), holders: map! { id => holder } }
    }

    pub fn wallet_ids(&self) -> impl Iterator<Item = DescrId> + use<'_, K, U> {
        self.holders.keys().copied()
    }

    pub fn upsert(&mut self, id: DescrId, holder: Holder<K, U>) {
        self.holders.insert(id, holder);
        if self.current.is_none() {
            self.current = Some(id);
        }
    }

    pub fn remove(&mut self, id: DescrId) -> Option<Holder<K, U>> {
        if self.current == Some(id) {
            self.current = None;
        }
        self.holders.remove(&id)
    }

    pub fn switch(&mut self, new: DescrId) {
        self.current = Some(new);
        debug_assert!(self.holders.contains_key(&new));
    }

    pub fn current(&self) -> &Holder<K, U> {
        &self.holders[&self
            .current
            .expect("current holder must be selected first with `MultiHolder::switch`")]
    }

    pub fn current_mut(&mut self) -> &mut Holder<K, U> {
        let current = self
            .current
            .expect("current holder must be selected first with `MultiHolder::switch`");
        self.holders
            .get_mut(&current)
            .expect("internal multiholder inconsistency")
    }
}

impl<K, U> OwnerProvider for MultiHolder<K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    U: UtxoSet,
{
    type Key = K;
    type UtxoSet = U;

    #[inline]
    fn descriptor(&self) -> &RgbDescr<K> {
        self.current().descriptor()
    }
    #[inline]
    fn utxos(&self) -> &U {
        self.current().utxos()
    }
    #[inline]
    fn descriptor_mut(&mut self) -> &mut RgbDescr<K> {
        self.current_mut().descriptor_mut()
    }
    #[inline]
    fn utxos_mut(&mut self) -> &mut U {
        self.current_mut().utxos_mut()
    }
}

/// Owner structure represents a holder of an RGB wallet, which keeps information of the wallet
/// descriptor and UTXO set. It doesn't know anything about RGB contracts, though (and that's why
/// it is not a full wallet) and is used as a component implementing [`WalletProvider`] inside
/// [`rgbstd::RgbWallet`] and [`crate::RgbRuntime`].
#[derive(Clone)]
pub struct Owner<R, O, K = XpubDerivable, U = MemUtxos>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    O: OwnerProvider<Key = K, UtxoSet = U>,
    R: Resolver,
    U: UtxoSet,
{
    network: Network,
    provider: O,
    resolver: R,
    _phantom: PhantomData<(K, U)>,
}

impl<R, O, K, U> Deref for Owner<R, O, K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    O: OwnerProvider<Key = K, UtxoSet = U>,
    R: Resolver,
    U: UtxoSet,
{
    type Target = O;

    fn deref(&self) -> &Self::Target {
        &self.provider
    }
}

impl<R, O, K, U> DerefMut for Owner<R, O, K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    O: OwnerProvider<Key = K, UtxoSet = U>,
    R: Resolver,
    U: UtxoSet,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.provider
    }
}

impl<R, O, K, U> Owner<R, O, K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    O: OwnerProvider<Key = K, UtxoSet = U>,
    R: Resolver,
    U: UtxoSet,
{
    pub fn with_components(network: Network, provider: O, resolver: R) -> Self {
        Self { network, provider, resolver, _phantom: PhantomData }
    }

    pub fn into_components(self) -> (O, R) {
        (self.provider, self.resolver)
    }

    #[inline]
    pub fn network(&self) -> Network {
        self.network
    }

    pub fn next_tondi_p2mr_address(&mut self) -> Result<String, String> {
        let next = self.next_derivation_index(Keychain::OUTER, true);
        let spk = self
            .provider
            .descriptor()
            .derive(Keychain::OUTER, next)
            .next()
            .ok_or_else(|| "P2MR descriptor produced no derivation".to_string())?
            .to_script_pubkey();
        encode_p2mr_for_network(self.network, spk.as_slice())
    }
}

impl<R, O, K, U> WalletProvider for Owner<R, O, K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    O: OwnerProvider<Key = K, UtxoSet = U>,
    R: Resolver,
    U: UtxoSet,
{
    type Error = ResolverError;

    fn has_utxo(&self, outpoint: Outpoint) -> bool {
        self.provider.utxos().has(outpoint)
    }

    fn utxos(&self) -> impl Iterator<Item = Outpoint> {
        self.provider.utxos().outpoints()
    }

    #[cfg(not(feature = "async"))]
    fn update_utxos(&mut self) -> Result<(), Self::Error> {
        let mut new = set![];
        let mut not_found = self.provider.utxos().outpoints().collect::<HashSet<_>>();

        for keychain in self.provider.descriptor().keychains() {
            let mut index = NormalIndex::ZERO;
            let last_index = self.provider.utxos().next_index_noshift(keychain);
            loop {
                let Some(to) = index.checked_add(20u16) else {
                    break;
                };

                let mut range = Vec::with_capacity(20);
                while index < to {
                    let terminal = Terminal::new(keychain, index);
                    let iter = self.provider.descriptor().derive(keychain, index);

                    range.extend(iter.map(|d| (terminal, d.to_script_pubkey())));

                    if index.checked_inc_assign().is_none() {
                        break;
                    }
                }

                let set = self.resolver.resolve_utxos(range);
                let mut found_in_range = false;
                for utxo in set {
                    let utxo = utxo?;
                    found_in_range = true;
                    not_found.remove(&utxo.outpoint);
                    new.insert(utxo);
                }
                if should_stop_utxo_scan(found_in_range, index, last_index) {
                    break;
                }

                index = to;
            }
        }
        let known_unmatched = known_unmatched_utxos(self.provider.utxos(), &not_found);
        let mut pending_spent = HashSet::new();
        for (outpoint, current_value, terminal) in known_unmatched {
            match self.resolver.check_utxo(outpoint)? {
                Some(live) => {
                    if live.value() != current_value {
                        return Err(ResolverError::Protocol);
                    }
                    new.insert(Utxo { outpoint, entry: live.entry, terminal });
                    not_found.remove(&outpoint);
                }
                None => {
                    if self.resolver.resolve_tx_status(outpoint.txid)? == WitnessStatus::Tentative {
                        if let Some(tx) = self.resolver.resolve_tx(outpoint.txid)? {
                            let tx = Tx::from(tx);
                            pending_spent.extend(
                                tx.inputs
                                    .iter()
                                    .filter(|i| i.consumes())
                                    .map(|i| Outpoint::from(i.outpoint())),
                            );
                        }
                        not_found.remove(&outpoint);
                    }
                }
            }
        }

        new.retain(|utxo| !pending_spent.contains(&utxo.outpoint));
        self.provider.utxos_mut().remove_all(not_found);
        self.provider.utxos_mut().insert_all(new);
        Ok(())
    }

    #[cfg(feature = "async")]
    async fn update_utxos_async(&mut self) -> Result<(), Self::Error> {
        let mut new = set![];
        let mut not_found = self.provider.utxos().outpoints().collect::<HashSet<_>>();
        for keychain in self.provider.descriptor().keychains() {
            let mut index = NormalIndex::ZERO;
            let last_index = self.provider.utxos().next_index_noshift(keychain);
            loop {
                let Some(to) = index.checked_add(20u16) else {
                    break;
                };

                let mut range = Vec::with_capacity(20);
                while index < to {
                    let terminal = Terminal::new(keychain, index);
                    let iter = self.provider.descriptor().derive(keychain, index);

                    range.extend(iter.map(|d| (terminal, d.to_script_pubkey())));

                    if index.checked_inc_assign().is_none() {
                        break;
                    }
                }

                let set = self.resolver.resolve_utxos_async(range).await;
                let mut found_in_range = false;
                for utxo in set {
                    let utxo = utxo?;
                    found_in_range = true;
                    not_found.remove(&utxo.outpoint);
                    new.insert(utxo);
                }
                if should_stop_utxo_scan(found_in_range, index, last_index) {
                    break;
                }

                index = to;
            }
        }
        let known_unmatched = known_unmatched_utxos(self.provider.utxos(), &not_found);
        let mut pending_spent = HashSet::new();
        for (outpoint, current_value, terminal) in known_unmatched {
            match self.resolver.check_utxo_async(outpoint).await? {
                Some(live) => {
                    if live.value() != current_value {
                        return Err(ResolverError::Protocol);
                    }
                    new.insert(Utxo { outpoint, entry: live.entry, terminal });
                    not_found.remove(&outpoint);
                }
                None => {
                    if self.resolver.resolve_tx_status_async(outpoint.txid).await?
                        == WitnessStatus::Tentative
                    {
                        if let Some(tx) = self.resolver.resolve_tx_async(outpoint.txid).await? {
                            let tx = Tx::from(tx);
                            pending_spent.extend(
                                tx.inputs
                                    .iter()
                                    .filter(|i| i.consumes())
                                    .map(|i| Outpoint::from(i.outpoint())),
                            );
                        }
                        not_found.remove(&outpoint);
                    }
                }
            }
        }

        new.retain(|utxo| !pending_spent.contains(&utxo.outpoint));
        self.provider.utxos_mut().remove_all_async(not_found).await;
        self.provider.utxos_mut().insert_all_async(new).await;
        Ok(())
    }

    fn register_seal(&mut self, seal: WTxoSeal) {
        self.provider.descriptor_mut().add_seal(seal);
    }

    fn resolve_seals(
        &self,
        seals: impl Iterator<Item = AuthToken>,
    ) -> impl Iterator<Item = WTxoSeal> {
        seals.flat_map(|auth| {
            self.provider
                .descriptor()
                .seals()
                .filter(move |seal| seal.auth_token() == auth)
        })
    }

    fn noise_seed(&self) -> Bytes32 {
        self.provider.descriptor().noise()
    }

    fn next_nonce(&mut self) -> u64 {
        self.provider.descriptor_mut().next_nonce()
    }

    #[cfg(not(feature = "async"))]
    fn txid_resolver(&self) -> impl Fn(Txid) -> Result<WitnessStatus, Self::Error> {
        |txid: Txid| self.resolver.resolve_tx_status(txid)
    }

    #[cfg(feature = "async")]
    fn txid_resolver_async(&self) -> impl AsyncFn(Txid) -> Result<WitnessStatus, Self::Error> {
        |txid: Txid| self.resolver.resolve_tx_status_async(txid)
    }

    #[cfg(not(feature = "async"))]
    fn last_block_height(&self) -> Result<u64, Self::Error> {
        self.resolver.last_block_height()
    }

    #[cfg(feature = "async")]
    async fn last_block_height_async(&self) -> Result<u64, Self::Error> {
        self.resolver.last_block_height_async().await
    }

    #[cfg(not(feature = "async"))]
    fn broadcast(&mut self, tx: &Tx, change: Option<(Vout, u32, u32)>) -> Result<(), Self::Error> {
        self.resolver.broadcast(tx)?;

        for inp in tx.inputs.iter().filter(|i| i.consumes()) {
            self.provider
                .utxos_mut()
                .remove(Outpoint::from(inp.outpoint()));
        }
        if let Some((vout, keychain, index)) = change {
            let txid = tx.txid();
            let out = &tx.outputs[vout.into_usize()];
            let terminal = Terminal::new(
                Keychain::with(keychain as u8),
                NormalIndex::try_from_index(index).expect("invalid derivation index"),
            );
            self.provider.utxos_mut().insert(
                Outpoint::new(txid, vout),
                bpstd::native::ResolvedInput {
                    note: out.clone(),
                    birth_daa_score: 0,
                    is_coinbase: false,
                },
                terminal,
            );
        }

        Ok(())
    }

    #[cfg(feature = "async")]
    async fn broadcast_async(
        &mut self,
        tx: &Tx,
        change: Option<(Vout, u32, u32)>,
    ) -> Result<(), Self::Error> {
        self.resolver.broadcast_async(tx).await?;

        for inp in tx.inputs.iter().filter(|i| i.consumes()) {
            self.provider
                .utxos_mut()
                .remove_async(Outpoint::from(inp.outpoint()))
                .await;
        }
        if let Some((vout, keychain, index)) = change {
            let txid = tx.txid();
            let out = &tx.outputs[vout.into_usize()];
            let terminal = Terminal::new(
                Keychain::with(keychain as u8),
                NormalIndex::try_from_index(index).expect("invalid derivation index"),
            );
            self.provider
                .utxos_mut()
                .insert_async(
                    Outpoint::new(txid, vout),
                    bpstd::native::ResolvedInput {
                        note: out.clone(),
                        birth_daa_score: 0,
                        is_coinbase: false,
                    },
                    terminal,
                )
                .await;
        }

        Ok(())
    }
}

fn should_stop_utxo_scan(
    found_in_range: bool,
    next_index: NormalIndex,
    last_known_index: NormalIndex,
) -> bool {
    !found_in_range && next_index > last_known_index
}

fn known_unmatched_utxos<U: UtxoSet>(
    utxos: &U,
    outpoints: &HashSet<Outpoint>,
) -> Vec<(Outpoint, bpstd::Sats, Terminal)> {
    outpoints
        .iter()
        .filter_map(|outpoint| {
            let (value, terminal) = utxos.get(*outpoint)?;
            Some((*outpoint, value, terminal))
        })
        .collect()
}

impl<R, O, K, U> PsttConstructor for Owner<R, O, K, U>
where
    K: DeriveSet<Legacy = K, Compr = K, XOnly = K> + DeriveLegacy + DeriveCompr + DeriveXOnly,
    O: OwnerProvider<Key = K, UtxoSet = U>,
    R: Resolver,
    U: UtxoSet,
{
    type Key = K;
    type Descr = RgbDescr<K>;

    fn descriptor(&self) -> &Self::Descr {
        self.provider.descriptor()
    }

    fn utxo(&self, outpoint: Outpoint) -> Option<Utxo> {
        let (_, terminal) = self.provider.utxos().get(outpoint)?;
        Some(Utxo {
            outpoint,
            entry: self.provider.utxos().entry(outpoint)?,
            terminal,
        })
    }
    fn network_domain(&self) -> bpstd::native::Hash32 {
        self.network.native_domain()
    }

    fn network(&self) -> Network {
        self.network
    }

    #[cfg(not(feature = "async"))]
    fn next_derivation_index(&mut self, keychain: impl Into<Keychain>, shift: bool) -> NormalIndex {
        self.provider.utxos_mut().next_index(keychain, shift)
    }
    #[cfg(feature = "async")]
    fn next_derivation_index(&mut self, keychain: impl Into<Keychain>, shift: bool) -> NormalIndex {
        use futures::executor::block_on;
        block_on(self.provider.utxos_mut().next_index_async(keychain, shift))
    }

    fn after_construct_pstt(&mut self, pstt: &mut Pstt, meta: &bpstd::pstt::PsttMeta) {
        if let Some(change) = meta.change {
            if let Some(o) = pstt.output_mut(change.vout.into_usize()) {
                let _ = o.push_proprietary(crate::state_anchor_host_key(), vec![1]);
            }
        }
    }
}

#[cfg(feature = "fs")]
pub mod file {
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    use super::*;

    pub struct FileHolder {
        inner: Holder,
        path: PathBuf,
    }

    impl FileHolder {
        const DESCRIPTOR_FILENAME: &'static str = "descriptor.toml";
        const UTXO_FILENAME: &'static str = "utxo.toml";

        pub fn create(path: PathBuf, descriptor: RgbDescr) -> io::Result<Self> {
            fs::create_dir_all(&path)?;

            let mut file = fs::File::create_new(path.join(Self::DESCRIPTOR_FILENAME))?;
            let ser = toml::to_string(&descriptor)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            file.write_all(ser.as_bytes())?;

            let mut file = fs::File::create_new(path.join(Self::UTXO_FILENAME))?;
            let utxos = MemUtxos::default();
            let ser = toml::to_string(&utxos)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            file.write_all(ser.as_bytes())?;

            let inner = Holder::with_components(descriptor, utxos);
            Ok(Self { inner, path })
        }

        pub fn load(path: PathBuf) -> io::Result<Self> {
            let mut file = fs::File::open(path.join(Self::DESCRIPTOR_FILENAME))?;
            let mut deser = String::new();
            file.read_to_string(&mut deser)?;
            let descriptor: RgbDescr = toml::from_str(&deser)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

            let mut file = fs::File::open(path.join(Self::UTXO_FILENAME))?;
            let mut deser = String::new();
            file.read_to_string(&mut deser)?;
            let utxos: MemUtxos = toml::from_str(&deser)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

            let inner = Holder::with_components(descriptor, utxos);
            Ok(Self { inner, path })
        }

        pub fn save(&self) -> io::Result<()> {
            fs::create_dir_all(&self.path)?;

            let mut file = fs::File::create(self.path.join(Self::DESCRIPTOR_FILENAME))?;
            let ser = toml::to_string(&self.inner.descriptor())
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            file.write_all(ser.as_bytes())?;

            let mut file = fs::File::create(self.path.join(Self::UTXO_FILENAME))?;
            let ser = toml::to_string(&self.inner.utxos())
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            file.write_all(ser.as_bytes())?;

            Ok(())
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for FileHolder {
        fn drop(&mut self) {
            if let Err(err) = self.save() {
                eprintln!("Error: unable to save wallet data. Details: {err}");
            }
        }
    }

    impl OwnerProvider for FileHolder {
        type Key = XpubDerivable;
        type UtxoSet = MemUtxos;
        #[inline]
        fn descriptor(&self) -> &RgbDescr<Self::Key> {
            self.inner.descriptor()
        }
        #[inline]
        fn utxos(&self) -> &Self::UtxoSet {
            self.inner.utxos()
        }
        #[inline]
        fn descriptor_mut(&mut self) -> &mut RgbDescr<Self::Key> {
            self.inner.descriptor_mut()
        }
        #[inline]
        fn utxos_mut(&mut self) -> &mut Self::UtxoSet {
            self.inner.utxos_mut()
        }
    }
}

#[cfg(all(test, not(feature = "async")))]
mod test {
    use bpstd::Sats;

    use super::*;

    fn setup() -> MemUtxos {
        let mut utxo = MemUtxos::default();
        let next = utxo.next_index(Keychain::OUTER, true);
        utxo.insert(
            Outpoint::coinbase(),
            bpstd::native::ResolvedInput {
                note: bpstd::native::StateNote::plain(
                    Sats::from_tondi(2).0,
                    bpstd::native::public_auth().0,
                ),
                birth_daa_score: 12,
                is_coinbase: false,
            },
            Terminal::new(Keychain::OUTER, next),
        );
        utxo
    }

    #[test]
    fn mem_utxo_next_index() {
        let mut utxo = MemUtxos::default();
        let next = utxo.next_index(Keychain::OUTER, true);
        assert_eq!(next, NormalIndex::ZERO);

        let next = utxo.next_index(Keychain::OUTER, false);
        assert_eq!(next, NormalIndex::ONE);

        let next = utxo.next_index(Keychain::OUTER, false);
        assert_eq!(next, NormalIndex::ONE);
    }

    #[test]
    fn mem_utxo_insert() {
        let utxo = setup();
        assert!(utxo.has(Outpoint::coinbase()));
        assert_eq!(
            utxo.get(Outpoint::coinbase()),
            Some((Sats::from_tondi(2), Terminal::new(Keychain::OUTER, NormalIndex::ZERO)))
        );
    }

    #[test]
    fn utxo_scan_continues_when_range_has_live_utxos() {
        let next_index = NormalIndex::try_from_index(20).unwrap();
        let last_known = NormalIndex::ZERO;

        assert!(!should_stop_utxo_scan(true, next_index, last_known));
        assert!(should_stop_utxo_scan(false, next_index, last_known));
    }

    #[test]
    fn utxo_scan_continues_until_beyond_last_known_index() {
        let next_index = NormalIndex::try_from_index(20).unwrap();
        let last_known = NormalIndex::try_from_index(20).unwrap();

        assert!(!should_stop_utxo_scan(false, next_index, last_known));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn mem_utxo_serde() {
        let utxo = setup();
        let s = toml::to_string(&utxo).unwrap();
        assert!(s.contains("birthDaaScore"));

        let utxo2 = toml::from_str(&s).unwrap();
        assert_eq!(utxo, utxo2);
    }
}

#[cfg(test)]
mod p2mr_address_tests {
    use bpstd::{h, Network, XprivAccount, XpubDerivable};
    use rgbdescr::RgbDescr;

    use super::super::resolvers::NoResolver;
    use super::{Holder, MemUtxos, Owner};
    use crate::tondi_address::encode_p2mr_for_network;

    #[test]
    fn tondi_p2mr_address_encoding() {
        let address =
            encode_p2mr_for_network(Network::Testnet, &[1u8; 32]).expect("encode P2MR address");
        assert!(
            address.starts_with("tondi0:m"),
            "P2MR address should use tondi0:m prefix: {address}"
        );
        assert!(encode_p2mr_for_network(Network::Testnet, &[0u8; 32]).is_err());
        assert!(encode_p2mr_for_network(Network::Testnet, &[1u8; 31]).is_err());
    }

    #[test]
    fn owner_derives_tondi_p2mr_funding_address() {
        let seed = [9u8; 64];
        let xpriv = XprivAccount::with_seed(true, &seed).derive(h![86, 1, 0]);
        let descriptor =
            RgbDescr::p2mr_unfunded(XpubDerivable::from(xpriv.to_xpub_account()), [0u8; 32]);
        let holder = Holder::with_components(descriptor, MemUtxos::default());
        let mut owner = Owner::with_components(Network::Testnet, holder, NoResolver);

        let address = owner
            .next_tondi_p2mr_address()
            .expect("derive P2MR funding address");
        assert!(address.starts_with("tondi0:m"), "funding address should be Tondi P2MR: {address}");
    }
}

#[cfg(all(test, feature = "async"))]
mod async_utxo_tests {
    use bpstd::pstt::Utxo;
    use bpstd::{
        h, Network, Outpoint, Sats, ScriptPubkey, Terminal, Tx, Txid, UnsignedTx, Vout,
        XprivAccount, XpubDerivable,
    };
    use futures::executor::block_on;
    use rgb::popls::bp::WalletProvider;
    use rgb::WitnessStatus;
    use rgbdescr::RgbDescr;

    use super::{Holder, MemUtxos, Owner};
    use crate::resolvers::{Resolver, ResolverError};
    use crate::UtxoSet;

    fn entry(value: Sats) -> bpstd::native::ResolvedInput {
        bpstd::native::ResolvedInput {
            note: bpstd::native::StateNote::plain(value.0, bpstd::native::public_auth().0),
            birth_daa_score: 12,
            is_coinbase: false,
        }
    }
    struct LiveKnownUtxoResolver {
        outpoint: Outpoint,
        value: Sats,
    }

    impl Resolver for LiveKnownUtxoResolver {
        async fn resolve_tx_async(&self, _txid: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
            Ok(None)
        }

        async fn resolve_tx_status_async(
            &self,
            _txid: Txid,
        ) -> Result<WitnessStatus, ResolverError> {
            Ok(WitnessStatus::Archived)
        }

        async fn resolve_utxos_async(
            &self,
            _iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
        ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
            Vec::new().into_iter()
        }

        async fn check_utxo_async(
            &self,
            outpoint: Outpoint,
        ) -> Result<Option<Utxo>, ResolverError> {
            if outpoint == self.outpoint {
                Ok(Some(Utxo {
                    outpoint,
                    entry: entry(self.value),
                    terminal: Terminal::new(0u8, 0u16.into()),
                }))
            } else {
                Ok(None)
            }
        }

        async fn last_block_height_async(&self) -> Result<u64, ResolverError> {
            Ok(0)
        }

        async fn broadcast_async(&self, _tx: &Tx) -> Result<(), ResolverError> {
            Ok(())
        }
    }

    #[test]
    fn async_sync_preserves_live_known_p2mr_change_outpoint() {
        block_on(async {
            let seed = [11u8; 64];
            let xpriv = XprivAccount::with_seed(true, &seed).derive(h![86, 1, 0]);
            let descriptor =
                RgbDescr::p2mr_unfunded(XpubDerivable::from(xpriv.to_xpub_account()), [0u8; 32]);
            let outpoint = Outpoint::coinbase();
            let value = Sats::from_tondi(2);
            let terminal = Terminal::new(1u8, 0u16.into());
            let mut utxos = MemUtxos::default();
            utxos.insert_async(outpoint, entry(value), terminal).await;

            let holder = Holder::with_components(descriptor, utxos);
            let resolver = LiveKnownUtxoResolver { outpoint, value };
            let mut owner = Owner::with_components(Network::Testnet, holder, resolver);

            owner.update_utxos_async().await.unwrap();

            assert!(owner.has_utxo(outpoint));
        });
    }

    struct TentativeChangeResolver {
        chain_visible_spent: Utxo,
        pending_tx: Tx,
    }

    impl Resolver for TentativeChangeResolver {
        async fn resolve_tx_async(&self, txid: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
            if txid == self.pending_tx.txid() {
                Ok(Some(self.pending_tx.clone()))
            } else {
                Ok(None)
            }
        }

        async fn resolve_tx_status_async(
            &self,
            txid: Txid,
        ) -> Result<WitnessStatus, ResolverError> {
            if txid == self.pending_tx.txid() {
                Ok(WitnessStatus::Tentative)
            } else {
                Ok(WitnessStatus::Archived)
            }
        }

        async fn resolve_utxos_async(
            &self,
            iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
        ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
            if iter
                .into_iter()
                .any(|(terminal, _)| terminal == self.chain_visible_spent.terminal)
            {
                vec![Ok(self.chain_visible_spent.clone())].into_iter()
            } else {
                Vec::new().into_iter()
            }
        }

        async fn check_utxo_async(
            &self,
            outpoint: Outpoint,
        ) -> Result<Option<Utxo>, ResolverError> {
            if outpoint == self.chain_visible_spent.outpoint {
                Ok(Some(self.chain_visible_spent.clone()))
            } else {
                Ok(None)
            }
        }

        async fn last_block_height_async(&self) -> Result<u64, ResolverError> {
            Ok(0)
        }

        async fn broadcast_async(&self, _tx: &Tx) -> Result<(), ResolverError> {
            Ok(())
        }
    }

    #[test]
    fn async_sync_preserves_tentative_change_and_ignores_spent_input() {
        block_on(async {
            let seed = [12u8; 64];
            let xpriv = XprivAccount::with_seed(true, &seed).derive(h![86, 1, 0]);
            let descriptor =
                RgbDescr::p2mr_unfunded(XpubDerivable::from(xpriv.to_xpub_account()), [0u8; 32]);
            let spent_outpoint = Outpoint::new(Txid::from([7u8; 32]), Vout::from_u32(0));
            let spent_value = Sats::from_tondi(2);
            let spent_terminal = Terminal::new(0u8, 0u16.into());
            let chain_visible_spent = Utxo {
                outpoint: spent_outpoint,
                entry: entry(spent_value),
                terminal: spent_terminal,
            };

            let prepared = bpstd::native::builder::NativeDraft {
                network: Network::Testnet.native_domain(),
                inputs: vec![bpstd::native::builder::DraftInput {
                    outpoint: spent_outpoint.into(),
                    entry: entry(spent_value),
                    auth: None,
                    since: 0,
                    consume: true,
                }],
                outputs: vec![bpstd::native::StateNote::plain(
                    Sats::from_tondi(1).0,
                    bpstd::native::public_auth().0,
                )],
                programs: vec![],
                payload: vec![],
                cycles_limit: 100_000_000,
                invocation_cycles_limit: 1_000_000,
            }
            .prepare()
            .unwrap();
            let pending_tx = Tx::from_native(prepared.transaction).unwrap();
            let pending_outpoint = Outpoint::new(pending_tx.txid(), Vout::from_u32(0));
            let pending_value = Sats::from_tondi(1);
            let pending_terminal = Terminal::new(1u8, 0u16.into());
            let mut utxos = MemUtxos::default();
            utxos
                .insert_async(pending_outpoint, entry(pending_value), pending_terminal)
                .await;

            let holder = Holder::with_components(descriptor, utxos);
            let resolver = TentativeChangeResolver { chain_visible_spent, pending_tx };
            let mut owner = Owner::with_components(Network::Testnet, holder, resolver);

            owner.update_utxos_async().await.unwrap();

            assert!(owner.has_utxo(pending_outpoint));
            assert!(!owner.has_utxo(spent_outpoint));
        });
    }
}
