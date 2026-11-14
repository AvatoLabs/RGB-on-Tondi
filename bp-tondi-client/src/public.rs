//! Exact native StateNote pins and read-only dependencies. Provenance is an application obligation.
use crate::{Error, Result, TondiClient, TondiOutpoint, UtxosByAddressEntry};
use tondi_vm::{
    builder::{DraftInput, NativeDraft},
    AuthRoot, LawInstance, ProgramRef, ResolvedInput, StateNote, StatePayload, LAW_DATA,
};
const PUBLIC_ARGS: &[u8] = b"Tondi/v4/RGB/public";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PublicDataKind {
    Schema = 1,
    Law = 2,
    Metadata = 3,
    StateDigest = 4,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicDatum {
    pub kind: PublicDataKind,
    pub data: Vec<u8>,
}
impl PublicDatum {
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.data.len() > 4095 {
            return Err(invalid("public datum exceeds capacity"));
        }
        let mut b = vec![self.kind as u8];
        b.extend_from_slice(&self.data);
        Ok(b)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > 4096 {
            return Err(invalid("invalid public datum length"));
        }
        let kind = match bytes[0] {
            1 => PublicDataKind::Schema,
            2 => PublicDataKind::Law,
            3 => PublicDataKind::Metadata,
            4 => PublicDataKind::StateDigest,
            _ => return Err(invalid("invalid public datum kind")),
        };
        Ok(Self {
            kind,
            data: bytes[1..].to_vec(),
        })
    }
    pub fn note(&self, value: u64, auth: AuthRoot) -> Result<StateNote> {
        let mut note = StateNote::plain(value, auth);
        note.state = Some(StatePayload {
            law: LawInstance {
                program: ProgramRef::Standard(LAW_DATA),
                args: PUBLIC_ARGS.to_vec(),
            },
            datum: self.encode()?,
        });
        Ok(note)
    }
    pub fn from_note(note: &StateNote) -> Result<Self> {
        let state = note
            .state
            .as_ref()
            .ok_or_else(|| invalid("missing public state"))?;
        if state.law.program != ProgramRef::Standard(LAW_DATA) || state.law.args != PUBLIC_ARGS {
            return Err(invalid("invalid public state domain"));
        }
        Self::decode(&state.datum)
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicReference {
    pub outpoint: TondiOutpoint,
    pub note: StateNote,
}
impl PublicReference {
    pub fn from_live(entry: &UtxosByAddressEntry) -> Result<Self> {
        PublicDatum::from_note(&entry.utxo_entry.note)?;
        Ok(Self {
            outpoint: entry.outpoint,
            note: entry.utxo_entry.note.clone(),
        })
    }
    pub fn authenticate(&self, entry: &UtxosByAddressEntry) -> Result<ResolvedInput> {
        PublicDatum::from_note(&self.note)?;
        if self.outpoint != entry.outpoint || self.note != entry.utxo_entry.note {
            return Err(invalid("public reference differs from live UTXO"));
        }
        Ok(entry.utxo_entry.clone())
    }
    /// Accepted-chain membership and live status must be checked through the node independently.
    pub fn verify_witness(&self, creation: &bp::Tx, anchor: &bp::Tx) -> Result<()> {
        PublicDatum::from_note(&self.note)?;
        let point: tondi_vm::OutPoint = self.outpoint.into();
        if creation.id() != point.transaction_id
            || creation.network != anchor.network
            || creation.outputs.get(point.index as usize) != Some(&self.note)
        {
            return Err(invalid("public reference creation mismatch"));
        }
        if !anchor
            .inputs
            .iter()
            .any(|i| !i.consumes() && i.outpoint() == point)
            || anchor
                .inputs
                .iter()
                .any(|i| i.consumes() && i.outpoint() == point)
        {
            return Err(invalid("anchor must reference the exact context"));
        }
        Ok(())
    }
    /// Add before preparing the body. Draft binding automatically includes the read dependency.
    pub fn append_to(&self, draft: &mut NativeDraft, entry: &UtxosByAddressEntry) -> Result<()> {
        let entry = self.authenticate(entry)?;
        let outpoint = self.outpoint.into();
        if draft.inputs.iter().any(|i| i.outpoint == outpoint) {
            return Err(invalid("duplicate consume/reference dependency"));
        }
        draft.inputs.push(DraftInput {
            outpoint,
            entry,
            auth: None,
            since: 0,
            consume: false,
        });
        Ok(())
    }
    pub async fn resolve(
        &self,
        client: &TondiClient,
        address: &str,
    ) -> Result<UtxosByAddressEntry> {
        let entry = client
            .get_utxos_by_address(address)
            .await?
            .into_iter()
            .find(|e| e.outpoint == self.outpoint)
            .ok_or_else(|| invalid("public reference is no longer live"))?;
        self.authenticate(&entry)?;
        Ok(entry)
    }
}
fn invalid(message: &str) -> Error {
    Error::InvalidNativeTransaction(message.into())
}
