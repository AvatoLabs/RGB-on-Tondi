// Tondi protocol consensus library.
//
// SPDX-License-Identifier: Apache-2.0
//
// Originally written in 2019-2024 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Adapted for Tondi in 2025
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

#![allow(unused_braces)] // required due to strict dumb derivation and compiler bug

use std::borrow::Borrow;
use std::fmt::{self, Formatter, LowerHex, UpperHex};
use std::ops::BitXor;
use std::str::FromStr;
use std::{cmp, io, slice, vec};

use amplify::confinement::Confined;
use amplify::hex::FromHex;
use amplify::{confinement, ByteArray, Bytes32, Wrapper};
use commit_verify::{DigestExt, TondiBlake3};
use secp256k1::{Keypair, PublicKey, Scalar, XOnlyPublicKey};
use sha2::{Digest, Sha256};
use strict_encoding::{
    DecodeError, ReadTuple, StrictDecode, StrictEncode, StrictProduct, StrictTuple, StrictType,
    TypeName, TypedRead, TypedWrite, WriteTuple,
};

use crate::opcodes::*;
use crate::{
    CompressedPk, ConsensusEncode, InvalidPubkey, PubkeyParseError, ScriptBytes, ScriptPubkey,
    TapCode, VarInt, VarIntBytes, WitnessVer, LIB_NAME_TONDI,
};

/// Tondi Copperoot domain tag for leaf hash (BLAKE3 derive_key context).
const COPPEROOT_LEAF_TAG: &str = "tondi.copperoot.leaf";

/// Tondi Copperoot domain tag for branch hash (BLAKE3 derive_key context).
const COPPEROOT_BRANCH_TAG: &str = "tondi.copperoot.branch";

/// Tondi Copperoot domain tag for tweak hash (BLAKE3 derive_key context).
const COPPEROOT_TWEAK_TAG: &str = "tondi.copperoot.tweak";

/// Tondi Copperoot domain tag for sighash (BLAKE3 derive_key context).
pub const COPPEROOT_SIGHASH_TAG: &str = "tondi.copperoot.sighash";

impl<const LEN: usize> From<InvalidPubkey<LEN>> for DecodeError {
    fn from(e: InvalidPubkey<LEN>) -> Self {
        DecodeError::DataIntegrityError(format!("invalid x-only public key value '{e}'"))
    }
}

/// Generic taproot x-only (BIP-340) public key - a wrapper around
/// [`XOnlyPublicKey`] providing APIs compatible with the rest of the library.
/// Should be used everywhere when [`InternalPk`] and [`OutputPk`] do not apply:
/// as an output of BIP32 key derivation functions, inside tapscripts/
/// leafscripts etc.
#[derive(Wrapper, WrapperMut, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, LowerHex, Display)]
#[wrapper_mut(DerefMut)]
#[derive(StrictType, StrictDumb)]
#[strict_type(lib = LIB_NAME_TONDI, dumb = Self::dumb())]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct XOnlyPk(XOnlyPublicKey);

impl XOnlyPk {
    fn dumb() -> Self { Self(XOnlyPublicKey::from_slice(&[1u8; 32]).unwrap()) }

    pub fn from_byte_array(data: [u8; 32]) -> Result<Self, InvalidPubkey<32>> {
        XOnlyPublicKey::from_slice(data.as_ref())
            .map(Self)
            .map_err(|_| InvalidPubkey::Specified(data.into()))
    }

    pub fn to_byte_array(&self) -> [u8; 32] { self.0.serialize() }

    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, InvalidPubkey<33>> {
        Ok(XOnlyPk(XOnlyPublicKey::from_slice(bytes.as_ref())?))
    }
}

impl From<CompressedPk> for XOnlyPk {
    fn from(pubkey: CompressedPk) -> Self { XOnlyPk(pubkey.x_only_public_key().0) }
}

impl From<PublicKey> for XOnlyPk {
    fn from(pubkey: PublicKey) -> Self { XOnlyPk(pubkey.x_only_public_key().0) }
}

impl From<XOnlyPk> for [u8; 32] {
    fn from(pk: XOnlyPk) -> [u8; 32] { pk.to_byte_array() }
}

impl StrictEncode for XOnlyPk {
    fn strict_encode<W: TypedWrite>(&self, writer: W) -> io::Result<W> {
        let bytes = Bytes32::from(self.0.serialize());
        writer.write_newtype::<Self>(&bytes)
    }
}

impl StrictDecode for XOnlyPk {
    fn strict_decode(reader: &mut impl TypedRead) -> Result<Self, DecodeError> {
        reader.read_tuple(|r| {
            let bytes: Bytes32 = r.read_field()?;
            XOnlyPublicKey::from_slice(bytes.as_slice())
                .map(Self)
                .map_err(|_| InvalidPubkey::Specified(bytes).into())
        })
    }
}

impl FromStr for XOnlyPk {
    type Err = PubkeyParseError<32>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let data = <[u8; 32]>::from_hex(s)?;
        let pk = Self::from_byte_array(data)?;
        Ok(pk)
    }
}

