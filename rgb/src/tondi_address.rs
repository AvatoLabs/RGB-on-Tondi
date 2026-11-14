use bpstd::native::{AuthRoot, Hash32, AUTH_P2MR};
#[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
use bpstd::AddressNetwork;
use bpstd::Network;
fn address(network: Network, root: &[u8]) -> Result<String, String> {
    let bytes: [u8; 32] = root
        .try_into()
        .map_err(|_| "Auth root must be 32 bytes".to_string())?;
    let auth = AuthRoot { version: AUTH_P2MR, merkle_root: Hash32::from_bytes(bytes) };
    let network_type = match network {
        Network::Mainnet => tondi_consensus_core::network::NetworkType::Mainnet,
        Network::Testnet => tondi_consensus_core::network::NetworkType::Testnet,
        Network::Simnet => tondi_consensus_core::network::NetworkType::Simnet,
        Network::Devnet => tondi_consensus_core::network::NetworkType::Devnet,
    };
    bpstd::native::address_for_auth(&auth, network_type.into())
        .map(|a| a.to_string())
        .map_err(|e| e.to_string())
}
pub(crate) fn encode_p2mr_for_network(network: Network, root: &[u8]) -> Result<String, String> {
    address(network, root)
}
#[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
pub(crate) fn encode_p2mr_for_address_network(
    network: AddressNetwork,
    root: &[u8],
) -> Result<String, String> {
    let n = match network {
        AddressNetwork::TondiMainnet => Network::Mainnet,
        AddressNetwork::TondiTestnet => Network::Testnet,
        AddressNetwork::TondiSimnet => Network::Simnet,
        AddressNetwork::TondiDevnet => Network::Devnet,
        _ => return Err("Tondi network required".into()),
    };
    address(n, root)
}
