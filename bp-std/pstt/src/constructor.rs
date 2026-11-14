// Modern, minimalistic & standard-compliant cold wallet library.
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2020-2023 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2020-2023 LNP/BP Standards Association. All rights reserved.
// Copyright (C) 2020-2023 Dr Maxim Orlovsky. All rights reserved.
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

use std::num::ParseIntError;
use std::str::FromStr;

use derive::{
    Address, AddressNetwork, AddressParseError, Keychain, Network, NormalIndex, Outpoint, Sats,
    ScriptPubkey, Terminal, Vout,
};
use descriptors::Descriptor;

use crate::{Pstt, PsttError};
use serde::{Deserialize, Serialize};
use tondi_vm::{
    builder::{AuthDescriptor, DraftInput},
    AuthRoot, Hash32, ResolvedInput, StateNote,
};

#[derive(Clone, Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum ConstructionError {
    #[display(inner)]
    #[from]
    Pstt(PsttError),

    /// the input spending {0} is not known for the current wallet.
    UnknownInput(Outpoint),

    /// impossible to construct a transaction having no inputs.
    NoInputs,

    /// the total payment amount ({0} sats) exceeds the number of sats in existence.
    Overflow(Sats),

    /// attempt to spend more than present in transaction inputs. Total transaction inputs are
    /// {input_value} sats, but the output is {output_value} sats.
    OutputExceedsInputs {
        input_value: Sats,
        output_value: Sats,
    },

    /// not enough funds to pay the fee of {fee} sats; the sum of inputs is {input_value} sats,
    /// and outputs spend {output_value} sats out of them.
    NoFundsForFee {
        input_value: Sats,
        output_value: Sats,
        fee: Sats,
    },

    /// network for address {0} mismatches the one used by the wallet.
    NetworkMismatch(Address),

    /// external RGB beneficiaries are disabled for Tondi-native P2MR.
    ExternalOutputDisabled,
}

#[derive(Clone, Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum BeneficiaryParseError {
    #[display("invalid format of the invoice")]
    InvalidFormat,

    #[from]
    Int(ParseIntError),

    #[from]
    Address(AddressParseError),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display, From)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Payment {
    #[from]
    #[display(inner)]
    Fixed(Sats),
    #[display("MAX")]
    Max,
}

impl Payment {
    #[inline]
    pub fn sats(&self) -> Option<Sats> {
        match self {
            Payment::Fixed(sats) => Some(*sats),
            Payment::Max => None,
        }
    }

    #[inline]
    pub fn unwrap_or(&self, default: impl Into<Sats>) -> Sats {
        self.sats().unwrap_or(default.into())
    }

    #[inline]
    pub fn is_max(&self) -> bool {
        *self == Payment::Max
    }
}

impl FromStr for Payment {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "MAX" {
            return Ok(Payment::Max);
        }
        Sats::from_str(s).map(Payment::Fixed)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
#[display("{amount}@{address}", alt = "tondi:{address}?amount={amount}")]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Beneficiary {
    pub address: Address,
    pub amount: Payment,
}

impl Beneficiary {
    #[inline]
    pub fn new(address: Address, amount: impl Into<Payment>) -> Self {
        Beneficiary {
            address,
            amount: amount.into(),
        }
    }
    #[inline]
    pub fn with_max(address: Address) -> Self {
        Beneficiary {
            address,
            amount: Payment::Max,
        }
    }
    #[inline]
    pub fn is_max(&self) -> bool {
        self.amount.is_max()
    }
    #[inline]
    pub fn script_pubkey(&self) -> ScriptPubkey {
        self.address.script_pubkey()
    }
}