/// Internal taproot public key, which can be present only in key fragment
/// inside taproot descriptors.
#[derive(Eq, PartialEq, From)]
pub struct InternalKeypair(#[from] Keypair);

impl InternalKeypair {
    pub fn to_output_keypair(&self, merkle_root: Option<TapNodeHash>) -> (Keypair, Parity) {
        let internal_pk = self.0.x_only_public_key().0;
        let mut engine = TondiBlake3::from_tag(COPPEROOT_TWEAK_TAG);
        // always hash the key
        engine.input_raw(&internal_pk.serialize());
        if let Some(merkle_root) = merkle_root {
            engine.input_raw(merkle_root.into_tap_hash().as_ref());
        }
        let tweak =
            Scalar::from_be_bytes(engine.finish()).expect("hash value greater than curve order");
        let pair = self.0.add_xonly_tweak(secp256k1::SECP256K1, &tweak).expect("hash collision");
        let (outpput_key, tweaked_parity) = pair.x_only_public_key();
        debug_assert!(internal_pk.tweak_add_check(
            secp256k1::SECP256K1,
            &outpput_key,
            tweaked_parity,
            tweak
        ));
        (pair, tweaked_parity.into())
    }
}

/// Internal taproot public key, which can be present only in key fragment
/// inside taproot descriptors.
#[derive(Wrapper, WrapperMut, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, LowerHex, Display, FromStr)]
#[wrapper_mut(DerefMut)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct InternalPk(
    #[from]
    #[from(XOnlyPublicKey)]
    XOnlyPk,
);

impl InternalPk {
    #[inline]
    pub fn from_unchecked(pk: XOnlyPk) -> Self { Self(pk) }

    #[inline]
    pub fn from_byte_array(data: [u8; 32]) -> Result<Self, InvalidPubkey<32>> {
        XOnlyPk::from_byte_array(data).map(Self)
    }

    #[inline]
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, InvalidPubkey<33>> {
        XOnlyPk::from_bytes(bytes).map(Self)
    }

    #[inline]
    pub fn to_byte_array(&self) -> [u8; 32] { self.0.to_byte_array() }

    #[inline]
    pub fn to_xonly_pk(&self) -> XOnlyPk { self.0 }

    pub fn to_output_pk(&self, merkle_root: Option<TapNodeHash>) -> (OutputPk, Parity) {
        let mut engine = TondiBlake3::from_tag(COPPEROOT_TWEAK_TAG);
        // always hash the key
        engine.input_raw(&self.0.serialize());
        if let Some(merkle_root) = merkle_root {
            engine.input_raw(merkle_root.into_tap_hash().as_ref());
        }
        let tweak =
            Scalar::from_be_bytes(engine.finish()).expect("hash value greater than curve order");
        let (output_key, tweaked_parity) =
            self.0.add_tweak(secp256k1::SECP256K1, &tweak).expect("hash collision");
        debug_assert!(self.tweak_add_check(
            secp256k1::SECP256K1,
            &output_key,
            tweaked_parity,
            tweak
        ));
        (OutputPk(XOnlyPk(output_key)), tweaked_parity.into())
    }
}

impl From<InternalPk> for [u8; 32] {
    fn from(pk: InternalPk) -> [u8; 32] { pk.to_byte_array() }
}

/// Output taproot key - an [`InternalPk`] tweaked with merkle root of the
/// script tree - or its own hash. Used only inside addresses and raw taproot
/// descriptors.
#[derive(Wrapper, WrapperMut, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, LowerHex, Display, FromStr)]
#[wrapper_mut(DerefMut)]
#[derive(StrictType, StrictEncode, StrictDecode, StrictDumb)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct OutputPk(XOnlyPk);

impl OutputPk {
    #[inline]
    pub fn from_unchecked(pk: XOnlyPk) -> Self { Self(pk) }

    #[inline]
    pub fn from_byte_array(data: [u8; 32]) -> Result<Self, InvalidPubkey<32>> {
        XOnlyPk::from_byte_array(data).map(Self)
    }

    #[inline]
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, InvalidPubkey<33>> {
        XOnlyPk::from_bytes(bytes).map(Self)
    }

    #[inline]
    pub fn to_xonly_pk(&self) -> XOnlyPk { self.0 }

    #[inline]
    pub fn to_script_pubkey(&self) -> ScriptPubkey { ScriptPubkey::p2tr_tweaked(*self) }

    #[inline]
    pub fn to_byte_array(&self) -> [u8; 32] { self.0.to_byte_array() }
}

impl From<OutputPk> for [u8; 32] {
    fn from(pk: OutputPk) -> [u8; 32] { pk.to_byte_array() }
}

pub trait IntoTapHash {
    fn into_tap_hash(self) -> TapNodeHash;
}

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, AsSlice, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct TapSighash(
    #[from]
    #[from([u8; 32])]
    pub Bytes32,
);

impl From<TapSighash> for [u8; 32] {
    fn from(value: TapSighash) -> Self { value.0.into_inner() }
}

impl From<TapSighash> for secp256k1::Message {
    fn from(sighash: TapSighash) -> Self {
        secp256k1::Message::from_digest(sighash.to_byte_array())
    }
}

impl TapSighash {
    pub fn engine() -> TondiBlake3 { TondiBlake3::from_tag(COPPEROOT_SIGHASH_TAG) }

    pub fn from_engine(engine: TondiBlake3) -> Self { Self(engine.finish().into()) }
}

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictEncode, StrictDecode, StrictDumb)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct TapLeafHash(
    #[from]
    #[from([u8; 32])]
    Bytes32,
);

impl TapLeafHash {
    pub fn with_leaf_script(leaf_script: &LeafScript) -> Self {
        Self::with_raw_script(leaf_script.version, leaf_script.as_script_bytes())
    }

    pub fn with_tap_script(tap_script: &TapScript) -> Self {
        Self::with_raw_script(LeafVer::TapScript, tap_script.as_script_bytes())
    }

    fn with_raw_script(version: LeafVer, script: &ScriptBytes) -> Self {
        let mut engine = TondiBlake3::from_tag(COPPEROOT_LEAF_TAG);
        engine.input_raw(&[version.to_consensus_u8()]);
        script.len_var_int().consensus_encode(&mut engine).ok();
        engine.input_raw(script.as_slice());
        Self(engine.finish().into())
    }
}

fn sha256_tagged_hash(tag: &str, payload: &[u8]) -> [u8; 32] {
    let tag_hash = Sha256::digest(tag.as_bytes());
    let mut hasher = Sha256::new();
    hasher.update(tag_hash);
    hasher.update(tag_hash);
    hasher.update(payload);
    hasher.finalize().into()
}

