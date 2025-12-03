// Modern, minimalistic & standard-compliant cold wallet library.
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2020-2024 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2020-2024 LNP/BP Standards Association. All rights reserved.
// Copyright (C) 2020-2024 Dr Maxim Orlovsky. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::str::FromStr;

use bc::BlockHash;

use crate::AddressNetwork;

/// Tondi network used by the address
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Display)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase",)
)]
#[display(lowercase)]
pub enum Network {
    /// Tondi mainnet
    #[display("tondi")]
    Mainnet,

    /// Tondi testnet (Frontier)
    #[display("tondi-testnet")]
    Testnet,

    /// Tondi simnet (for simulation)
    #[display("tondi-simnet")]
    Simnet,

    /// Tondi devnet (for development)
    #[display("tondi-devnet")]
    Devnet,
}

impl Network {
    /// Detects whether the network is a kind of test network (testnet, simnet,
    /// devnet).
    pub fn is_testnet(self) -> bool { self != Self::Mainnet }

    /// Returns the genesis block hash for the network.
    pub const fn genesis_hash(self) -> BlockHash {
        match self {
            Network::Mainnet => BlockHash::GENESIS_TONDI_MAINNET,
            Network::Testnet => BlockHash::GENESIS_TONDI_TESTNET,
            Network::Simnet => BlockHash::GENESIS_TONDI_SIMNET,
            Network::Devnet => BlockHash::GENESIS_TONDI_DEVNET,
        }
    }
}

impl From<Network> for AddressNetwork {
    fn from(network: Network) -> Self {
        match network {
            Network::Mainnet => AddressNetwork::TondiMainnet,
            Network::Testnet => AddressNetwork::TondiTestnet,
            Network::Simnet => AddressNetwork::TondiSimnet,
            Network::Devnet => AddressNetwork::TondiDevnet,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display, Error)]
#[display("unknown genesis block hash '{0}'")]
pub struct UnknownGenesisBlock(pub BlockHash);

impl TryFrom<BlockHash> for Network {
    type Error = UnknownGenesisBlock;

    fn try_from(hash: BlockHash) -> Result<Self, Self::Error> {
        match hash {
            BlockHash::GENESIS_TONDI_MAINNET => Ok(Network::Mainnet),
            BlockHash::GENESIS_TONDI_TESTNET => Ok(Network::Testnet),
            BlockHash::GENESIS_TONDI_SIMNET => Ok(Network::Simnet),
            BlockHash::GENESIS_TONDI_DEVNET => Ok(Network::Devnet),
            _ => Err(UnknownGenesisBlock(hash)),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Display, Error)]
#[display("unknown Tondi network '{0}'")]
pub struct UnknownNetwork(pub String);

impl FromStr for Network {
    type Err = UnknownNetwork;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "tondi" | "mainnet" => Network::Mainnet,
            "tondi-testnet" | "testnet" => Network::Testnet,
            "tondi-simnet" | "simnet" => Network::Simnet,
            "tondi-devnet" | "devnet" => Network::Devnet,
            other => return Err(UnknownNetwork(other.to_owned())),
        })
    }
}