impl FromStr for Beneficiary {
    type Err = BeneficiaryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (amount, beneficiary) =
            s.split_once('@').ok_or(BeneficiaryParseError::InvalidFormat)?;
        Ok(Beneficiary::new(Address::from_str(beneficiary)?, Payment::from_str(amount)?))
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TxParams {
    pub fee: Sats,
    #[cfg_attr(feature = "serde", serde(with = "tondi_vm::codec::u64_decimal"))]
    pub since: u64,
    pub change_shift: bool,
    pub change_keychain: Keychain,
}

impl TxParams {
    pub fn with(fee: Sats) -> Self {
        TxParams {
            fee,
            since: 0,
            change_shift: true,
            change_keychain: Keychain::INNER,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ChangeInfo {
    pub vout: Vout,
    pub terminal: Terminal,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PsttMeta {
    pub network: AddressNetwork,
    pub fee: Sats,
    pub weight: u32,
    pub size: u32,
    pub change: Option<ChangeInfo>,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Utxo {
    pub outpoint: Outpoint,
    pub entry: ResolvedInput,
    pub terminal: Terminal,
}
impl std::hash::Hash for Utxo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.outpoint, state)
    }
}
impl Utxo {
    pub fn value(&self) -> Sats {
        Sats(self.entry.note.value)
    }
}

/// Wallet construction is single-key and explicit about the complete resolved note.
/// Advanced Auth/Law transactions use `Pstt::from_native` without rebuilding their Actions.
pub trait PsttConstructor {
    type Key;
    type Descr: Descriptor<Self::Key>;
    fn descriptor(&self) -> &Self::Descr;
    fn utxo(&self, outpoint: Outpoint) -> Option<Utxo>;
    fn network(&self) -> Network;
    fn network_domain(&self) -> Hash32;
    fn next_derivation_index(&mut self, keychain: impl Into<Keychain>, shift: bool) -> NormalIndex;
    fn construct_pstt(
        &mut self,
        coins: impl IntoIterator<Item = Outpoint>,
        beneficiaries: impl IntoIterator<Item = Beneficiary>,
        params: TxParams,
    ) -> Result<(Pstt, PsttMeta), ConstructionError> {
        let mut p = Pstt::create(self.network_domain());
        for coin in coins {
            if p.inputs().any(|i| i.previous_outpoint == coin) {
                continue;
            }
            let u = self.utxo(coin).ok_or(ConstructionError::UnknownInput(coin))?;
            if !u.entry.note.assets.is_empty() {
                return Err(PsttError(
                    "native asset inputs require an explicit transfer draft".into(),
                )
                .into());
            }
            if u.entry.note.state.is_some()
                && tondi_wallet_pstt::extensions::rgb::RgbAnchor::from_note(&u.entry.note)
                    .map_err(PsttError::from)?
                    .is_none()
            {
                return Err(PsttError("protocol state requires an explicit draft".into()).into());
            }
            let keys = self.descriptor().xonly_keyset(u.terminal);
            if keys.len() != 1 {
                return Err(PsttError("single-key Auth descriptor required".into()).into());
            }
            let key = keys.keys().next().expect("one key");
            let (root, leaf) = tondi_vm::schnorr_auth(key.to_byte_array());
            if root != u.entry.note.auth {
                return Err(PsttError("resolved note Auth differs from descriptor".into()).into());
            }
            p.append_input(
                DraftInput {
                    outpoint: coin.into(),
                    entry: u.entry,
                    auth: Some(AuthDescriptor {
                        leaf,
                        merkle_path: vec![],
                    }),
                    since: params.since,
                    consume: true,
                },
                Some(u.terminal),
            )?;
        }
        if p.inputs().next().is_none() {
            return Err(ConstructionError::NoInputs);
        }
        let input_value = p.input_sum()?;
        let mut fixed = 0u64;
        let mut max = vec![];
        for b in beneficiaries {
            if b.address.network != self.network().into() {
                return Err(ConstructionError::NetworkMismatch(b.address));
            }
            let script = b.script_pubkey();
            let bytes: [u8; 32] = script
                .as_slice()
                .try_into()
                .map_err(|_| PsttError("beneficiary must contain a native Auth root".into()))?;
            let value = b.amount.unwrap_or(Sats::ZERO).0;
            fixed = fixed.checked_add(value).ok_or(ConstructionError::Overflow(Sats(fixed)))?;
            let out = p.append_output(
                StateNote::plain(
                    value,
                    AuthRoot {
                        version: tondi_vm::AUTH_P2MR,
                        merkle_root: Hash32::from_bytes(bytes),
                    },
                ),
                None,
            )?;
            if b.is_max() {
                max.push(out.index);
            }
        }
        let mut remaining =
            input_value.0.checked_sub(fixed).and_then(|n| n.checked_sub(params.fee.0)).ok_or(
                ConstructionError::NoFundsForFee {
                    input_value,
                    output_value: Sats(fixed),
                    fee: params.fee,
                },
            )?;
        if !max.is_empty() {
            let portion = remaining / max.len() as u64;
            let extra = remaining % max.len() as u64;
            for (rank, index) in max.iter().enumerate() {
                p.output_mut(*index).expect("output").note.value =
                    portion + u64::from((rank as u64) < extra);
            }
            remaining = 0;
        }
        let change = if remaining != 0 {
            let index = self.next_derivation_index(params.change_keychain, params.change_shift);
            let terminal = Terminal::new(params.change_keychain, index);
            let keys = self.descriptor().xonly_keyset(terminal);
            if keys.len() != 1 {
                return Err(PsttError("single-key change descriptor required".into()).into());
            }
            let auth =
                tondi_vm::schnorr_auth(keys.keys().next().expect("one key").to_byte_array()).0;
            let index = p.append_output(StateNote::plain(remaining, auth), Some(terminal))?.index;
            Some(ChangeInfo {
                vout: Vout::from_u32(index as u32),
                terminal,
            })
        } else {
            None
        };
        for output in p.outputs() {
            if output.note.value
                < output.note.occupied_bytes() * tondi_vm::Limits::default().capacity_per_byte
            {
                return Err(PsttError("output below occupied capacity".into()).into());
            }
        }
        let meta = PsttMeta {
            network: self.network().into(),
            fee: params.fee,
            weight: 0,
            size: 0,
            change,
        };
        self.after_construct_pstt(&mut p, &meta);
        p.complete_construction();
        Ok((p, meta))
    }
    fn after_construct_pstt(&mut self, _pstt: &mut Pstt, _meta: &PsttMeta) {}
}