/// Computes the Tondi native TapLeaf hash for a TapScript leaf.
///
/// Tondi P2MR follows the node-side TapLeaf/BIP360 encoding for the public
/// P2MR envelope, not the legacy RGB Copperoot/BLAKE3 hash above.
pub fn tondi_tapleaf_hash_from_tapscript(script: &[u8]) -> TapLeafHash {
    tondi_p2mr_leaf_hash(script, LeafVer::TapScript)
}

/// Computes the Tondi native P2MR leaf hash for a script leaf.
pub fn tondi_p2mr_leaf_hash(script: &[u8], leaf_version: LeafVer) -> TapLeafHash {
    let mut payload = Vec::with_capacity(2 + script.len());
    payload.push(leaf_version.to_consensus_u8());
    VarInt(script.len() as u64).consensus_encode(&mut payload).expect("Vec writes cannot fail");
    payload.extend_from_slice(script);
    TapLeafHash(sha256_tagged_hash("TapLeaf", &payload).into())
}

/// Computes the Tondi native one-leaf P2MR root from a TapScript leaf.
pub fn tondi_p2mr_root_from_tapscript(script: &[u8]) -> [u8; 32] {
    tondi_tapleaf_hash_from_tapscript(script).to_byte_array()
}

/// Computes a Tondi native P2MR branch hash from two child hashes.
pub fn tondi_p2mr_branch_hash(left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    let (first, second) = if left <= right { (left, right) } else { (right, left) };
    let mut payload = Vec::with_capacity(64);
    payload.extend_from_slice(&first);
    payload.extend_from_slice(&second);
    sha256_tagged_hash("TapBranch", &payload)
}

/// P2MR control block base size.
pub const TONDI_P2MR_CONTROL_BASE_SIZE: usize = 1;

/// P2MR control block sibling hash size.
pub const TONDI_P2MR_CONTROL_NODE_SIZE: usize = 32;

/// Maximum P2MR script tree depth.
pub const TONDI_P2MR_CONTROL_MAX_NODE_COUNT: usize = 128;

/// Standard single-leaf P2MR control block.
pub const TONDI_P2MR_CONTROL_BLOCK_SINGLE_LEAF: [u8; 1] = [TAPROOT_LEAF_TAPSCRIPT | 1];

/// Errors produced by Tondi P2MR tree and witness helpers.
#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
#[display(doc_comments)]
pub enum TondiP2mrTreeError {
    /// P2MR tree must contain at least one leaf or hidden branch.
    EmptyTree,

    /// P2MR tree depth {depth} exceeds max depth {max}.
    DepthTooLarge {
        /// Supplied tree depth.
        depth: usize,
        /// Maximum accepted tree depth.
        max: usize,
    },

    /// P2MR tree depths are not valid DFS order.
    InvalidDepthOrder,

    /// P2MR leaf version {0:#x} has the control-block parity bit set.
    LeafVersionCarriesParity(u8),

    /// P2MR tree is incomplete.
    IncompleteTree,

    /// P2MR witness stack item count exceeds u32.
    TooManyWitnessItems,

    /// P2MR witness stack item length exceeds u32.
    WitnessItemTooLarge,
}

/// Spend data for one tracked P2MR script leaf.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct TondiP2mrScriptSpend {
    /// Script bytes revealed for this spend.
    pub leaf_script: Vec<u8>,
    /// Leaf version committed by the control block.
    pub leaf_version: LeafVer,
    /// Control block containing the parity-set leaf version and merkle branch.
    pub control_block: Vec<u8>,
}

impl TondiP2mrScriptSpend {
    /// Encodes a Tondi P2MR Borsh witness stack for this script spend.
    pub fn encode_witness(
        &self,
        input: impl IntoIterator<Item = Vec<u8>>,
        annex: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, TondiP2mrTreeError> {
        let mut stack: Vec<Vec<u8>> = input.into_iter().collect();
        stack.push(self.leaf_script.clone());
        stack.push(self.control_block.clone());
        if let Some(annex) = annex {
            stack.push(annex);
        }
        tondi_p2mr_encode_witness_stack(stack)
    }
}

/// Tracked spend data for a P2MR tree.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct TondiP2mrSpendData {
    /// P2MR merkle root committed in the output script.
    pub merkle_root: [u8; 32],
    /// Spendable leaves tracked by the local wallet.
    pub scripts: Vec<TondiP2mrScriptSpend>,
}

