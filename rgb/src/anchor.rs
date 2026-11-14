//! Off-chain marker selecting the StateNote that will carry the native RGB anchor.
use bpstd::pstt::PropKey;
pub fn state_anchor_host_key() -> PropKey {
    PropKey {
        identifier: "Tondi/v4/RGB".into(),
        subtype: 1,
        data: Default::default(),
    }
}
