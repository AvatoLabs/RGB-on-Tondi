//! Native block identifiers. Full BlockDAG records use the official RPC4 types.
use crate::LIB_NAME_TONDI;
use amplify::{Bytes32, Wrapper};
#[derive(Wrapper, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, From)]
#[wrapper(BorrowSlice, Index, RangeOps, Debug, Hex, Display, FromStr)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib=LIB_NAME_TONDI)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(transparent))]
pub struct BlockHash(
    #[from]
    #[from([u8;32])]
    Bytes32,
);