impl TondiP2mrSpendData {
    /// Returns the shortest known spend data for a leaf script/version pair.
    pub fn shortest_spend_for(
        &self,
        leaf_script: &[u8],
        leaf_version: LeafVer,
    ) -> Option<&TondiP2mrScriptSpend> {
        self.scripts
            .iter()
            .filter(|spend| spend.leaf_script == leaf_script && spend.leaf_version == leaf_version)
            .min_by(|a, b| {
                a.control_block
                    .len()
                    .cmp(&b.control_block.len())
                    .then_with(|| a.control_block.cmp(&b.control_block))
            })
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
struct P2mrLeafInfo {
    script: Vec<u8>,
    leaf_version: LeafVer,
    merkle_branch: Vec<[u8; 32]>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
struct P2mrNodeInfo {
    hash: [u8; 32],
    leaves: Vec<P2mrLeafInfo>,
}

impl P2mrNodeInfo {
    fn combine(mut a: Self, mut b: Self) -> Self {
        for leaf in &mut a.leaves {
            leaf.merkle_branch.push(b.hash);
        }
        for leaf in &mut b.leaves {
            leaf.merkle_branch.push(a.hash);
        }

        let hash = tondi_p2mr_branch_hash(a.hash, b.hash);
        a.leaves.extend(b.leaves);
        Self {
            hash,
            leaves: a.leaves,
        }
    }
}

/// Builder for Tondi P2MR script trees in DFS depth order.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct TondiP2mrTreeBuilder {
    branches: Vec<Option<P2mrNodeInfo>>,
}

impl TondiP2mrTreeBuilder {
    /// Constructs an empty P2MR tree builder.
    pub fn new() -> Self { Self::default() }

    /// Adds a TapScript leaf at the specified DFS depth.
    pub fn add_tapscript_leaf(
        &mut self,
        depth: u8,
        script: impl AsRef<[u8]>,
    ) -> Result<&mut Self, TondiP2mrTreeError> {
        self.add_leaf(depth, script, LeafVer::TapScript)
    }

    /// Adds a script leaf at the specified DFS depth.
    pub fn add_leaf(
        &mut self,
        depth: u8,
        script: impl AsRef<[u8]>,
        leaf_version: LeafVer,
    ) -> Result<&mut Self, TondiP2mrTreeError> {
        if leaf_version.to_consensus_u8() & 1 != 0 {
            return Err(TondiP2mrTreeError::LeafVersionCarriesParity(
                leaf_version.to_consensus_u8(),
            ));
        }

        let script = script.as_ref().to_vec();
        let node = P2mrNodeInfo {
            hash: tondi_p2mr_leaf_hash(&script, leaf_version).to_byte_array(),
            leaves: vec![P2mrLeafInfo {
                script,
                leaf_version,
                merkle_branch: vec![],
            }],
        };
        self.insert(depth as usize, node)?;
        Ok(self)
    }

    /// Adds an omitted hidden branch hash at the specified DFS depth.
    pub fn add_hidden(
        &mut self,
        depth: u8,
        hash: [u8; 32],
    ) -> Result<&mut Self, TondiP2mrTreeError> {
        let node = P2mrNodeInfo {
            hash,
            leaves: vec![],
        };
        self.insert(depth as usize, node)?;
        Ok(self)
    }

    /// Returns true when the tree has exactly one complete root.
    pub fn is_complete(&self) -> bool { self.branches.len() == 1 && self.branches[0].is_some() }

    /// Returns the P2MR merkle root.
    pub fn merkle_root(&self) -> Result<[u8; 32], TondiP2mrTreeError> {
        Ok(self.complete_root()?.hash)
    }

    /// Builds spend data for all tracked leaves.
    pub fn spend_data(&self) -> Result<TondiP2mrSpendData, TondiP2mrTreeError> {
        let root = self.complete_root()?;
        let mut scripts = Vec::with_capacity(root.leaves.len());

        for leaf in &root.leaves {
            let mut control_block = Vec::with_capacity(
                TONDI_P2MR_CONTROL_BASE_SIZE
                    + TONDI_P2MR_CONTROL_NODE_SIZE * leaf.merkle_branch.len(),
            );
            control_block.push(leaf.leaf_version.to_consensus_u8() | 1);
            for node in &leaf.merkle_branch {
                control_block.extend_from_slice(node);
            }
            scripts.push(TondiP2mrScriptSpend {
                leaf_script: leaf.script.clone(),
                leaf_version: leaf.leaf_version,
                control_block,
            });
        }

        scripts.sort_by(|a, b| {
            a.leaf_script
                .cmp(&b.leaf_script)
                .then_with(|| {
                    a.leaf_version.to_consensus_u8().cmp(&b.leaf_version.to_consensus_u8())
                })
                .then_with(|| a.control_block.len().cmp(&b.control_block.len()))
                .then_with(|| a.control_block.cmp(&b.control_block))
        });

        Ok(TondiP2mrSpendData {
            merkle_root: root.hash,
            scripts,
        })
    }

    /// Validates DFS depth order for a P2MR tree description.
    pub fn valid_depths(depths: impl IntoIterator<Item = u8>) -> bool {
        let mut branch = Vec::<bool>::new();
        for depth in depths {
            let mut depth = depth as usize;
            if depth > TONDI_P2MR_CONTROL_MAX_NODE_COUNT || depth + 1 < branch.len() {
                return false;
            }
            while branch.len() > depth && branch[depth] {
                branch.pop();
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            if branch.len() <= depth {
                branch.resize(depth + 1, false);
            }
            if branch[depth] {
                return false;
            }
            branch[depth] = true;
        }
        branch.len() == 1 && branch[0]
    }

    fn complete_root(&self) -> Result<&P2mrNodeInfo, TondiP2mrTreeError> {
        if self.branches.is_empty() {
            return Err(TondiP2mrTreeError::EmptyTree);
        }
        if !self.is_complete() {
            return Err(TondiP2mrTreeError::IncompleteTree);
        }
        Ok(self.branches[0].as_ref().expect("complete P2MR tree has a root"))
    }

    fn insert(&mut self, depth: usize, mut node: P2mrNodeInfo) -> Result<(), TondiP2mrTreeError> {
        if depth > TONDI_P2MR_CONTROL_MAX_NODE_COUNT {
            return Err(TondiP2mrTreeError::DepthTooLarge {
                depth,
                max: TONDI_P2MR_CONTROL_MAX_NODE_COUNT,
            });
        }
        if depth + 1 < self.branches.len() {
            return Err(TondiP2mrTreeError::InvalidDepthOrder);
        }

        let mut depth = depth;
        while self.branches.len() > depth && self.branches[depth].is_some() {
            let sibling =
                self.branches.pop().and_then(|entry| entry).expect("branch at depth is populated");
            node = P2mrNodeInfo::combine(node, sibling);
            if depth == 0 {
                return Err(TondiP2mrTreeError::InvalidDepthOrder);
            }
            depth -= 1;
        }

        if self.branches.len() <= depth {
            self.branches.resize_with(depth + 1, || None);
        }
        self.branches[depth] = Some(node);
        Ok(())
    }
}

/// Builds the standard single-key Tondi P2MR leaf script.
pub fn tondi_p2mr_single_key_leaf_script(xonly_public_key: XOnlyPk) -> Vec<u8> {
    let mut script = Vec::with_capacity(34);
    script.push(0x20);
    script.extend_from_slice(&xonly_public_key.to_byte_array());
    script.push(OP_CHECKSIG);
    script
}

/// Builds spend data for the standard single-key Tondi P2MR leaf.
pub fn tondi_p2mr_single_key_spend_data(xonly_public_key: XOnlyPk) -> TondiP2mrSpendData {
    let mut builder = TondiP2mrTreeBuilder::new();
    builder
        .add_tapscript_leaf(0, tondi_p2mr_single_key_leaf_script(xonly_public_key))
        .expect("standard single-key P2MR leaf is valid");
    builder.spend_data().expect("standard single-key P2MR tree is complete")
}

/// Computes the standard single-key Tondi P2MR root.
pub fn tondi_p2mr_single_key_root(xonly_public_key: XOnlyPk) -> [u8; 32] {
    tondi_p2mr_single_key_spend_data(xonly_public_key).merkle_root
}

/// Encodes a Tondi P2MR Borsh witness stack.
pub fn tondi_p2mr_encode_witness_stack(
    stack: impl IntoIterator<Item = Vec<u8>>,
) -> Result<Vec<u8>, TondiP2mrTreeError> {
    let stack = stack.into_iter().collect::<Vec<_>>();
    let mut bytes = Vec::new();
    append_p2mr_witness_len(&mut bytes, stack.len())
        .ok_or(TondiP2mrTreeError::TooManyWitnessItems)?;
    for item in stack {
        append_p2mr_witness_len(&mut bytes, item.len())
            .ok_or(TondiP2mrTreeError::WitnessItemTooLarge)?;
        bytes.extend_from_slice(&item);
    }
    Ok(bytes)
}

fn append_p2mr_witness_len(bytes: &mut Vec<u8>, len: usize) -> Option<()> {
    let len = u32::try_from(len).ok()?;
    bytes.extend_from_slice(&len.to_le_bytes());
    Some(())
}

impl IntoTapHash for TapLeafHash {
    fn into_tap_hash(self) -> TapNodeHash { TapNodeHash(self.0) }
}

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictEncode, StrictDecode, StrictDumb)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct TapBranchHash(
    #[from]
    #[from([u8; 32])]
    Bytes32,
);

impl TapBranchHash {
    pub fn with_nodes(node1: TapNodeHash, node2: TapNodeHash) -> Self {
        let mut engine = TondiBlake3::from_tag(COPPEROOT_BRANCH_TAG);
        engine.input_raw(cmp::min(&node1, &node2).borrow());
        engine.input_raw(cmp::max(&node1, &node2).borrow());
        Self(engine.finish().into())
    }
}

impl IntoTapHash for TapBranchHash {
    fn into_tap_hash(self) -> TapNodeHash { TapNodeHash(self.0) }
}

#[derive(Wrapper, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Index, RangeOps, AsSlice, BorrowSlice, Hex, Display, FromStr)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct TapNodeHash(
    #[from]
    #[from([u8; 32])]
    #[from(TapLeafHash)]
    #[from(TapBranchHash)]
    Bytes32,
);

impl IntoTapHash for TapNodeHash {
    fn into_tap_hash(self) -> TapNodeHash { self }
}

#[derive(Wrapper, WrapperMut, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From, Default)]
#[wrapper(Deref)]
#[wrapper_mut(DerefMut)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct TapMerklePath(Confined<Vec<TapBranchHash>, 0, 128>);

impl IntoIterator for TapMerklePath {
    type Item = TapBranchHash;
    type IntoIter = vec::IntoIter<TapBranchHash>;

    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl<'a> IntoIterator for &'a TapMerklePath {
    type Item = &'a TapBranchHash;
    type IntoIter = slice::Iter<'a, TapBranchHash>;

    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

impl TapMerklePath {
    /// Tries to construct a confinement over a collection. Fails if the number
    /// of items in the collection exceeds one of the confinement bounds.
    // We can't use `impl TryFrom` due to the conflict with core library blanked
    // implementation
    #[inline]
    pub fn try_from(path: Vec<TapBranchHash>) -> Result<Self, confinement::Error> {
        Confined::try_from(path).map(Self::from_inner)
    }

    /// Tries to construct a confinement with a collection of elements taken
    /// from an iterator. Fails if the number of items in the collection
    /// exceeds one of the confinement bounds.
    #[inline]
    pub fn try_from_iter<I: IntoIterator<Item = TapBranchHash>>(
        iter: I,
    ) -> Result<Self, confinement::Error> {
        Confined::try_from_iter(iter).map(Self::from_inner)
    }
}

/// Taproot annex prefix.
pub const TAPROOT_ANNEX_PREFIX: u8 = 0x50;

/// Tapscript leaf version.
// https://github.com/tondi/tondi/blob/e826b22da252e0599c61d21c98ff89f366b3120f/src/script/interpreter.h#L226
pub const TAPROOT_LEAF_TAPSCRIPT: u8 = 0xc0;

/// Tapleaf mask for getting the leaf version from first byte of control block.
// https://github.com/tondi/tondi/blob/e826b22da252e0599c61d21c98ff89f366b3120f/src/script/interpreter.h#L225
pub const TAPROOT_LEAF_MASK: u8 = 0xfe;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Display, Error)]
#[display(doc_comments)]
/// invalid taproot leaf version {0}.
pub struct InvalidLeafVer(u8);

/// The leaf version for tapleafs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub enum LeafVer {
    /// BIP-342 tapscript.
    #[default]
    TapScript,

