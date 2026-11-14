//! Native v4 PSTT construction and RGB metadata. The node wallet crate owns signing and VM validation.
#[macro_use]
extern crate amplify;
mod native;
mod metadata;
pub mod constructor;
#[cfg(feature = "client-side-validation")]
mod csval;
pub use constructor::{
    Beneficiary, BeneficiaryParseError, ChangeInfo, ConstructionError, Payment, PsttConstructor,
    PsttMeta, TxParams, Utxo,
};
#[cfg(feature = "client-side-validation")]
pub use csval::*;
pub use metadata::*;
pub use native::*;
