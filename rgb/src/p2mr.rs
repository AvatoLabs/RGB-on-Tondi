use bpstd::pstt::PropKey;

pub const TONDI_RGB_PSTT_PREFIX: &str = "TONDI_RGB";
pub const TONDI_RGB_OUT_P2MR_ORIGINAL_PUBKEY: u64 = 0x00;

pub fn p2mr_original_pubkey_key() -> PropKey {
    PropKey {
        identifier: TONDI_RGB_PSTT_PREFIX.to_owned(),
        subtype: TONDI_RGB_OUT_P2MR_ORIGINAL_PUBKEY,
        data: Default::default(),
    }
}
