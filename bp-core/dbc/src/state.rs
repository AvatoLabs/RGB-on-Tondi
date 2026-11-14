//! Native StatePayload commitments, independent of the output's Auth tree.
use crate::LIB_NAME_BPCORE;
use amplify::{ByteArray, Bytes32};
use bc::Tx;
use commit_verify::mpc::Commitment;
use tondi_wallet_pstt::extensions::rgb::RgbAnchor;

/// Exact location and sidecar commitment of the single RGB anchor in a native transaction.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib=LIB_NAME_BPCORE)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "camelCase", deny_unknown_fields)
)]
pub struct StateAnchorProof {
    /// Index of the StateNote carrying the RGB anchor.
    pub output: u32,
    /// Network-bound sidecar identifier committed in that Note's datum.
    pub sidecar_id: Bytes32,
}
impl StateAnchorProof {
    /// Check exact output, unique anchor, version, MPC root and sidecar commitment.
    /// Chain acceptance and RGB operation semantics are verified separately.
    pub fn verify(&self, commitment: &Commitment, tx: &Tx) -> Result<(), StateAnchorError> {
        let mut anchors = Vec::new();
        for (index, note) in tx.outputs().enumerate() {
            if let Some(anchor) =
                RgbAnchor::from_note(note).map_err(|_| StateAnchorError::Encoding)?
            {
                anchors.push((index, anchor));
            }
        }
        if anchors.len() != 1 {
            return Err(StateAnchorError::AnchorCount);
        }
        let (index, anchor) = &anchors[0];
        if *index != self.output as usize
            || anchor.commitment.as_bytes() != commitment.to_byte_array()
            || anchor.sidecar_id.as_bytes() != self.sidecar_id.to_byte_array()
        {
            return Err(StateAnchorError::CommitmentMismatch);
        }
        Ok(())
    }
}
/// Reasons a published native RGB anchor is invalid.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Display, Error)]
#[display(doc_comments)]
pub enum StateAnchorError {
    /// native RGB anchor encoding or version is invalid.
    Encoding,
    /// a native RGB transaction must contain exactly one recognised state anchor.
    AnchorCount,
    /// anchor output, MPC commitment or sidecar identifier differs from the proof.
    CommitmentMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc::native::{public_auth, Hash32};
    use strict_encoding::StrictDumb;

    fn anchored() -> (Tx, StateAnchorProof, Commitment) {
        let commitment = Commitment::from_byte_array([7; 32]);
        let anchor = RgbAnchor::new(Hash32::from_bytes([7; 32]), Hash32::from_bytes([8; 32]));
        let mut native = Tx::strict_dumb().into_native();
        native.body.outputs = vec![anchor.note(1_000_000, public_auth().0).unwrap()];
        let proof = StateAnchorProof {
            output: 0,
            sidecar_id: Bytes32::from_byte_array([8; 32]),
        };
        (Tx::from_native(native).unwrap(), proof, commitment)
    }

    #[test]
    fn exact_state_anchor_is_independent_of_auth() {
        let (tx, proof, commitment) = anchored();
        proof.verify(&commitment, &tx).unwrap();
        let mut native = tx.into_native();
        native.body.outputs[0].auth = bc::native::schnorr_auth([3; 32]).0;
        proof.verify(&commitment, &Tx::from_native(native).unwrap()).unwrap();
    }

    #[test]
    fn duplicate_wrong_index_sidecar_and_version_fail() {
        let (tx, mut proof, commitment) = anchored();
        proof.output = 1;
        assert!(proof.verify(&commitment, &tx).is_err());
        proof.output = 0;
        proof.sidecar_id = Bytes32::from_byte_array([9; 32]);
        assert!(proof.verify(&commitment, &tx).is_err());
        let (tx, proof, _) = anchored();
        let mut native = tx.clone().into_native();
        native.body.outputs.push(native.body.outputs[0].clone());
        assert!(proof.verify(&commitment, &Tx::from_native(native).unwrap()).is_err());
        let mut native = tx.into_native();
        native.body.outputs[0].state.as_mut().unwrap().datum[0] = 2;
        assert!(proof.verify(&commitment, &Tx::from_native(native).unwrap()).is_err());
    }
}
