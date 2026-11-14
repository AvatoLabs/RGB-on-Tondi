//! RGB multi-message and multi-protocol metadata for native v4 PSTTs.
mod mmb;
mod mpc;
pub use mmb::{MmbPsttError, PSTT_IN_MMB_MESSAGE, PSTT_MMB_PREFIX};
pub use mpc::{
    MpcPsttError, PSTT_MPC_PREFIX, PSTT_OUT_MPC_COMMITMENT, PSTT_OUT_MPC_ENTROPY,
    PSTT_OUT_MPC_MESSAGE, PSTT_OUT_MPC_MIN_TREE_DEPTH, PSTT_OUT_MPC_PROOF,
};
