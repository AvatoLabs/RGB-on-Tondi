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

use core::ops::{Deref, DerefMut};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use amplify::confinement::{self, KeyedCollection};
use amplify::MultiError;
use bpstd::pstt::{
    ConstructionError, KeyMap, MmbPsttError, MpcPsttError, PsttConstructor, PsttMeta, TxParams,
    UnfinalizedInputs,
};
use bpstd::seals::{TondiP2mrError, TondiP2mrProof, TxoSeal};
use bpstd::{CompressedPk, IdxBase, Pstt, Sats, ScriptPubkey, Tx, Vout};
use commit_verify::mpc;
use rgb::invoice::{RgbBeneficiary, RgbInvoice};
use rgb::popls::bp::{
    BundleError, Coinselect, FulfillError, IncludeError, OpRequestSet, PaymentScript, PrefabBundle,
    RgbWallet, WalletProvider,
};
use rgb::{
    AuthToken, CodexId, Contract, ContractId, Contracts, EitherSeal, Issuer, Pile, Stock, Stockpile,
};
use rgpstt::{RgbPstt, RgbPsttCsvError, RgbPsttPrepareError};

use crate::p2mr::p2mr_original_pubkey_key;
use crate::CoinselectStrategy;

/// Payment structure is used in the process of RBF (replace by fee). It is returned by
/// [`RgbRuntime::pay_invoice`] method when the original transaction is created, and then must be
/// provided to [`RgbRuntime::rbf`] to do the RBF transaction.
#[derive(Clone, Eq, PartialEq, Debug)]
// TODO: Add Deserialize once implemented in Pstt
//#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "camelCase"))]
pub struct Payment {
    pub uncomit_pstt: Pstt,
    pub pstt_meta: PsttMeta,
    pub bundle: PrefabBundle,
    pub terminals: BTreeSet<AuthToken>,
}

/// RGB Runtime is a lightweight stateless layer integrating some wallet provider (`Wallet` generic
/// parameter) and RGB stockpile (`Sp` generic parameter).
///
/// It provides
/// - synchronization for the history of witness transactions, extending the main wallet UTXO set
///   synchronization ([`Self::sync`]);
/// - low-level methods for working with PSTTs using `bp-std` library (these methods utilize
///   [`rgb-pstt`] crate) - like [`Self::compose_pstt`] and [`Self::color_pstt`];
/// - high-level payment methods ([`Self::pay`], [`Self::rbf`]) relaying on the above.
pub struct RgbRuntime<
    W,
    Sp,
    S = HashMap<CodexId, Issuer>,
    C = HashMap<ContractId, Contract<<Sp as Stockpile>::Stock, <Sp as Stockpile>::Pile>>,
>(RgbWallet<W, Sp, S, C>)
where
    W: WalletProvider,
    Sp: Stockpile,
    Sp::Pile: Pile<Seal = TxoSeal>,
    S: KeyedCollection<Key = CodexId, Value = Issuer>,
    C: KeyedCollection<Key = ContractId, Value = Contract<Sp::Stock, Sp::Pile>>;

impl<W, Sp, S, C> From<RgbWallet<W, Sp, S, C>> for RgbRuntime<W, Sp, S, C>
where
    W: WalletProvider,
    Sp: Stockpile,
    Sp::Pile: Pile<Seal = TxoSeal>,
    S: KeyedCollection<Key = CodexId, Value = Issuer>,
    C: KeyedCollection<Key = ContractId, Value = Contract<Sp::Stock, Sp::Pile>>,
{
    fn from(wallet: RgbWallet<W, Sp, S, C>) -> Self { Self(wallet) }
}

