//! Native fields persisted in PSTT proprietary maps. RGB state is not stored here.
use crate::{Input, Output, PropKey, PsttError};
use derive::{Datum, TxOut, TONDI_P2MR_SCRIPT_VERSION};
fn key(subtype: u64) -> PropKey {
    PropKey {
        identifier: "TONDI_NATIVE".into(),
        subtype,
        data: Default::default(),
    }
}
impl Input {
    pub fn set_native_input(&mut self, is_reference: bool, sig_op_count: u8) {
        assert!(!is_reference || sig_op_count == 0, "reference cannot carry signature operations");
        self.proprietary.insert(key(0), vec![u8::from(is_reference), sig_op_count].into());
    }
    pub fn native_input(&self) -> Result<(bool, u8), PsttError> {
        match self.proprietary.get(&key(0)).map(|v| v.as_slice()) {
            Some([0, count]) => Ok((false, *count)),
            Some([1, 0]) => Ok((true, 0)),
            _ => Err(PsttError::InvalidTondiFields),
        }
    }
    pub fn is_reference(&self) -> bool {
        self.native_input().expect("validated native PSTT input").0
    }
    pub fn sig_op_count(&self) -> u8 {
        self.native_input().expect("validated native PSTT input").1
    }
}
impl Output {
    pub fn set_native_output(&mut self, version: u16, datum: Option<&Datum>) {
        let mut bytes = version.to_le_bytes().to_vec();
        bytes.push(u8::from(datum.is_some()));
        if let Some(datum) = datum {
            bytes.extend((datum.len() as u32).to_le_bytes());
            bytes.extend(datum.as_slice());
        }
        self.proprietary.insert(key(1), bytes.into());
    }
    pub fn native_output(&self) -> Result<(u16, Option<Datum>), PsttError> {
        let bytes = self.proprietary.get(&key(1)).ok_or(PsttError::InvalidTondiFields)?;
        let b = bytes.as_slice();
        if b.len() < 3 {
            return Err(PsttError::InvalidTondiFields);
        }
        let version = u16::from_le_bytes([b[0], b[1]]);
        let datum = match b[2] {
            0 if b.len() == 3 => None,
            1 if b.len() >= 7 => {
                let len = u32::from_le_bytes(b[3..7].try_into().unwrap()) as usize;
                if len > derive::MAX_DATUM_SIZE || b.len() != 7 + len {
                    return Err(PsttError::InvalidTondiFields);
                }
                Some(Datum::from_checked(b[7..].to_vec()))
            }
            _ => return Err(PsttError::InvalidTondiFields),
        };
        Ok((version, datum))
    }
    pub fn set_datum(&mut self, datum: Option<&Datum>) -> Result<(), PsttError> {
        let (version, _) = self.native_output()?;
        self.set_native_output(version, datum);
        Ok(())
    }
    pub(crate) fn init_native_output(&mut self) {
        self.set_native_output(TONDI_P2MR_SCRIPT_VERSION, None);
    }
    pub(crate) fn native_txout(&self) -> TxOut {
        let (version, datum) = self.native_output().expect("validated native PSTT output");
        let mut out = TxOut::with_script_version(version, self.script.clone(), self.amount);
        out.datum = datum;
        out
    }
}
impl crate::Pstt {
    pub fn validate_native(&self) -> Result<(), PsttError> {
        if self.tx_version != derive::TxVer::CURRENT {
            return Err(PsttError::InvalidTondiFields);
        }
        let mut points = std::collections::HashSet::new();
        let mut refs = 0;
        for input in self.inputs() {
            let (reference, _) = input.native_input()?;
            if !points.insert(input.previous_outpoint) {
                return Err(PsttError::InvalidTondiFields);
            }
            if reference {
                refs += 1;
                if input.sequence_number.is_some_and(|s| s.to_consensus_u32() != 0)
                    || input.final_script_sig.is_some()
                    || input.final_witness.is_some()
                    || input.witness_utxo.is_none()
                {
                    return Err(PsttError::InvalidTondiFields);
                }
            }
        }
        if refs > 64 {
            return Err(PsttError::InvalidTondiFields);
        }
        for output in self.outputs() {
            output.native_output()?;
        }
        Ok(())
    }
}