    /// Future leaf version.
    Future(FutureLeafVer),
}

impl StrictType for LeafVer {
    const STRICT_LIB_NAME: &'static str = LIB_NAME_TONDI;
    fn strict_name() -> Option<TypeName> { Some(tn!("LeafVer")) }
}
impl StrictProduct for LeafVer {}
impl StrictTuple for LeafVer {
    const FIELD_COUNT: u8 = 1;
}
impl StrictEncode for LeafVer {
    fn strict_encode<W: TypedWrite>(&self, writer: W) -> std::io::Result<W> {
        writer.write_tuple::<Self>(|w| Ok(w.write_field(&self.to_consensus_u8())?.complete()))
    }
}
impl StrictDecode for LeafVer {
    fn strict_decode(reader: &mut impl TypedRead) -> Result<Self, DecodeError> {
        reader.read_tuple(|r| {
            let version = r.read_field()?;
            Self::from_consensus_u8(version)
                .map_err(|err| DecodeError::DataIntegrityError(err.to_string()))
        })
    }
}

impl LeafVer {
    #[doc(hidden)]
    #[deprecated(since = "0.10.9", note = "use from_consensus_u8")]
    pub fn from_consensus(version: u8) -> Result<Self, InvalidLeafVer> {
        Self::from_consensus_u8(version)
    }

