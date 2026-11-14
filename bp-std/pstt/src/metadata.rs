use derive::ByteStr;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
pub type KeyData = ByteStr;
#[derive(
    Wrapper,
    Clone,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    Default,
    Debug,
    Display,
    From,
    Serialize,
    Deserialize
)]
#[wrapper(Deref, Index, RangeOps, AsSlice, BorrowSlice, Hex)]
#[display(LowerHex)]
#[serde(transparent)]
pub struct ValueData(ByteStr);
impl From<Vec<u8>> for ValueData {
    fn from(v: Vec<u8>) -> Self {
        ByteStr::from(v).into()
    }
}
#[derive(Clone, PartialOrd, Ord, Eq, PartialEq, Hash, Debug, Display, Serialize, Deserialize)]
#[display("{identifier} {subtype:#x} {data:#x}")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropKey {
    pub identifier: String,
    pub subtype: u64,
    pub data: KeyData,
}
#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
#[display("proprietary key '{0}' is already present")]
pub struct KeyAlreadyPresent(pub PropKey);
pub trait KeyMap {
    fn properties(&self) -> &IndexMap<PropKey, ValueData>;
    fn properties_mut(&mut self) -> &mut IndexMap<PropKey, ValueData>;
    fn proprietary(&self, key: &PropKey) -> Option<&ValueData> {
        self.properties().get(key)
    }
    fn has_proprietary(&self, key: &PropKey) -> bool {
        self.proprietary(key).is_some()
    }
    fn push_proprietary(
        &mut self,
        key: PropKey,
        value: impl Into<ValueData>,
    ) -> Result<bool, KeyAlreadyPresent> {
        let value = value.into();
        if let Some(v) = self.proprietary(&key) {
            return if v == &value { Ok(false) } else { Err(KeyAlreadyPresent(key)) };
        }
        self.properties_mut().insert(key, value);
        Ok(true)
    }
    fn remove_proprietary(&mut self, key: &PropKey) -> Option<ValueData> {
        self.properties_mut().shift_remove(key)
    }
}
// A sequence keeps binary proprietary keys lossless in JSON and rejects duplicates.
pub(crate) mod properties {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        v: &IndexMap<PropKey, ValueData>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        v.iter().collect::<Vec<_>>().serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<IndexMap<PropKey, ValueData>, D::Error> {
        let pairs = Vec::<(PropKey, ValueData)>::deserialize(d)?;
        let mut map = IndexMap::new();
        for (k, v) in pairs {
            if map.insert(k, v).is_some() {
                return Err(serde::de::Error::custom("duplicate proprietary key"));
            }
        }
        Ok(map)
    }
}
