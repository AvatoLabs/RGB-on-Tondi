// Wallet Library for RGB smart contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Copyright (C) 2025 AvatoLabs.
//
// Licensed under the Apache License, Version 2.0.

use serde::{Deserialize, Serialize};

pub const HTTP_CONTRACT_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    Json(&'static str),
    NullableJson(&'static str),
    NotFoundNullJson(&'static str),
    AddressString,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    pub method: HttpMethod,
    pub path: &'static str,
    pub request: Option<&'static str>,
    pub response: ResponseKind,
}

pub const HTTP_ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        method: HttpMethod::Get,
        path: "/wallet/profile",
        request: None,
        response: ResponseKind::NotFoundNullJson("WalletProfile"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/wallets",
        request: Some("CreateWalletInput"),
        response: ResponseKind::Json("WalletProfile"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/wallet/lock",
        request: None,
        response: ResponseKind::Empty,
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/wallet/unlock",
        request: Some("UnlockWalletInput"),
        response: ResponseKind::NullableJson("WalletProfile"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/wallet/sync",
        request: None,
        response: ResponseKind::Json("DashboardSnapshot"),
    },
    Endpoint {
        method: HttpMethod::Get,
        path: "/dashboard",
        request: None,
        response: ResponseKind::Json("DashboardSnapshot"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/wallet/p2mr-address",
        request: None,
        response: ResponseKind::AddressString,
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/issuers/import",
        request: Some("ImportIssuerInput"),
        response: ResponseKind::Empty,
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/contracts/issue",
        request: Some("IssueContractInput"),
        response: ResponseKind::Json("RgbAsset"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/contracts/accept",
        request: Some("AcceptContractInput"),
        response: ResponseKind::Json("RgbAsset"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/invoices",
        request: Some("CreateInvoiceInput"),
        response: ResponseKind::Json("RgbInvoice"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/transfers/pay",
        request: Some("PayInvoiceInput"),
        response: ResponseKind::Json("RgbTransfer"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/transfers/pay-batch",
        request: Some("PayInvoicesInput"),
        response: ResponseKind::Json("RgbTransfer"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/transfers/accept",
        request: Some("AcceptTransferInput"),
        response: ResponseKind::Json("RgbTransfer"),
    },
    Endpoint {
        method: HttpMethod::Post,
        path: "/transfers/finalise",
        request: Some("FinaliseTransferInput"),
        response: ResponseKind::Json("RgbTransfer"),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TondiNetwork {
    Mainnet,
    Testnet,
    Testnet2,
    Devnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationState {
    NotStarted,
    Pending,
    Valid,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferState {
    InvoiceCreated,
    PsttPrepared,
    ConsignmentReady,
    ReceiverAccepted,
    Finalised,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WalletLifecycle {
    NotCreated,
    Locked,
    Ready,
    Syncing,
    ServiceRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum P2mrOutputState {
    Available,
    Anchoring,
    Reserved,
    Spent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DashboardServiceMode {
    Connected,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletProfile {
    pub wallet_id: String,
    pub network: TondiNetwork,
    pub tondi_rpc_endpoint: String,
    pub p2mr_address: String,
    pub lifecycle: WalletLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2mrOutput {
    pub seal: String,
    pub outpoint: String,
    pub value_tondi: String,
    pub state: P2mrOutputState,
    pub bound_contract_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbAsset {
    pub contract_id: String,
    pub ticker: String,
    pub name: String,
    pub precision: u8,
    pub balance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_supply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_consignment_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub issuer: String,
    pub validation: ValidationState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_transition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbInvoice {
    pub invoice: String,
    pub contract_id: String,
    pub api_name: String,
    pub method_name: String,
    pub state_name: String,
    pub amount: String,
    pub receiver_p2mr_seal: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbTransferContract {
    pub contract_id: String,
    pub amount: String,
    pub consignment_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbTransfer {
    pub id: String,
    pub contract_transfers: Vec<RgbTransferContract>,
    pub invoices: Vec<String>,
    pub pstt_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broadcast_txid: Option<String>,
    pub state: TransferState,
    pub validation: ValidationState,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub profile: WalletProfile,
    pub chain_balance_tondi: String,
    pub p2mr_outputs: Vec<P2mrOutput>,
    pub assets: Vec<RgbAsset>,
    pub invoices: Vec<RgbInvoice>,
    pub transfers: Vec<RgbTransfer>,
    pub service_mode: DashboardServiceMode,
    pub service_notice: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWalletInput {
    pub wallet_id: String,
    pub network: TondiNetwork,
    pub tondi_rpc_endpoint: String,
    pub passphrase: String,
    pub recovery_phrase: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockWalletInput {
    pub passphrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportIssuerInput {
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueContractInput {
    pub issuer_file_name: String,
    pub ticker: String,
    pub name: String,
    pub precision: u8,
    pub issued_supply: String,
    pub issue_seal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptContractInput {
    pub consignment_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInvoiceInput {
    pub contract_id: String,
    pub api_name: String,
    pub method_name: String,
    pub state_name: String,
    pub amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayInvoiceInput {
    pub invoice: String,
    pub fee_tondi: String,
    pub carrier_value_tondi: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayInvoicesInput {
    pub invoices: Vec<String>,
    pub fee_tondi: String,
    pub carrier_value_tondi: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptTransferInput {
    pub consignment_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinaliseTransferInput {
    pub pstt_ref: String,
    pub broadcast: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2mrAddressResponse {
    pub address: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn contract_exposes_frontend_endpoint_count() {
        assert_eq!(HTTP_CONTRACT_VERSION, 1);
        assert_eq!(HTTP_ENDPOINTS.len(), 15);
        assert_eq!(HTTP_ENDPOINTS[0].method.as_str(), "GET");
        assert_eq!(HTTP_ENDPOINTS[0].path, "/wallet/profile");
        assert_eq!(HTTP_ENDPOINTS[0].response, ResponseKind::NotFoundNullJson("WalletProfile"));
    }

    #[test]
    fn dashboard_sample_matches_wire_shape() {
        let dashboard: DashboardSnapshot = serde_json::from_value(json!({
            "profile": {
                "walletId": "primary",
                "network": "testnet",
                "tondiRpcEndpoint": "127.0.0.1:16210",
                "p2mrAddress": "tondi0:qr6n3-example-p2mr-address",
                "lifecycle": "ready"
            },
            "chainBalanceTondi": "12.50000000",
            "p2mrOutputs": [{
                "seal": "testnet:p2mr-seal:seal_0001",
                "outpoint": "11b1ffefeebb11932b5455b9b020c5d02c7fc3366282361dbf9702fa02e5363d:1",
                "valueTondi": "12.50000000",
                "state": "reserved",
                "boundContractIds": ["contract:rgb_sample"]
            }],
            "assets": [{
                "contractId": "contract:rgb_sample",
                "ticker": "RGB",
                "name": "Avato RGB Asset",
                "precision": 0,
                "balance": "1000000",
                "issuedSupply": "1000000",
                "contractConsignmentRef": "contracts/contract_rgb_sample.rgb",
                "issuer": "issuer.yaml",
                "validation": "valid",
                "lastTransition": "genesis"
            }],
            "invoices": [{
                "invoice": "rgb-on-tondi:contract:rgb_sample:amount:25:invoice_sample",
                "contractId": "contract:rgb_sample",
                "apiName": "Test",
                "methodName": "transfer",
                "stateName": "amount",
                "amount": "25",
                "receiverP2mrSeal": "testnet:p2mr-seal:seal_0001",
                "createdAt": "2026-07-01T00:00:00.000Z"
            }],
            "transfers": [{
                "id": "transfer_sample",
                "contractTransfers": [{
                    "contractId": "contract:rgb_sample",
                    "amount": "25",
                    "consignmentRef": "transfer_sample.rgb"
                }],
                "invoices": ["rgb-on-tondi:contract:rgb_sample:amount:25:invoice_sample"],
                "psttRef": "pstt_sample.pstt",
                "broadcastTxid": "23d1f754fb3fc9b4362ea83a30aa5977ec040613ae79a4791c2aa8185886ba19",
                "state": "broadcast",
                "validation": "valid",
                "updatedAt": "2026-07-01T00:00:00.000Z"
            }],
            "serviceMode": "connected",
            "serviceNotice": "Connected to RGB-on-Tondi service."
        }))
        .expect("dashboard sample must match the HTTP wire contract");

        assert_eq!(dashboard.profile.network, TondiNetwork::Testnet);
        assert_eq!(dashboard.service_mode, DashboardServiceMode::Connected);
        assert_eq!(dashboard.transfers[0].state, TransferState::Broadcast);
    }

    #[test]
    fn backend_service_mode_excludes_dev_preview() {
        let payload = json!({
            "profile": {
                "walletId": "primary",
                "network": "devnet",
                "tondiRpcEndpoint": "127.0.0.1:26310",
                "p2mrAddress": "tondidev:example",
                "lifecycle": "ready"
            },
            "chainBalanceTondi": "0",
            "p2mrOutputs": [],
            "assets": [],
            "invoices": [],
            "transfers": [],
            "serviceMode": "dev-preview",
            "serviceNotice": "bad"
        });

        assert!(serde_json::from_value::<DashboardSnapshot>(payload).is_err());
    }

    #[test]
    fn finalise_transfer_rejects_raw_signer_material() {
        let raw_signer_material_field = ["tondi", "P2mr", "Secret", "Keys"].join("");
        let mut payload = json!({
            "psttRef": "transfer.pstt",
            "broadcast": true,
        });
        payload
            .as_object_mut()
            .expect("object payload")
            .insert(raw_signer_material_field, json!("not-accepted"));

        assert!(serde_json::from_value::<FinaliseTransferInput>(payload).is_err());
    }
}