    /// Creates a [`LeafVer`] from consensus byte representation.
    ///
    /// # Errors
    ///
    /// - If the last bit of the `version` is odd.
    /// - If the `version` is 0x50 ([`TAPROOT_ANNEX_PREFIX`]).
    pub fn from_consensus_u8(version: u8) -> Result<Self, InvalidLeafVer> {
        match version {
            TAPROOT_LEAF_TAPSCRIPT => Ok(LeafVer::TapScript),
            TAPROOT_ANNEX_PREFIX => Err(InvalidLeafVer(TAPROOT_ANNEX_PREFIX)),
            future => FutureLeafVer::from_consensus(future).map(LeafVer::Future),
        }
    }

    #[doc(hidden)]
    #[deprecated(since = "0.10.9", note = "use to_consensus_u8")]
    pub fn to_consensus(self) -> u8 { self.to_consensus_u8() }

    /// Returns the consensus representation of this [`LeafVer`].
    pub fn to_consensus_u8(self) -> u8 {
        match self {
            LeafVer::TapScript => TAPROOT_LEAF_TAPSCRIPT,
            LeafVer::Future(version) => version.to_consensus(),
        }
    }
}

impl LowerHex for LeafVer {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result { LowerHex::fmt(&self.to_consensus_u8(), f) }
}

impl UpperHex for LeafVer {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result { UpperHex::fmt(&self.to_consensus_u8(), f) }
}

/// Inner type representing future (non-tapscript) leaf versions. See
/// [`LeafVer::Future`].
///
/// NB: NO PUBLIC CONSTRUCTOR!
/// The only way to construct this is by converting `u8` to [`LeafVer`] and then
/// extracting it.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI, dumb = { Self(0x51) })]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct FutureLeafVer(u8);

impl FutureLeafVer {
    pub(self) fn from_consensus(version: u8) -> Result<FutureLeafVer, InvalidLeafVer> {
        match version {
            TAPROOT_LEAF_TAPSCRIPT => unreachable!(
                "FutureLeafVersion::from_consensus should be never called for 0xC0 value"
            ),
            TAPROOT_ANNEX_PREFIX => Err(InvalidLeafVer(TAPROOT_ANNEX_PREFIX)),
            odd if odd & 0xFE != odd => Err(InvalidLeafVer(odd)),
            even => Ok(FutureLeafVer(even)),
        }
    }

    /// Returns the consensus representation of this [`FutureLeafVer`].
    #[inline]
    pub fn to_consensus(self) -> u8 { self.0 }
}

impl LowerHex for FutureLeafVer {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { LowerHex::fmt(&self.0, f) }
}

impl UpperHex for FutureLeafVer {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { UpperHex::fmt(&self.0, f) }
}

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Default, Display)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[display("{version:04x} {script:x}")]
pub struct LeafScript {
    pub version: LeafVer,
    pub script: ScriptBytes,
}

// TODO: Impl Hex and FromStr for LeafScript

impl From<TapScript> for LeafScript {
    fn from(tap_script: TapScript) -> Self {
        LeafScript {
            version: LeafVer::TapScript,
            script: tap_script.into_inner(),
        }
    }
}

impl LeafScript {
    #[inline]
    pub fn new(version: LeafVer, script: ScriptBytes) -> Self { LeafScript { version, script } }
    #[inline]
    pub fn with_bytes(version: LeafVer, script: Vec<u8>) -> Result<Self, confinement::Error> {
        Ok(LeafScript {
            version,
            script: ScriptBytes::try_from(script)?,
        })
    }
    #[inline]
    pub fn from_tap_script(tap_script: TapScript) -> Self { Self::from(tap_script) }

    #[inline]
    pub fn as_script_bytes(&self) -> &ScriptBytes { &self.script }

    #[inline]
    pub fn tap_leaf_hash(&self) -> TapLeafHash { TapLeafHash::with_leaf_script(self) }
}

#[derive(Wrapper, WrapperMut, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From, Default)]
#[wrapper(Deref, AsSlice, Hex)]
#[wrapper_mut(DerefMut, AsSliceMut)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct TapScript(ScriptBytes);
// TODO: impl Display/FromStr for TapScript providing correct opcodes

impl TryFrom<Vec<u8>> for TapScript {
    type Error = confinement::Error;
    fn try_from(script_bytes: Vec<u8>) -> Result<Self, Self::Error> {
        ScriptBytes::try_from(script_bytes).map(Self)
    }
}

