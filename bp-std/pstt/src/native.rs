use crate::{KeyMap, PropKey, ValueData};
use bc::{Outpoint, Sats, Tx, Txid};
use derive::Terminal;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    io::{Read, Write},
    str::FromStr,
};
use tondi_vm::{
    builder::{AuthDescriptor, DraftInput, NativeDraft},
    ClockContext, Hash32, Input as NativeInput, ProgramPackage, ResolvedInput, StateNote,
};
/// The sole supported PSTT format. Old BIP maps are not accepted.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PsttVer {
    #[serde(rename = "v4")]
    V4,
}
impl PsttVer {
    pub const fn max() -> Self {
        Self::V4
    }
}
#[derive(Clone, Debug, Display, Error)]
#[display("native v4 PSTT: {0}")]
pub struct PsttError(pub String);
impl From<tondi_vm::Error> for PsttError {
    fn from(e: tondi_vm::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<tondi_wallet_pstt::error::Error> for PsttError {
    fn from(e: tondi_wallet_pstt::error::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<std::io::Error> for PsttError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<serde_json::Error> for PsttError {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}
pub type PsttParseError = PsttError;
pub type UnsignedTx = Tx;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub index: usize,
    pub previous_outpoint: Outpoint,
    pub entry: ResolvedInput,
    pub auth: Option<AuthDescriptor>,
    #[serde(with = "tondi_vm::codec::u64_decimal")]
    pub since: u64,
    pub consume: bool,
    pub terminal: Option<Terminal>,
    #[serde(with = "crate::metadata::properties")]
    pub proprietary: IndexMap<PropKey, ValueData>,
}
impl Input {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn is_reference(&self) -> bool {
        !self.consume
    }
    pub fn prev_txout(&self) -> &StateNote {
        &self.entry.note
    }
    fn draft(&self) -> DraftInput {
        DraftInput {
            outpoint: self.previous_outpoint.into(),
            entry: self.entry.clone(),
            auth: self.auth.clone(),
            since: self.since,
            consume: self.consume,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Output {
    pub index: usize,
    pub note: StateNote,
    pub terminal: Option<Terminal>,
    #[serde(with = "crate::metadata::properties")]
    pub proprietary: IndexMap<PropKey, ValueData>,
}
impl Output {
    pub fn index(&self) -> usize {
        self.index
    }
}
macro_rules! properties {
    ($t:ty) => {
        impl KeyMap for $t {
            fn properties(&self) -> &IndexMap<PropKey, ValueData> {
                &self.proprietary
            }
            fn properties_mut(&mut self) -> &mut IndexMap<PropKey, ValueData> {
                &mut self.proprietary
            }
        }
    };
}
properties!(Input);
properties!(Output);
/// Resolved notes are signing context. A node must independently resolve live UTXOs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pstt {
    pub version: PsttVer,
    pub network: Hash32,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    pub programs: Vec<ProgramPackage>,
    payload: Vec<u8>,
    #[serde(with = "tondi_vm::codec::u64_decimal")]
    pub cycles_limit: u64,
    #[serde(with = "tondi_vm::codec::u64_decimal")]
    pub invocation_cycles_limit: u64,
    modifiable: bool,
    prepared: Option<tondi_wallet_pstt::Pstt>,
    rgb_sidecar: Option<tondi_wallet_pstt::extensions::rgb::RgbSidecar>,
}
impl Pstt {
    pub fn create(network: Hash32) -> Self {
        Self {
            version: PsttVer::V4,
            network,
            inputs: vec![],
            outputs: vec![],
            programs: vec![],
            payload: vec![],
            cycles_limit: 100_000_000,
            invocation_cycles_limit: 1_000_000,
            modifiable: true,
            prepared: None,
            rgb_sidecar: None,
        }
    }
    pub fn from_native(native: tondi_wallet_pstt::Pstt) -> Result<Self, PsttError> {
        if native.transaction.inputs.len() != native.resolved_inputs.len() {
            return Err(PsttError("resolved input count".into()));
        }
        let mut p = Self::create(native.transaction.network);
        p.inputs = native
            .transaction
            .inputs
            .iter()
            .zip(&native.resolved_inputs)
            .enumerate()
            .map(|(index, (i, e))| Input {
                index,
                previous_outpoint: i.outpoint().into(),
                entry: e.clone(),
                auth: native
                    .transaction
                    .witness
                    .auth
                    .iter()
                    .find(|a| a.input as usize == index)
                    .map(|a| AuthDescriptor {
                        leaf: a.leaf.clone(),
                        merkle_path: a.merkle_path.clone(),
                    }),
                since: i.since().unwrap_or(0),
                consume: i.consumes(),
                terminal: None,
                proprietary: IndexMap::new(),
            })
            .collect();
        p.outputs = native
            .transaction
            .outputs
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, note)| Output {
                index,
                note,
                terminal: None,
                proprietary: IndexMap::new(),
            })
            .collect();
        p.programs = native.transaction.witness.programs.clone();
        p.cycles_limit = native.transaction.cycles_limit;
        p.modifiable = false;
        p.prepared = Some(native);
        p.validate_native()?;
        Ok(p)
    }
    pub fn inputs(&self) -> impl Iterator<Item = &Input> {
        self.inputs.iter()
    }
    pub fn inputs_mut(&mut self) -> impl Iterator<Item = &mut Input> {
        self.inputs.iter_mut()
    }
    pub fn input(&self, i: usize) -> Option<&Input> {
        self.inputs.get(i)
    }
    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.outputs.iter()
    }
    pub fn outputs_mut(&mut self) -> impl Iterator<Item = &mut Output> {
        self.outputs.iter_mut()
    }
    pub fn output_mut(&mut self, i: usize) -> Option<&mut Output> {
        self.outputs.get_mut(i)
    }
    pub fn are_outputs_modifiable(&self) -> bool {
        self.modifiable
    }
    pub fn complete_construction(&mut self) {
        self.modifiable = false;
    }
    pub fn append_input(
        &mut self,
        d: DraftInput,
        terminal: Option<Terminal>,
    ) -> Result<&mut Input, PsttError> {
        if !self.modifiable || self.prepared.is_some() {
            return Err(PsttError("construction frozen".into()));
        }
        let point = Outpoint::from(d.outpoint);
        if self.inputs.iter().any(|i| i.previous_outpoint == point) {
            return Err(PsttError("duplicate input".into()));
        }
        let index = self.inputs.len();
        let auth = d.auth.or_else(|| {
            (d.consume && d.entry.note.auth == tondi_vm::public_auth().0).then(|| AuthDescriptor {
                leaf: tondi_vm::public_auth().1,
                merkle_path: vec![],
            })
        });
        self.inputs.push(Input {
            index,
            previous_outpoint: point,
            entry: d.entry,
            auth,
            since: d.since,
            consume: d.consume,
            terminal,
            proprietary: IndexMap::new(),
        });
        Ok(&mut self.inputs[index])
    }
    pub fn append_output(
        &mut self,
        note: StateNote,
        terminal: Option<Terminal>,
    ) -> Result<&mut Output, PsttError> {
        if !self.modifiable || self.prepared.is_some() {
            return Err(PsttError("construction frozen".into()));
        }
        let index = self.outputs.len();
        self.outputs.push(Output {
            index,
            note,
            terminal,
            proprietary: IndexMap::new(),
        });
        Ok(&mut self.outputs[index])
    }
    fn draft(&self) -> NativeDraft {
        NativeDraft {
            network: self.network,
            inputs: self.inputs.iter().map(Input::draft).collect(),
            outputs: self.outputs.iter().map(|o| o.note.clone()).collect(),
            programs: self.programs.clone(),
            payload: self.payload.clone(),
            cycles_limit: self.cycles_limit,
            invocation_cycles_limit: self.invocation_cycles_limit,
        }
    }
    pub fn set_rgb_sidecar(
        &mut self,
        sidecar: tondi_wallet_pstt::extensions::rgb::RgbSidecar,
    ) -> Result<(), PsttError> {
        if self.prepared.is_some() {
            return Err(PsttError("body already prepared".into()));
        }
        if sidecar.network != self.network {
            return Err(PsttError("sidecar network".into()));
        }
        sidecar.id()?;
        self.rgb_sidecar = Some(sidecar);
        Ok(())
    }
    pub fn rgb_sidecar(&self) -> Option<&tondi_wallet_pstt::extensions::rgb::RgbSidecar> {
        self.rgb_sidecar.as_ref()
    }
    pub fn prepare(&mut self) -> Result<(), PsttError> {
        if self.modifiable {
            return Err(PsttError("complete construction before preparing body".into()));
        }
        if self.prepared.is_none() {
            let p = self.draft().prepare()?;
            self.prepared = Some(tondi_wallet_pstt::Pstt {
                transaction: p.transaction,
                resolved_inputs: p.resolved_inputs,
            });
        }
        self.validate_native()
    }
    /// Checks structure and retained signing context, not chain liveness or maturity.
    pub fn validate_native(&self) -> Result<(), PsttError> {
        if self.inputs.iter().enumerate().any(|(n, i)| i.index != n)
            || self.outputs.iter().enumerate().any(|(n, o)| o.index != n)
        {
            return Err(PsttError("noncanonical resource index".into()));
        }
        if let Some(sidecar) = &self.rgb_sidecar {
            let anchors = self
                .outputs
                .iter()
                .filter_map(|o| {
                    match tondi_wallet_pstt::extensions::rgb::RgbAnchor::from_note(&o.note) {
                        Ok(Some(a)) => Some(Ok(a)),
                        Ok(None) => None,
                        Err(e) => Some(Err(e)),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            if anchors.len() != 1 {
                return Err(PsttError("sidecar requires exactly one native RGB anchor".into()));
            }
            sidecar.verify(self.network, &anchors[0])?;
        }
        let Some(p) = &self.prepared else {
            self.draft().prepare()?;
            return Ok(());
        };
        let tx = &p.transaction;
        tondi_vm::validation::validate_structure(tx, self.network, &tondi_vm::Limits::default())?;
        if self.modifiable
            || self.inputs.len() != tx.inputs.len()
            || self.outputs.len() != tx.outputs.len()
            || p.resolved_inputs.len() != self.inputs.len()
            || self.programs != tx.witness.programs
            || self.cycles_limit != tx.cycles_limit
        {
            return Err(PsttError("prepared context changed".into()));
        }
        for (index, (i, n)) in self.inputs.iter().zip(&tx.inputs).enumerate() {
            let expected = if i.consume {
                NativeInput::consume(i.previous_outpoint.into(), i.since)
            } else {
                NativeInput::reference(i.previous_outpoint.into())
            };
            let proof = tx.witness.auth.iter().find(|p| p.input as usize == index);
            let auth = match (&i.auth, proof) {
                (Some(a), Some(p)) => a.leaf == p.leaf && a.merkle_path == p.merkle_path,
                (None, None) => true,
                _ => false,
            };
            if i.index != index || n != &expected || i.entry != p.resolved_inputs[index] || !auth {
                return Err(PsttError("prepared input changed".into()));
            }
        }
        for (index, (o, n)) in self.outputs.iter().zip(&tx.outputs).enumerate() {
            if o.index != index || &o.note != n {
                return Err(PsttError("prepared output changed".into()));
            }
        }
        Ok(())
    }
    pub fn to_unsigned_tx(&self) -> Result<UnsignedTx, PsttError> {
        self.validate_native()?;
        let p = self.prepared.as_ref().ok_or_else(|| PsttError("body not prepared".into()))?;
        Ok(Tx::from_native(p.transaction.clone())?.to_unsigned_tx())
    }
    pub fn txid(&self) -> Result<Txid, PsttError> {
        Ok(self.to_unsigned_tx()?.txid())
    }
    pub fn sign(&mut self, keys: &[[u8; 32]]) -> Result<usize, PsttError> {
        self.prepare()?;
        Ok(self.prepared.as_mut().expect("prepared").sign(keys)?)
    }
    pub fn combine(&mut self, other: &Self) -> Result<(), PsttError> {
        self.validate_native()?;
        other.validate_native()?;
        let a = self.prepared.as_mut().ok_or_else(|| PsttError("body not prepared".into()))?;
        let b = other.prepared.as_ref().ok_or_else(|| PsttError("body not prepared".into()))?;
        a.combine(b)?;
        Ok(())
    }
    pub fn native(&self) -> Result<&tondi_wallet_pstt::Pstt, PsttError> {
        self.validate_native()?;
        self.prepared.as_ref().ok_or_else(|| PsttError("body not prepared".into()))
    }
    /// Requires explicit authenticated node clock. Validation never invents maturity.
    pub fn extract(&self, clock: ClockContext) -> Result<Tx, PsttError> {
        let p = self.native()?;
        p.validate(self.network, clock)?;
        Ok(Tx::from_native(p.transaction.clone())?)
    }
    pub fn input_sum(&self) -> Result<Sats, PsttError> {
        self.inputs
            .iter()
            .filter(|i| i.consume)
            .try_fold(0u64, |s, i| {
                s.checked_add(i.entry.note.value).ok_or_else(|| PsttError("input overflow".into()))
            })
            .map(Sats)
    }
    pub fn fee(&self) -> Result<Sats, PsttError> {
        let outputs = self.outputs.iter().try_fold(0u64, |s, o| {
            s.checked_add(o.note.value).ok_or_else(|| PsttError("output overflow".into()))
        })?;
        self.input_sum()?
            .0
            .checked_sub(outputs)
            .map(Sats)
            .ok_or_else(|| PsttError("outputs exceed consumed value".into()))
    }
    pub fn encode(&self, _version: PsttVer, writer: &mut impl Write) -> Result<usize, PsttError> {
        self.validate_native()?;
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(PsttError("PSTT size limit".into()));
        }
        writer.write_all(&bytes)?;
        Ok(bytes.len())
    }
    pub fn decode(reader: &mut impl Read) -> Result<Self, PsttError> {
        let mut bytes = vec![];
        reader.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(PsttError("PSTT size limit".into()));
        }
        let p: Self = serde_json::from_slice(&bytes)?;
        p.validate_native()?;
        Ok(p)
    }
}
impl fmt::Display for Pstt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&serde_json::to_string(self).map_err(|_| fmt::Error)?)
    }
}
impl FromStr for Pstt {
    type Err = PsttError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::decode(&mut s.as_bytes())
    }
}
