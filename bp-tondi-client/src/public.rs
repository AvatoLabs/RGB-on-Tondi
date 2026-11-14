//! Explicit public data and pinned, read-only dependencies for RGB applications.
//! These records never contain private allocations or consignment locations automatically.
use crate::{Error, Result, TondiClient, TondiOutpoint, UtxosByAddressEntry};
use bp::{Datum, Outpoint, SeqNo, SigScript, Tx, TxIn, TxOut, Witness};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PublicDataKind {
    Schema = 1,
    Policy = 2,
    Metadata = 3,
    StateDigest = 4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicDatum {
    pub kind: PublicDataKind,
    pub data: Datum,
}
impl PublicDatum {
    /// Wire format: RGBD | 1 | kind | public bytes, at most 4096 bytes in total.
    pub fn encode(&self) -> Result<Datum> {
        let mut bytes = b"RGBD\x01".to_vec();
        bytes.push(self.kind as u8);
        bytes.extend(self.data.as_slice());
        Datum::try_from(bytes)
            .map_err(|_| invalid("public Datum exceeds 4096 bytes including framing"))
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 6 || bytes.len() > bp::MAX_DATUM_SIZE || &bytes[..5] != b"RGBD\x01" {
            return Err(invalid("invalid public Datum framing"));
        }
        let kind = match bytes[5] {
            1 => PublicDataKind::Schema,
            2 => PublicDataKind::Policy,
            3 => PublicDataKind::Metadata,
            4 => PublicDataKind::StateDigest,
            _ => return Err(invalid("unknown public Datum kind")),
        };
        Ok(Self {
            kind,
            data: Datum::from_checked(bytes[6..].to_vec()),
        })
    }
}

/// Pins an exact public UTXO version. This is not a latest-state query or mint authority.
/// The application authenticates the pin's provenance separately: anyone can fund
/// an output to another party's script, so a destination does not prove publication authority.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicReference {
    pub outpoint: TondiOutpoint,
    pub script_version: u16,
    #[serde(with = "hex::serde")]
    pub script: Vec<u8>,
    pub datum: Datum,
}
impl PublicReference {
    pub fn from_live(entry: &UtxosByAddressEntry) -> Result<Self> {
        entry.utxo_entry.script_public_key.validate_p2mr_root()?;
        let datum = entry
            .utxo_entry
            .datum
            .clone()
            .ok_or_else(|| invalid("public reference has no Datum"))?;
        PublicDatum::decode(datum.as_slice())?;
        Ok(Self {
            outpoint: entry.outpoint,
            script_version: entry.utxo_entry.script_public_key.version,
            script: entry.utxo_entry.script_public_key.script.clone(),
            datum,
        })
    }
    /// Authenticate a node's current UTXO response against the application-selected pin.
    pub fn authenticate(&self, entry: &UtxosByAddressEntry) -> Result<TxOut> {
        PublicDatum::decode(self.datum.as_slice())?;
        if entry.outpoint != self.outpoint
            || entry.utxo_entry.script_public_key.version != self.script_version
            || entry.utxo_entry.script_public_key.script != self.script
            || entry.utxo_entry.datum.as_ref() != Some(&self.datum)
        {
            return Err(invalid("public reference does not match live UTXO"));
        }
        entry.utxo_entry.script_public_key.validate_p2mr_root()?;
        let mut prevout = TxOut::with_script_version(
            self.script_version,
            bp::ScriptPubkey::from_checked(self.script.clone()),
            entry.utxo_entry.amount,
        );
        prevout.datum = Some(self.datum.clone());
        Ok(prevout)
    }
    /// Verify retained context data against its creation transaction and an anchor.
    /// The caller separately verifies that the anchor was accepted by the native chain.
    pub fn verify_witness(&self, creation: &Tx, anchor: &Tx) -> Result<()> {
        PublicDatum::decode(self.datum.as_slice())?;
        crate::ScriptPublicKey {
            version: self.script_version,
            script: self.script.clone(),
        }
        .validate_p2mr_root()?;
        if creation.txid().to_string() != self.outpoint.transaction_id.to_string() {
            return Err(invalid("context creation transaction ID mismatch"));
        }
        let output = creation
            .outputs
            .get(self.outpoint.index as usize)
            .ok_or_else(|| invalid("missing context creation output"))?;
        if output.script_pubkey_version != self.script_version
            || output.script_pubkey.as_slice() != self.script.as_slice()
            || output.datum.as_ref() != Some(&self.datum)
        {
            return Err(invalid("context creation output mismatch"));
        }
        let point: Outpoint = self.outpoint.into();
        if !anchor
            .inputs
            .iter()
            .any(|i| i.is_reference && i.prev_output == point)
            || anchor
                .inputs
                .iter()
                .any(|i| !i.is_reference && i.prev_output == point)
        {
            return Err(invalid("anchor does not reference the pinned context"));
        }
        Ok(())
    }
    /// Append before sealing/signing. The returned prevout belongs at the same input index.
    /// The caller obtains `entry` through its trusted Tondi node, not from a counterparty.
    pub fn append_to(&self, tx: &mut Tx, entry: &UtxosByAddressEntry) -> Result<TxOut> {
        let prevout = self.authenticate(entry)?;
        if tx.version != bp::TxVer::CURRENT
            || tx
                .inputs
                .iter()
                .any(|i| !i.sig_script.is_empty() || !i.witness.is_empty())
        {
            return Err(invalid("references must be selected before signing"));
        }
        let outpoint: Outpoint = self.outpoint.into();
        if tx.inputs.iter().any(|i| i.prev_output == outpoint) {
            return Err(invalid("duplicate reference or reference/spend overlap"));
        }
        if tx.inputs.iter().filter(|i| i.is_reference).count() >= 64 {
            return Err(invalid("more than 64 reference inputs"));
        }
        tx.inputs
            .push(TxIn {
                prev_output: outpoint,
                is_reference: true,
                sig_op_count: 0,
                sig_script: SigScript::new(),
                sequence: SeqNo::from_consensus_u32(0),
                witness: Witness::new(),
            })
            .map_err(|_| invalid("too many inputs"))?;
        Ok(prevout)
    }
    /// Address is a lookup hint; all returned fields must still match the exact pin.
    #[cfg(feature = "async")]
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
            .ok_or_else(|| invalid("referenced UTXO is no longer live"))?;
        self.authenticate(&entry)?;
        Ok(entry)
    }
}
fn invalid(message: &str) -> Error {
    Error::InvalidNativeTransaction(message.into())
}