impl TapScript {
    #[inline]
    pub fn new() -> Self { Self::default() }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(ScriptBytes::from(Confined::with_capacity(capacity)))
    }

    /// Constructs script object assuming the script length is less than 4GB.
    /// Panics otherwise.
    #[inline]
    pub fn from_checked(script_bytes: Vec<u8>) -> Self {
        Self(ScriptBytes::from_checked(script_bytes))
    }

    #[inline]
    pub fn tap_leaf_hash(&self) -> TapLeafHash { TapLeafHash::with_tap_script(self) }

    /// Adds a single opcode to the script.
    #[inline]
    pub fn push_opcode(&mut self, op_code: TapCode) { self.0.push(op_code as u8); }

    #[inline]
    pub fn as_script_bytes(&self) -> &ScriptBytes { &self.0 }
}

impl ScriptPubkey {
    pub fn p2tr(internal_key: InternalPk, merkle_root: Option<TapNodeHash>) -> Self {
        let (output_key, _) = internal_key.to_output_pk(merkle_root);
        Self::p2tr_tweaked(output_key)
    }

    pub fn p2tr_key_only(internal_key: InternalPk) -> Self {
        let (output_key, _) = internal_key.to_output_pk(None);
        Self::p2tr_tweaked(output_key)
    }

    pub fn p2tr_scripted(internal_key: InternalPk, merkle_root: impl IntoTapHash) -> Self {
        let (output_key, _) = internal_key.to_output_pk(Some(merkle_root.into_tap_hash()));
        Self::p2tr_tweaked(output_key)
    }

    pub fn p2tr_tweaked(output_key: OutputPk) -> Self {
        // output key is 32 bytes long, so it's safe to use
        // `new_witness_program_unchecked` (Segwitv1)
        Self::with_witness_program_unchecked(WitnessVer::V1, &output_key.serialize())
    }

    pub fn is_p2tr(&self) -> bool {
        self.len() == 34 && self[0] == WitnessVer::V1.op_code() as u8 && self[1] == OP_PUSHBYTES_32
    }
}

/// invalid parity value {0} - must be 0 or 1
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, Error)]
#[display(doc_comments)]
pub struct InvalidParityValue(pub u8);

/// Represents the parity passed between FFI function calls.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Display)]
#[display(lowercase)]
#[derive(StrictType, StrictEncode, StrictDecode, StrictDumb)]
#[strict_type(lib = LIB_NAME_TONDI, tags = repr, into_u8, try_from_u8)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
#[repr(u8)]
pub enum Parity {
    /// Even parity.
    #[strict_type(dumb)]
    Even = 0,
    /// Odd parity.
    Odd = 1,
}

impl From<secp256k1::Parity> for Parity {
    fn from(parity: secp256k1::Parity) -> Self {
        match parity {
            secp256k1::Parity::Even => Parity::Even,
            secp256k1::Parity::Odd => Parity::Odd,
        }
    }
}

impl Parity {
    /// Converts parity into an integer (byte) value.
    ///
    /// This returns `0` for even parity and `1` for odd parity.
    pub fn to_consensus_u8(self) -> u8 { self as u8 }

    /// Constructs a [`Parity`] from a byte.
    ///
    /// The only allowed values are `0` meaning even parity and `1` meaning odd.
    /// Other values result in error being returned.
    pub fn from_consensus_u8(parity: u8) -> Result<Parity, InvalidParityValue> {
        match parity {
            0 => Ok(Parity::Even),
            1 => Ok(Parity::Odd),
            invalid => Err(InvalidParityValue(invalid)),
        }
    }
}

/// Returns even parity if the operands are equal, odd otherwise.
impl BitXor for Parity {
    type Output = Parity;

