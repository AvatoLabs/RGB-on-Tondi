//! RPC4 resolver: complete native Notes and witnesses, accepted-chain status and live references.
use super::{Resolver, ResolverError};
use crate::tondi_address::encode_p2mr_for_address_network;
use amplify::ByteArray;
use bpstd::pstt::Utxo;
use bpstd::{AddressNetwork, ScriptPubkey, Terminal, Tx, Txid, UnsignedTx};
use core::num::NonZeroU64;
use rgb::{Outpoint, WitnessStatus};
use tondi::{Error as TondiError, NetworkType, TondiClient, TondiConfig, TondiOutpoint};
pub struct TondiResolver {
    endpoint: String,
    network: AddressNetwork,
    network_type: NetworkType,
    min_blue_score_depth: u64,
}
impl TondiResolver {
    pub async fn new(endpoint: &str) -> Result<Self, ResolverError> {
        Ok(Self {
            endpoint: endpoint.into(),
            network: AddressNetwork::TondiMainnet,
            network_type: NetworkType::Mainnet,
            min_blue_score_depth: 10,
        })
    }
    pub async fn mainnet(e: &str) -> Result<Self, ResolverError> {
        Self::new(e).await
    }
    pub async fn testnet(e: &str) -> Result<Self, ResolverError> {
        Ok(Self::new(e)
            .await?
            .with_network(AddressNetwork::TondiTestnet))
    }
    pub fn with_min_blue_score_depth(mut self, depth: u64) -> Self {
        self.min_blue_score_depth = depth;
        self
    }
    pub fn with_network(mut self, network: AddressNetwork) -> Self {
        self.network = network;
        self.network_type = match network {
            AddressNetwork::TondiMainnet => NetworkType::Mainnet,
            AddressNetwork::TondiTestnet => NetworkType::Testnet,
            AddressNetwork::TondiSimnet => NetworkType::Simnet,
            AddressNetwork::TondiDevnet => NetworkType::Devnet,
            _ => self.network_type,
        };
        self
    }
    pub fn network(&self) -> AddressNetwork {
        self.network
    }
    // CLI calls may use separate Tokio runtimes. Close each connection before its owning runtime ends.
    async fn query<T>(
        &self,
        operation: impl AsyncFnOnce(&TondiClient) -> Result<T, ResolverError>,
    ) -> Result<T, ResolverError> {
        let client = TondiClient::connect(
            TondiConfig::new(self.endpoint.clone()).with_network(self.network_type),
        )
        .await
        .map_err(ResolverError::from)?;
        let result = operation(&client).await;
        let closed = client.disconnect().await.map_err(ResolverError::from);
        match result {
            Err(e) => Err(e),
            Ok(v) => {
                closed?;
                Ok(v)
            }
        }
    }
    pub async fn clock(&self) -> Result<bpstd::native::ClockContext, ResolverError> {
        self.query(async |c| c.clock().await.map_err(ResolverError::from))
            .await
    }
    pub async fn get_full_tx(&self, id: Txid) -> Result<Option<Tx>, ResolverError> {
        self.query(async |c| {
            match c
                .get_transaction(amplify::Bytes32::from_byte_array(id.to_byte_array()))
                .await
            {
                Ok(t) => {
                    let tx = t.to_bp_tx().map_err(ResolverError::from)?;
                    if tx.txid() != id {
                        return Err(ResolverError::Protocol);
                    }
                    Ok(Some(tx))
                }
                Err(TondiError::TransactionNotFound(_)) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await
    }
    pub async fn check_utxo(&self, outpoint: Outpoint) -> Result<Option<Utxo>, ResolverError> {
        let Some(tx) = self.get_full_tx(outpoint.txid).await? else {
            return Ok(None);
        };
        let Some(note) = tx.outputs.get(outpoint.vout.to_u32() as usize) else {
            return Ok(None);
        };
        let address =
            encode_p2mr_for_address_network(self.network, &note.auth.merkle_root.as_bytes())
                .map_err(|_| ResolverError::Protocol)?;
        let point = TondiOutpoint {
            transaction_id: amplify::Bytes32::from_byte_array(outpoint.txid.to_byte_array()),
            index: outpoint.vout.to_u32(),
        };
        self.query(async |c| {
            let live = c
                .get_utxos_by_address(&address)
                .await
                .map_err(ResolverError::from)?
                .into_iter()
                .find(|u| u.outpoint == point);
            let Some(live) = live else { return Ok(None) };
            if &live.utxo_entry.note != note {
                return Err(ResolverError::Protocol);
            }
            Ok(Some(Utxo {
                outpoint,
                entry: live.utxo_entry,
                terminal: Terminal::new(0u8, 0u16.into()),
            }))
        })
        .await
    }
}
fn convert_status(
    accepted: bool,
    in_mempool: bool,
    removed: bool,
    score: Option<u64>,
    deep: bool,
) -> WitnessStatus {
    if removed {
        return WitnessStatus::Archived;
    }
    if !accepted {
        return if in_mempool { WitnessStatus::Tentative } else { WitnessStatus::Archived };
    }
    if !deep {
        return WitnessStatus::Tentative;
    }
    match score {
        Some(0) => WitnessStatus::Genesis,
        Some(s) => WitnessStatus::Mined(NonZeroU64::new(s).expect("nonzero")),
        None => WitnessStatus::Archived,
    }
}
impl Resolver for TondiResolver {
    async fn resolve_tx_async(&self, id: Txid) -> Result<Option<UnsignedTx>, ResolverError> {
        self.get_full_tx(id).await
    }
    async fn resolve_tx_status_async(&self, id: Txid) -> Result<WitnessStatus, ResolverError> {
        self.query(async |c| {
            let s = c
                .get_transaction_status(amplify::Bytes32::from_byte_array(id.to_byte_array()))
                .await
                .map_err(ResolverError::from)?;
            let d = c.get_block_dag_info().await.map_err(ResolverError::from)?;
            let deep = s.accepting_block_daa_score.is_some_and(|birth| {
                d.virtual_daa_score
                    .checked_sub(birth)
                    .is_some_and(|depth| depth >= self.min_blue_score_depth)
            });
            Ok(convert_status(
                s.accepted,
                s.in_mempool,
                s.removed,
                s.accepting_block_daa_score,
                deep,
            ))
        })
        .await
    }
    async fn resolve_utxos_async(
        &self,
        iter: impl IntoIterator<Item = (Terminal, ScriptPubkey)>,
    ) -> impl Iterator<Item = Result<Utxo, ResolverError>> {
        // Resolve the batch within one connection, including all descriptor terminals.
        let items = iter.into_iter().collect::<Vec<_>>();
        match self
            .query(async |c| {
                let mut result = vec![];
                for (terminal, root) in items {
                    let address =
                        match encode_p2mr_for_address_network(self.network, root.as_slice()) {
                            Ok(a) => a,
                            Err(_) => {
                                result.push(Err(ResolverError::Protocol));
                                continue;
                            }
                        };
                    match c.get_utxos_by_address(&address).await {
                        Ok(entries) => {
                            for e in entries {
                                if e.utxo_entry.note.auth.merkle_root.as_bytes().as_slice()
                                    != root.as_slice()
                                {
                                    result.push(Err(ResolverError::Protocol));
                                    continue;
                                }
                                result.push(Ok(Utxo {
                                    outpoint: e.outpoint.into(),
                                    entry: e.utxo_entry,
                                    terminal,
                                }));
                            }
                        }
                        Err(e) => result.push(Err(e.into())),
                    }
                }
                Ok(result)
            })
            .await
        {
            Ok(result) => result.into_iter(),
            Err(e) => vec![Err(e)].into_iter(),
        }
    }
    async fn check_utxo_async(&self, p: Outpoint) -> Result<Option<Utxo>, ResolverError> {
        self.check_utxo(p).await
    }
    async fn last_block_height_async(&self) -> Result<u64, ResolverError> {
        self.query(async |c| {
            Ok(c.get_block_dag_info()
                .await
                .map_err(ResolverError::from)?
                .virtual_daa_score)
        })
        .await
    }
    async fn broadcast_async(&self, tx: &Tx) -> Result<(), ResolverError> {
        self.query(async |c| {
            let id = c
                .submit_transaction(
                    tondi::TondiTransaction::from_bp_tx(tx).map_err(ResolverError::from)?,
                    false,
                )
                .await
                .map_err(ResolverError::from)?;
            if id.to_byte_array() != tx.txid().to_byte_array() {
                return Err(ResolverError::Protocol);
            }
            Ok(())
        })
        .await
    }
}
impl From<TondiError> for ResolverError {
    fn from(err: TondiError) -> Self {
        match err {
            TondiError::Connection(_) | TondiError::Timeout => ResolverError::Connectivity,
            TondiError::TransactionNotFound(_) | TondiError::BlockNotFound(_) => {
                ResolverError::Logic
            }
            TondiError::InvalidServerResponse | TondiError::Serialization(_) => {
                ResolverError::Protocol
            }
            TondiError::NodeNotSynced | TondiError::UtxoIndexNotEnabled => ResolverError::Logic,
            TondiError::RpcCall(message) | TondiError::InvalidNativeTransaction(message) => {
                ResolverError::ServerSide(message)
            }
            _ => ResolverError::Connectivity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_requires_acceptance_and_depth() {
        assert_eq!(convert_status(false, true, false, None, false), WitnessStatus::Tentative);
        assert_eq!(convert_status(false, false, false, None, false), WitnessStatus::Archived);
        assert_eq!(convert_status(true, false, true, Some(42), true), WitnessStatus::Archived);
        assert_eq!(convert_status(true, false, false, Some(42), false), WitnessStatus::Tentative);
        assert_eq!(
            convert_status(true, false, false, Some(42), true),
            WitnessStatus::Mined(NonZeroU64::new(42).unwrap())
        );
    }
}
