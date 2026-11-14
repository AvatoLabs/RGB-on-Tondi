//! Native execution weight. There is no witness discount or legacy mass field.
use crate::Tx;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceUsage {
    pub serialized_bytes: u64,
    pub cycles: u64,
}
impl ResourceUsage {
    pub fn weight(self) -> tondi_vm::Result<u64> {
        tondi_vm::resources::transaction_weight(self.serialized_bytes, self.cycles, 1, 1000)
    }
    pub fn fee_quote(self, rate_per_1000: u64) -> tondi_vm::Result<u64> {
        tondi_vm::resources::fee_quote(self.weight()?, rate_per_1000)
    }
}
impl Tx {
    pub fn resource_usage(&self, cycles: u64) -> ResourceUsage {
        ResourceUsage {
            serialized_bytes: self.wire_bytes().len() as u64,
            cycles,
        }
    }
}