    fn bitxor(self, rhs: Parity) -> Self::Output {
        // This works because Parity has only two values (i.e. only 1 bit of
        // information).
        if self == rhs {
            Parity::Even // 1^1==0 and 0^0==0
        } else {
            Parity::Odd // 1^0==1 and 0^1==1
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictEncode, StrictDecode, StrictDumb)]
#[strict_type(lib = LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "camelCase"))]
pub struct ControlBlock {
    /// The tapleaf version.
    pub leaf_version: LeafVer,
    /// The parity of the output key (NOT THE INTERNAL KEY WHICH IS ALWAYS
    /// XONLY).
    pub output_key_parity: Parity,
    /// The internal key.
    pub internal_pk: InternalPk,
    /// The merkle proof of a script associated with this leaf.
    pub merkle_branch: TapMerklePath,
}

impl ControlBlock {
    #[inline]
    pub fn with(
        leaf_version: LeafVer,
        internal_pk: InternalPk,
        output_key_parity: Parity,
        merkle_branch: TapMerklePath,
    ) -> Self {
        ControlBlock {
            leaf_version,
            output_key_parity,
            internal_pk,
            merkle_branch,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum AnnexError {
    /// invalid first annex byte `{0:#02x}`, which must be `0x50`.
    WrongFirstByte(u8),

    #[from]
    #[display(inner)]
    Size(confinement::Error),
}

/// The `Annex` struct enforces first byte to be `0x50`.
#[derive(Wrapper, WrapperMut, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, From)]
#[wrapper(Deref, AsSlice, Hex)]
#[wrapper_mut(DerefMut, AsSliceMut)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_TONDI, dumb = { Self(VarIntBytes::with(0x50)) })]
pub struct Annex(VarIntBytes<1>);

impl TryFrom<Vec<u8>> for Annex {
    type Error = confinement::Error;
    fn try_from(script_bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Confined::try_from(script_bytes).map(Self)
    }
}

impl Annex {
    /// Creates a new `Annex` struct checking the first byte is `0x50`.
    /// Constructs script object assuming the script length is less than 4GB.
    /// Panics otherwise.
    #[inline]
    pub fn new(annex_bytes: Vec<u8>) -> Result<Self, AnnexError> {
        let annex = Confined::try_from(annex_bytes).map(Self)?;
        if annex[0] != TAPROOT_ANNEX_PREFIX {
            return Err(AnnexError::WrongFirstByte(annex[0]));
        }
        Ok(annex)
    }

    pub fn len_var_int(&self) -> VarInt { VarInt(self.len() as u64) }

    pub fn into_vec(self) -> Vec<u8> { self.0.release() }

    /// Returns the Annex bytes data (including first byte `0x50`).
    pub fn as_slice(&self) -> &[u8] { self.0.as_slice() }

    pub(crate) fn as_var_int_bytes(&self) -> &VarIntBytes<1> { &self.0 }
}

#[cfg(test)]
mod tests {
    use amplify::hex::FromHex;

    use super::*;

    #[test]
    fn tondi_p2mr_root_uses_tapleaf_sha256() {
        let expected = <[u8; 32]>::from_hex(
            "a85b2107f791b26a84e7586c28cec7cb61202ed3d01944d832500f363782d675",
        )
        .expect("valid hash");

        assert_eq!(tondi_p2mr_root_from_tapscript(&[0x51]), expected);
    }

    #[test]
    fn tondi_p2mr_tree_builder_tracks_multileaf_control_blocks() {
        let left = vec![0x51];
        let mut right = Vec::with_capacity(34);
        right.push(0x20);
        right.extend_from_slice(&[7u8; 32]);
        right.push(OP_CHECKSIG);

        let mut builder = TondiP2mrTreeBuilder::new();
        builder.add_tapscript_leaf(1, &left).unwrap().add_tapscript_leaf(1, &right).unwrap();
        let spend_data = builder.spend_data().unwrap();

        let left_hash = tondi_p2mr_root_from_tapscript(&left);
        let right_hash = tondi_p2mr_root_from_tapscript(&right);
        assert_eq!(spend_data.merkle_root, tondi_p2mr_branch_hash(left_hash, right_hash));

        let left_spend = spend_data.shortest_spend_for(&left, LeafVer::TapScript).unwrap();
        assert_eq!(left_spend.control_block[0], TAPROOT_LEAF_TAPSCRIPT | 1);
        assert_eq!(&left_spend.control_block[1..], right_hash.as_slice());

        let right_spend = spend_data.shortest_spend_for(&right, LeafVer::TapScript).unwrap();
        assert_eq!(right_spend.control_block[0], TAPROOT_LEAF_TAPSCRIPT | 1);
        assert_eq!(&right_spend.control_block[1..], left_hash.as_slice());
    }

    #[test]
    fn tondi_p2mr_tree_builder_supports_hidden_branches() {
        let visible = vec![0x51];
        let hidden = [9u8; 32];

        let mut builder = TondiP2mrTreeBuilder::new();
        builder.add_tapscript_leaf(1, &visible).unwrap().add_hidden(1, hidden).unwrap();
        let spend_data = builder.spend_data().unwrap();

        let visible_hash = tondi_p2mr_root_from_tapscript(&visible);
        assert_eq!(spend_data.merkle_root, tondi_p2mr_branch_hash(visible_hash, hidden));
        assert_eq!(spend_data.scripts.len(), 1);
        assert_eq!(&spend_data.scripts[0].control_block[1..], hidden.as_slice());
    }

    #[test]
    fn tondi_p2mr_single_key_spend_data_encodes_witness_stack() {
        let secret_key = secp256k1::SecretKey::from_slice(&[3u8; 32]).unwrap();
        let public_key = secp256k1::PublicKey::from_secret_key(secp256k1::SECP256K1, &secret_key);
        let xonly = XOnlyPk::from_byte_array(public_key.x_only_public_key().0.serialize()).unwrap();
        let spend_data = tondi_p2mr_single_key_spend_data(xonly);
        let spend = spend_data.scripts.first().unwrap();

        assert_eq!(spend_data.merkle_root, tondi_p2mr_single_key_root(xonly));
        assert_eq!(spend.control_block, TONDI_P2MR_CONTROL_BLOCK_SINGLE_LEAF);

        let encoded = spend.encode_witness([vec![1u8; 64]], None).unwrap();
        let mut cursor = encoded.as_slice();
        assert_eq!(read_test_witness_len(&mut cursor), 3);
        assert_eq!(read_test_witness_item(&mut cursor), vec![1u8; 64]);
        assert_eq!(read_test_witness_item(&mut cursor), spend.leaf_script);
        assert_eq!(read_test_witness_item(&mut cursor), spend.control_block);
        assert!(cursor.is_empty());
    }

    fn read_test_witness_len(cursor: &mut &[u8]) -> usize {
        let (len, rest) = cursor.split_at(4);
        *cursor = rest;
        u32::from_le_bytes(len.try_into().unwrap()) as usize
    }

    fn read_test_witness_item(cursor: &mut &[u8]) -> Vec<u8> {
        let len = read_test_witness_len(cursor);
        let (item, rest) = cursor.split_at(len);
        *cursor = rest;
        item.to_vec()
    }
}

#[cfg(feature = "serde")]
mod _serde {
    use amplify::hex::{FromHex, ToHex};
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    impl Serialize for Annex {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer {
            if serializer.is_human_readable() {
                serializer.serialize_str(&self.to_hex())
            } else {
                serializer.serialize_bytes(self.as_slice())
            }
        }
    }

    impl<'de> Deserialize<'de> for Annex {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de> {
            if deserializer.is_human_readable() {
                String::deserialize(deserializer).and_then(|string| {
                    Self::from_hex(&string).map_err(|_| D::Error::custom("wrong hex data"))
                })
            } else {
                let bytes = Vec::<u8>::deserialize(deserializer)?;
                Self::new(bytes).map_err(|_| D::Error::custom("invalid annex data"))
            }
        }
    }
}
