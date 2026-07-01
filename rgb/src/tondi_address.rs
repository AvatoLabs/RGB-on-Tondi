#[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
use bpstd::AddressNetwork;
use bpstd::Network;

const P2MR_VERSION: u8 = 216;
const BECH32M_CONST: u32 = 0x2bc830a3;
const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const GENERATORS: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

pub(crate) fn encode_p2mr_for_network(network: Network, root: &[u8]) -> Result<String, String> {
    let prefix = match network {
        Network::Mainnet => "tondi",
        Network::Testnet => "tondi0",
        Network::Simnet => "tondisim",
        Network::Devnet => "tondidev",
    };
    encode_p2mr(prefix, root)
}

#[cfg(any(feature = "resolver-tondi", feature = "resolver-tondi-async"))]
pub(crate) fn encode_p2mr_for_address_network(
    network: AddressNetwork,
    root: &[u8],
) -> Result<String, String> {
    encode_p2mr(network.bech32_hrp(), root)
}

fn encode_p2mr(prefix: &str, root: &[u8]) -> Result<String, String> {
    if root.len() != 32 || root.iter().all(|byte| *byte == 0) {
        return Err(format!(
            "invalid P2MR root for address: expected non-zero 32 bytes, got {} bytes",
            root.len()
        ));
    }

    let mut bytes = Vec::with_capacity(33);
    bytes.push(P2MR_VERSION);
    bytes.extend_from_slice(root);
    let data = bytes_to_five_bit(&bytes)?;
    let checksum = create_checksum(prefix, &data)?;
    let payload = data
        .iter()
        .chain(checksum.iter())
        .map(|value| CHARSET[*value as usize] as char)
        .collect::<String>();

    Ok(format!("{prefix}:{payload}"))
}

fn create_checksum(prefix: &str, data: &[u8]) -> Result<[u8; 6], String> {
    let mut values = bytes_to_five_bit(prefix.as_bytes())?;
    values.extend_from_slice(data);
    values.extend_from_slice(&[0u8; 6]);

    let residue = polymod(values.into_iter()) ^ BECH32M_CONST;
    let mut checksum = [0u8; 6];
    for (index, value) in checksum.iter_mut().enumerate() {
        *value = ((residue >> (5 * (5 - index))) & 0x1f) as u8;
    }
    Ok(checksum)
}

fn bytes_to_five_bit(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut values = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    let max_value = 0x1f;

    for byte in bytes {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            values.push(((accumulator >> bits) & max_value) as u8);
        }
    }

    if bits > 0 {
        values.push(((accumulator << (5 - bits)) & max_value) as u8);
    }

    Ok(values)
}

fn polymod(values: impl IntoIterator<Item = u8>) -> u32 {
    let mut checksum = 1u32;
    for value in values {
        let top = checksum >> 25;
        checksum = ((checksum & 0x1ffffff) << 5) ^ u32::from(value);
        for (index, generator) in GENERATORS.iter().enumerate() {
            if ((top >> index) & 1) != 0 {
                checksum ^= generator;
            }
        }
    }
    checksum
}