impl<W, Sp, S, C> Deref for RgbRuntime<W, Sp, S, C>
where
    W: WalletProvider,
    Sp: Stockpile,
    Sp::Pile: Pile<Seal = TxoSeal>,
    S: KeyedCollection<Key = CodexId, Value = Issuer>,
    C: KeyedCollection<Key = ContractId, Value = Contract<Sp::Stock, Sp::Pile>>,
{
    type Target = RgbWallet<W, Sp, S, C>;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl<W, Sp, S, C> DerefMut for RgbRuntime<W, Sp, S, C>
where
    W: WalletProvider,
    Sp: Stockpile,
    Sp::Pile: Pile<Seal = TxoSeal>,
    S: KeyedCollection<Key = CodexId, Value = Issuer>,
    C: KeyedCollection<Key = ContractId, Value = Contract<Sp::Stock, Sp::Pile>>,
{
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl<W, Sp, S, C> RgbRuntime<W, Sp, S, C>
where
    W: WalletProvider,
    Sp: Stockpile,
    Sp::Pile: Pile<Seal = TxoSeal>,
    S: KeyedCollection<Key = CodexId, Value = Issuer>,
    C: KeyedCollection<Key = ContractId, Value = Contract<Sp::Stock, Sp::Pile>>,
{
    pub fn with_components(wallet: W, contracts: Contracts<Sp, S, C>) -> Self {
        Self(RgbWallet::with_components(wallet, contracts))
    }
    pub fn into_rgb_wallet(self) -> RgbWallet<W, Sp, S, C> { self.0 }
    pub fn into_components(self) -> (W, Contracts<Sp, S, C>) { self.0.into_components() }
}

impl<W, Sp, S, C> RgbRuntime<W, Sp, S, C>
where
    W: PsttConstructor + WalletProvider,
    Sp: Stockpile,
    Sp::Pile: Pile<Seal = TxoSeal>,
    S: KeyedCollection<Key = CodexId, Value = Issuer>,
    C: KeyedCollection<Key = ContractId, Value = Contract<Sp::Stock, Sp::Pile>>,
{
    /// Pay an invoice producing PSTT ready to be signed.
    ///
    /// Should not be used in multi-party protocols like coinjoins, when a PSTT may need to be
    /// modified in the number of inputs or outputs. Use the `construct_pstt` method for such
    /// scenarios.
    ///
    /// If you need more flexibility in constructing payments (do multiple payments with multiple
    /// contracts, use global state etc.) in a single PSTT, please use `pay_custom` APIs and
    /// [`PrefabBundleSet`] instead of this simplified API.
    #[allow(clippy::type_complexity)]
    pub fn pay_invoice(
        &mut self,
        invoice: &RgbInvoice<ContractId>,
        strategy: impl Coinselect,
        params: TxParams,
        giveaway: Option<Sats>,
    ) -> Result<(Pstt, Payment), MultiError<PayError, <Sp::Stock as Stock>::Error>> {
        let request = self
            .fulfill(invoice, strategy, giveaway)
            .map_err(MultiError::from_a)?;
        let script = OpRequestSet::with(request);
        let (pstt, mut payment) = self
            .transfer(script, params)
            .map_err(MultiError::from_other_a)?;
        let terminal = match invoice.auth {
            RgbBeneficiary::Token(auth) => auth,
        };
        payment.terminals.insert(terminal);
        Ok((pstt, payment))
    }

    /// Pay multiple same-contract invoices with one RGB operation and one PSTT.
    ///
    /// This is the production path for same-contract MIMO transfers: one aggregate coinselection,
    /// multiple receiver assignments, and one RGB change assignment.
    #[allow(clippy::type_complexity)]
    pub fn pay_same_contract_invoices(
        &mut self,
        invoices: &[RgbInvoice<ContractId>],
        strategy: impl Coinselect,
        params: TxParams,
        giveaway: Option<Sats>,
    ) -> Result<(Pstt, Payment), MultiError<PayError, <Sp::Stock as Stock>::Error>> {
        let (request, terminals) = self
            .fulfill_same_contract(invoices, strategy, giveaway)
            .map_err(MultiError::from_a)?;
        let script = OpRequestSet::with(request);
        let (pstt, mut payment) = self
            .transfer(script, params)
            .map_err(MultiError::from_other_a)?;
        payment.terminals.extend(terminals);
        Ok((pstt, payment))
    }

    /// Pay invoices across one or more contracts with one payment script and one PSTT.
    ///
    /// Invoices are grouped by contract. Each contract still produces a single aggregate RGB
    /// operation, preserving the same-contract MIMO invariant and avoiding independent
    /// coinselection over the same owned RGB state.
    #[allow(clippy::type_complexity)]
    pub fn pay_invoices(
        &mut self,
        invoices: &[RgbInvoice<ContractId>],
        strategy: CoinselectStrategy,
        params: TxParams,
        giveaway: Option<Sats>,
    ) -> Result<(Pstt, Payment), MultiError<PayError, <Sp::Stock as Stock>::Error>> {
        if invoices.is_empty() {
            return Err(MultiError::A(PayError::Fulfill(FulfillError::InvoiceSetEmpty)));
        }

        let mut by_contract = BTreeMap::<ContractId, Vec<RgbInvoice<ContractId>>>::new();
        for invoice in invoices {
            by_contract
                .entry(invoice.scope)
                .or_default()
                .push(invoice.clone());
        }

        let mut requests = Vec::with_capacity(by_contract.len());
        let mut terminals = BTreeSet::new();
        for contract_invoices in by_contract.into_values() {
            let (request, contract_terminals) = self
                .fulfill_same_contract(&contract_invoices, strategy, giveaway)
                .map_err(MultiError::from_a)?;
            requests.push(request);
            terminals.extend(contract_terminals);
        }

        let script = OpRequestSet::try_from_iter(requests)
            .map_err(PayError::from)
            .map_err(MultiError::from_a)?;
        let (pstt, mut payment) = self
            .transfer(script, params)
            .map_err(MultiError::from_other_a)?;
        payment.terminals.extend(terminals);
        Ok((pstt, payment))
    }

    pub fn rbf(&mut self, payment: &Payment, fee: impl Into<Sats>) -> Result<Pstt, PayError> {
        let mut pstt = payment.uncomit_pstt.clone();
        let change = payment
            .pstt_meta
            .change
            .expect("Can't RBF when no change is present");
        let old_fee = pstt.fee().expect("Invalid PSTT with zero inputs");
        let out = pstt
            .output_mut(change.vout.into_usize())
            .expect("invalid PSTT meta-information in the payment");
        out.amount -= fee.into() - old_fee;

        Ok(self.complete(pstt, &payment.bundle)?)
    }

    /// Convert invoice into a payment script.
    pub fn script(
        &mut self,
        invoice: &RgbInvoice<ContractId>,
        strategy: CoinselectStrategy,
        giveaway: Option<Sats>,
    ) -> Result<PaymentScript, PayError> {
        let request = self.fulfill(invoice, strategy, giveaway)?;
        Ok(OpRequestSet::with(request))
    }

    /// Construct transfer, consisting of PSTT and a consignment stream
    // TODO: Return a dedicated Transfer object which can stream a consignment
    #[allow(clippy::type_complexity)]
    pub fn transfer(
        &mut self,
        script: PaymentScript,
        params: TxParams,
    ) -> Result<(Pstt, Payment), MultiError<TransferError, <Sp::Stock as Stock>::Error>> {
        let payment = self.exec(script, params)?;
        let pstt = self
            .complete(payment.uncomit_pstt.clone(), &payment.bundle)
            .map_err(MultiError::A)?;
        Ok((pstt, payment))
    }

    pub fn compose_pstt(
        &mut self,
        bundle: &PaymentScript,
        params: TxParams,
    ) -> Result<(Pstt, PsttMeta), ConstructionError> {
        if bundle
            .iter()
            .flat_map(|params| &params.owned)
            .any(|assignment| matches!(&assignment.state.seal, EitherSeal::Alt(Some(_))))
        {
            return Err(ConstructionError::ExternalOutputDisabled);
        }

        let closes = bundle
            .iter()
            .flat_map(|params| &params.using)
            .map(|used| used.outpoint);

        let beneficiaries = std::iter::empty::<bpstd::pstt::Beneficiary>();
        self.wallet.construct_pstt(closes, beneficiaries, params)
    }

    /// Fill in RGB information into a pre-composed PSTT, aligning it with the provided payment
    /// script.
    ///
    /// This procedure internally calls [`RgbWallet::bundle`], ensuring all other RGB data (from
    /// other contracts) which were assigned to the UTXOs spent by this RGB, are not lost and
    /// re-assigned to the change output(s) of the PSTT.
    pub fn color_pstt(
        &mut self,
        mut pstt: Pstt,
        mut meta: PsttMeta,
        script: PaymentScript,
    ) -> Result<Payment, MultiError<TransferError, <Sp::Stock as Stock>::Error>> {
        // From this moment the transaction becomes unmodifiable
        let mut change_vout = meta.change.map(|c| c.vout);
        let request = pstt
            .rgb_resolve(script, &mut change_vout)
            .map_err(MultiError::from_a)?;
        if let Some(c) = meta.change.as_mut() {
            if let Some(vout) = change_vout {
                c.vout = vout
            }
        }

        let bundle = self
            .bundle(request, meta.change.map(|c| c.vout))
            .map_err(MultiError::from_other_a)?;

        pstt.rgb_fill_csv(&bundle).map_err(MultiError::from_a)?;

        Ok(Payment {
            uncomit_pstt: pstt,
            pstt_meta: meta,
            bundle,
            terminals: none!(),
        })
    }

    /// Execute payment script creating PSTT and prefabricated operation bundle.
    ///
    /// The returned PSTT contains only anonymous client-side validation information and is
    /// not modifiable, since it contains RGB data.
    pub fn exec(
        &mut self,
        script: PaymentScript,
        params: TxParams,
    ) -> Result<Payment, MultiError<TransferError, <Sp::Stock as Stock>::Error>> {
        let (pstt, meta) = self
            .compose_pstt(&script, params)
            .map_err(MultiError::from_a)?;
        self.color_pstt(pstt, meta, script)
    }

    /// Completes PSTT and includes the prefabricated bundle into the contract.
    pub fn complete(
        &mut self,
        mut pstt: Pstt,
        bundle: &PrefabBundle,
    ) -> Result<Pstt, TransferError> {
        let (mpc, p2mr_proof) = Self::tondi_p2mr_commit(&mut pstt)?;
        let tx = pstt.to_unsigned_tx();

        let prevouts = pstt
            .inputs()
            .map(|inp| inp.previous_outpoint)
            .collect::<Vec<_>>();
        self.include(bundle, &tx.into(), mpc, Some(p2mr_proof), &prevouts)?;

        Ok(pstt)
    }

    fn tondi_p2mr_commit(
        pstt: &mut Pstt,
    ) -> Result<(mpc::MerkleBlock, TondiP2mrProof), TransferError> {
        if pstt.are_outputs_modifiable() {
            return Err(TransferError::P2mrAnchor(
                "transaction outputs are still modifiable".to_string(),
            ));
        }

        let map = pstt.mmb_complete()?;
        let key = p2mr_original_pubkey_key();
        let output = pstt
            .outputs_mut()
            .find(|out| out.proprietary(&key).is_some())
            .ok_or_else(|| {
                TransferError::P2mrAnchor(
                    "RGB transfer requires a Tondi P2MR change/anchor output carrying the \
                     original pubkey"
                        .to_string(),
                )
            })?;
        let original_pubkey = output
            .proprietary(&key)
            .and_then(|bytes| {
                let bytes: [u8; 33] = bytes.as_slice().try_into().ok()?;
                CompressedPk::from_byte_array(bytes).ok()
            })
            .ok_or_else(|| {
                TransferError::P2mrAnchor("invalid Tondi P2MR anchor pubkey".to_string())
            })?;

        for (id, msg) in map {
            output.set_mpc_message(id, msg)?;
        }
        let (commitment, mpc_proof) = output.mpc_commit()?;
        let p2mr_proof = TondiP2mrProof::new(original_pubkey);
        output.script = ScriptPubkey::from_checked(p2mr_proof.p2mr_root(commitment)?.to_vec());

        Ok((mpc_proof, p2mr_proof))
    }

    #[allow(clippy::type_complexity)]
    fn finalize_inner(
        &mut self,
        mut pstt: Pstt,
        meta: PsttMeta,
    ) -> Result<(Tx, Option<(Vout, u32, u32)>), FinalizeError<W::Error>> {
        pstt.finalize(self.wallet.descriptor());
        let tx = pstt.extract()?;
        let change = meta.change.map(|change| {
            (change.vout, change.terminal.keychain.index(), change.terminal.index.index())
        });
        Ok((tx, change))
    }

    #[cfg(not(feature = "async"))]
    /// Finalizes PSTT, extracts the signed transaction, broadcasts it and updates wallet UTXO set
    /// accordingly.
    pub fn finalize(&mut self, pstt: Pstt, meta: PsttMeta) -> Result<(), FinalizeError<W::Error>> {
        let (tx, change) = self.finalize_inner(pstt, meta)?;
        self.wallet
            .broadcast(&tx, change)
            .map_err(FinalizeError::Broadcast)?;
        Ok(())
    }

    #[cfg(feature = "async")]
    /// Finalizes PSTT, extracts the signed transaction, broadcasts it and updates wallet UTXO set
    /// accordingly.
    pub async fn finalize_async(
        &mut self,
        pstt: Pstt,
        meta: PsttMeta,
    ) -> Result<(), FinalizeError<W::Error>> {
        let (tx, change) = self.finalize_inner(pstt, meta)?;
        self.wallet
            .broadcast_async(&tx, change)
            .await
            .map_err(FinalizeError::Broadcast)?;
        Ok(())
    }
}

#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum PayError {
    #[from]
    Fulfill(FulfillError),
    #[from]
    Transfer(TransferError),
    #[from]
    PaymentScript(confinement::Error),
}

#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum TransferError {
    #[from]
    PsttConstruct(ConstructionError),

    #[from]
    PsttRgbCsv(RgbPsttCsvError),

    #[from]
    PsttMmb(MmbPsttError),

    #[from]
    PsttMpc(MpcPsttError),

    #[from]
    PsttP2mr(TondiP2mrError),

    P2mrAnchor(String),

    #[from]
    PsttPrepare(RgbPsttPrepareError),

    #[from]
    Bundle(BundleError),

    #[from]
    Include(IncludeError),
}

#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum FinalizeError<E: core::error::Error> {
    #[from]
    UnfinalizedPstt(UnfinalizedInputs),
    Broadcast(E),
}

#[cfg(feature = "fs")]
pub mod file {
    use std::io;

    use rgb_persist_fs::StockpileDir;

    use super::*;
    use crate::{FileHolder, Owner};

    pub type RgbpRuntimeDir<R> = RgbRuntime<Owner<R, FileHolder>, StockpileDir<TxoSeal>>;

    pub trait ConsignmentStream {
        fn write(self, writer: impl io::Write) -> io::Result<()>;
    }

    pub struct Transfer<C: ConsignmentStream> {
        pub pstt: Pstt,
        pub consignment: C,
    }
}
