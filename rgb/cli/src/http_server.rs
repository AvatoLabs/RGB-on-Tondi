// Wallet Library for RGB smart contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Copyright (C) 2025 AvatoLabs.
//
// Licensed under the Apache License, Version 2.0.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::convert::Infallible;
use std::fmt;
use std::fs::{self, File};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use binfile::BinFile;
use bpstd::pstt::{Input as PsttInput, KeyMap, PropKey, PsttConstructor, TxParams};
use bpstd::seals::{mpc, TondiP2mrProof, TxoSeal};
use bpstd::{
    h, CompressedPk, ConsensusDecode, ConsensusEncode, IdxBase, Network, Outpoint, Pstt, PsttVer,
    Sats, Terminal, Tx, Txid, Vout, XprivAccount, XpubDerivable,
};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::{SecondsFormat, Utc};
use getrandom::fill as fill_random;
use pbkdf2::pbkdf2_hmac;
use rgb::invoice::{RgbBeneficiary, RgbInvoice as CoreRgbInvoice};
use rgb::popls::bp::{RgbWallet, WalletProvider};
use rgb::{
    parse_consignment, Assignment, AuthToken, CallScope, Consensus, ContractId, ContractRef,
    Contracts, CreateParams, Issuer, WitnessStatus, CONSIGN_MAGIC_NUMBER, CONSIGN_VERSION,
};
use rgb_persist_fs::StockpileDir;
use rgbp::descriptors::RgbDescr;
use rgbp::resolvers::{MultiResolver, Resolver};
use rgbp::{
    p2mr_original_pubkey_key, CoinselectStrategy, ContractInfo, FileHolder, Owner, OwnerProvider,
    Payment, RgbpRuntimeDir, UtxoSet,
};
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use strict_encoding::{StreamReader, StrictReader, TypeName, VariantName};
use strict_types::StrictVal;

use crate::exec::sign_tondi_p2mr_inputs;
use crate::http_contract::{
    AcceptContractInput, AcceptTransferInput, CreateInvoiceInput, CreateWalletInput,
    DashboardServiceMode, DashboardSnapshot, FinaliseTransferInput, ImportIssuerInput,
    IssueContractInput, P2mrAddressResponse, P2mrOutput, P2mrOutputState, PayInvoiceInput,
    PayInvoicesInput, RgbAsset, RgbInvoice as HttpRgbInvoice, RgbTransfer, RgbTransferContract,
    TondiNetwork, TransferState, UnlockWalletInput, ValidationState, WalletLifecycle,
    WalletProfile,
};

const PROFILE_FILE: &str = "http-wallet-profile.json";
const ASSET_METADATA_DIR: &str = "asset-metadata";
const BIP39_SEED_ROUNDS: u32 = 2048;
const PASSWORD_VERIFIER_ROUNDS: u32 = 100_000;
const RGB_AMOUNT_STATE: &str = "amount";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpServerConfig {
    pub data_dir: PathBuf,
    pub network: Network,
    pub min_confirmations: u32,
    pub tondi_rpc_endpoint: String,
}

#[derive(Debug)]
struct AppState {
    config: HttpServerConfig,
    profile_lock: Mutex<()>,
    wallet_state_lock: Mutex<()>,
    signer_session: Mutex<Option<UnlockedSigner>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAssetMetadata {
    contract_id: String,
    ticker: String,
    name: String,
    precision: u8,
    issued_supply: Option<String>,
    contract_consignment_ref: Option<String>,
}

pub async fn serve(bind: SocketAddr, config: HttpServerConfig) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("Serving RGB-on-Tondi HTTP boundary at http://{}", listener.local_addr()?);
    axum::serve(listener, router(config)).await?;
    Ok(())
}

pub fn router(config: HttpServerConfig) -> Router {
    let state = Arc::new(AppState::new(config));

    Router::new()
        .route("/wallet/profile", get(wallet_profile).options(cors_preflight))
        .route("/wallets", post(create_wallet).options(cors_preflight))
        .route("/wallet/lock", post(lock_wallet).options(cors_preflight))
        .route("/wallet/unlock", post(unlock_wallet).options(cors_preflight))
        .route("/wallet/sync", post(sync_wallet).options(cors_preflight))
        .route("/dashboard", get(dashboard).options(cors_preflight))
        .route("/wallet/p2mr-address", post(p2mr_address).options(cors_preflight))
        .route("/issuers/import", post(import_issuer).options(cors_preflight))
        .route("/contracts/issue", post(issue_contract).options(cors_preflight))
        .route("/contracts/accept", post(accept_contract).options(cors_preflight))
        .route("/invoices", post(create_invoice).options(cors_preflight))
        .route("/transfers/pay", post(pay_invoice).options(cors_preflight))
        .route("/transfers/pay-batch", post(pay_invoices).options(cors_preflight))
        .route("/transfers/accept", post(accept_transfer).options(cors_preflight))
        .route("/transfers/finalise", post(finalise_transfer).options(cors_preflight))
        .with_state(state)
        .layer(middleware::from_fn(cors_headers))
}

async fn wallet_profile(
    State(state): State<Arc<AppState>>,
) -> Result<Json<WalletProfile>, ApiError> {
    let profile = run_blocking(move || {
        let profile = state.load_profile()?;
        state.wallet_profile(&profile)
    })
    .await?;
    Ok(Json(profile))
}

async fn create_wallet(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateWalletInput>,
) -> Result<Json<WalletProfile>, ApiError> {
    let profile = run_blocking(move || state.create_wallet(input)).await?;
    Ok(Json(profile))
}

async fn lock_wallet(State(state): State<Arc<AppState>>) -> Result<StatusCode, ApiError> {
    run_blocking(move || state.lock_wallet()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unlock_wallet(
    State(state): State<Arc<AppState>>,
    Json(input): Json<UnlockWalletInput>,
) -> Result<Json<Option<WalletProfile>>, ApiError> {
    let profile = run_blocking(move || state.unlock_wallet(&input.passphrase)).await?;
    Ok(Json(profile))
}

async fn dashboard(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DashboardSnapshot>, ApiError> {
    run_blocking(move || dashboard_snapshot_cached(state))
        .await
        .map(Json)
}

async fn sync_wallet(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DashboardSnapshot>, ApiError> {
    let snapshot = tokio::task::spawn_blocking(move || state.dashboard_snapshot_synced())
        .await
        .map_err(|err| ApiError::internal(format!("wallet sync worker failed: {err}")))??;
    Ok(Json(snapshot))
}

async fn p2mr_address(
    State(state): State<Arc<AppState>>,
) -> Result<Json<P2mrAddressResponse>, ApiError> {
    let address = run_blocking(move || state.p2mr_address()).await?;
    Ok(Json(P2mrAddressResponse { address }))
}

async fn import_issuer(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ImportIssuerInput>,
) -> Result<StatusCode, ApiError> {
    run_blocking(move || state.import_issuer(input)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn issue_contract(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IssueContractInput>,
) -> Result<Json<RgbAsset>, ApiError> {
    let asset = run_blocking(move || state.issue_contract(input)).await?;
    Ok(Json(asset))
}

async fn accept_contract(
    State(state): State<Arc<AppState>>,
    Json(input): Json<AcceptContractInput>,
) -> Result<Json<RgbAsset>, ApiError> {
    let asset = run_blocking(move || state.accept_contract(input)).await?;
    Ok(Json(asset))
}

async fn create_invoice(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateInvoiceInput>,
) -> Result<Json<HttpRgbInvoice>, ApiError> {
    let invoice = run_blocking(move || state.create_invoice(input)).await?;
    Ok(Json(invoice))
}

async fn pay_invoice(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PayInvoiceInput>,
) -> Result<Json<RgbTransfer>, ApiError> {
    let transfer = run_blocking(move || state.pay_invoice(input)).await?;
    Ok(Json(transfer))
}

async fn pay_invoices(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PayInvoicesInput>,
) -> Result<Json<RgbTransfer>, ApiError> {
    let transfer = run_blocking(move || state.pay_invoices(input)).await?;
    Ok(Json(transfer))
}

async fn accept_transfer(
    State(state): State<Arc<AppState>>,
    Json(input): Json<AcceptTransferInput>,
) -> Result<Json<RgbTransfer>, ApiError> {
    let transfer = run_blocking(move || state.accept_transfer(input)).await?;
    Ok(Json(transfer))
}

async fn finalise_transfer(
    State(state): State<Arc<AppState>>,
    Json(input): Json<FinaliseTransferInput>,
) -> Result<Json<RgbTransfer>, ApiError> {
    let transfer = run_blocking(move || state.finalise_transfer(input)).await?;
    Ok(Json(transfer))
}

async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| ApiError::internal(format!("blocking worker failed: {err}")))?
}

fn dashboard_snapshot_cached(state: Arc<AppState>) -> Result<DashboardSnapshot, ApiError> {
    let profile = match state.load_profile() {
        Ok(profile) => profile,
        Err(err) if err.status == StatusCode::NOT_FOUND => {
            return Ok(state.unavailable_dashboard(
                WalletLifecycle::NotCreated,
                "No RGB-on-Tondi wallet profile is configured for the HTTP service yet.",
            ));
        }
        Err(err) => return Err(err),
    };

    if profile.locked {
        return Ok(state.unavailable_dashboard(
            WalletLifecycle::Locked,
            "The RGB-on-Tondi wallet profile is locked.",
        ));
    }

    let mut runtime = state.load_runtime_cached(&profile)?;
    state.dashboard_from_runtime(&profile, &mut runtime)
}

async fn cors_preflight() -> impl IntoResponse { StatusCode::NO_CONTENT }

async fn cors_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    add_cors_headers(response.headers_mut());
    response
}

fn add_cors_headers(headers: &mut HeaderMap) {
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET,POST,OPTIONS"));
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type,authorization"),
    );
}

impl AppState {
    fn new(config: HttpServerConfig) -> Self {
        Self {
            config,
            profile_lock: Mutex::new(()),
            wallet_state_lock: Mutex::new(()),
            signer_session: Mutex::new(None),
        }
    }

    fn profile_path(&self) -> PathBuf { self.config.data_dir.join(PROFILE_FILE) }

    fn wallet_dir(&self, wallet_id: &str) -> PathBuf {
        self.config
            .data_dir
            .join(wallet_id)
            .with_extension("wallet")
    }

    fn create_wallet(&self, input: CreateWalletInput) -> Result<WalletProfile, ApiError> {
        let network = network_from_contract(input.network);
        if network != self.config.network {
            return Err(ApiError::bad_request(format!(
                "wallet network '{}' does not match service network '{}'",
                network_label(network),
                network_label(self.config.network)
            )));
        }

        validate_wallet_id(&input.wallet_id)?;
        validate_recovery_phrase(&input.recovery_phrase)?;
        validate_passphrase(&input.passphrase)?;
        if input.tondi_rpc_endpoint.trim().is_empty() {
            return Err(ApiError::bad_request("Tondi RPC endpoint must not be empty"));
        }

        let profile = {
            let _guard = self
                .profile_lock
                .lock()
                .map_err(|_| ApiError::internal("wallet profile lock is poisoned"))?;
            fs::create_dir_all(&self.config.data_dir).map_err(|err| {
                ApiError::internal(format!("unable to create RGB data directory: {err}"))
            })?;

            let seed = bip39_seed(&input.recovery_phrase)?;
            let account = account_from_seed(network, &seed);
            let descriptor =
                RgbDescr::p2mr_unfunded(XpubDerivable::from(account.to_xpub_account()), [0u8; 32]);
            let wallet_dir = self.wallet_dir(&input.wallet_id);
            FileHolder::create(wallet_dir.clone(), descriptor).map_err(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    ApiError::conflict(format!(
                        "wallet '{}' already exists at '{}'",
                        input.wallet_id,
                        wallet_dir.display()
                    ))
                } else {
                    ApiError::internal(format!(
                        "unable to create wallet '{}' at '{}': {err}",
                        input.wallet_id,
                        wallet_dir.display()
                    ))
                }
            })?;

            let mut profile = StoredProfile {
                wallet_id: input.wallet_id,
                network,
                tondi_rpc_endpoint: input.tondi_rpc_endpoint,
                p2mr_address: String::new(),
                password: PasswordVerifier::new(&input.passphrase)?,
                encrypted_seed: EncryptedSeed::new(&seed, &input.passphrase)?,
                locked: false,
            };
            profile.p2mr_address = self.next_p2mr_address(&profile)?;
            self.save_profile_unlocked(&profile)?;
            self.store_unlocked_signer(&profile, seed)?;
            profile
        };

        self.wallet_profile(&profile)
    }

    fn lock_wallet(&self) -> Result<(), ApiError> {
        let _guard = self
            .profile_lock
            .lock()
            .map_err(|_| ApiError::internal("wallet profile lock is poisoned"))?;
        let mut profile = self.load_profile_unlocked()?;
        profile.locked = true;
        self.clear_unlocked_signer()?;
        self.save_profile_unlocked(&profile)
    }

    fn unlock_wallet(&self, passphrase: &str) -> Result<Option<WalletProfile>, ApiError> {
        let profile = {
            let _guard = self
                .profile_lock
                .lock()
                .map_err(|_| ApiError::internal("wallet profile lock is poisoned"))?;
            let mut profile = self.load_profile_unlocked()?;
            if !profile.password.verify(passphrase) {
                return Ok(None);
            }
            let seed = profile.encrypted_seed.decrypt(passphrase)?;
            self.store_unlocked_signer(&profile, seed)?;
            profile.locked = false;
            self.save_profile_unlocked(&profile)?;
            profile
        };
        self.wallet_profile(&profile).map(Some)
    }

    fn p2mr_address(&self) -> Result<String, ApiError> {
        let _guard = self
            .profile_lock
            .lock()
            .map_err(|_| ApiError::internal("wallet profile lock is poisoned"))?;
        let mut profile = self.load_profile_unlocked()?;
        if profile.locked {
            return Err(ApiError::locked());
        }
        let address = self.next_p2mr_address(&profile)?;
        profile.p2mr_address = address.clone();
        self.save_profile_unlocked(&profile)?;
        Ok(address)
    }

    fn import_issuer(&self, input: ImportIssuerInput) -> Result<(), ApiError> {
        let path = self.resolve_input_file(&input.file_name)?;
        let mut contracts = self.load_contracts()?;
        let issuer =
            Issuer::load(&path, |_, _, _| Result::<_, Infallible>::Ok(())).map_err(|err| {
                ApiError::bad_request(format!(
                    "unable to load RGB issuer '{}': {err}",
                    path.display()
                ))
            })?;
        contracts
            .import_issuer(issuer)
            .map_err(|err| ApiError::internal(format!("unable to import RGB issuer: {err}")))?;
        Ok(())
    }

    fn issue_contract(&self, input: IssueContractInput) -> Result<RgbAsset, ApiError> {
        let issuer_path = self.resolve_input_file(&input.issuer_file_name)?;
        let ticker = parse_asset_ticker(&input.ticker)?;
        let name = parse_asset_name(&input.name)?;
        let issued_supply = parse_u64_amount(&input.issued_supply, "issuedSupply")?;
        if issued_supply == 0 {
            return Err(ApiError::bad_request("issuedSupply must be greater than zero"));
        }
        let issue_outpoint = parse_p2mr_issue_seal(&input.issue_seal)?;
        let contract_type = TypeName::from_str(&ticker).map_err(|err| {
            ApiError::bad_request(format!(
                "ticker must be a strict RGB type name such as TEST or RGB20: {err}"
            ))
        })?;

        let profile = self.load_profile()?;
        if profile.locked {
            return Err(ApiError::locked());
        }

        let issuer = Issuer::load(&issuer_path, |_, _, _| Result::<_, Infallible>::Ok(()))
            .map_err(|err| {
                ApiError::bad_request(format!(
                    "unable to load RGB issuer '{}': {err}",
                    issuer_path.display()
                ))
            })?;
        let codex_id = issuer.codex_id();
        let mut contracts = self.load_contracts()?;
        if !contracts.has_issuer(codex_id) {
            contracts.import_issuer(issuer).map_err(|err| {
                ApiError::internal(format!("unable to import RGB issuer for issue: {err}"))
            })?;
        }

        let mut runtime = self.load_runtime_cached(&profile)?;
        if !runtime
            .wallet
            .utxos()
            .any(|outpoint| outpoint == issue_outpoint)
        {
            return Err(ApiError::bad_request(format!(
                "issueSeal '{}' is not a live P2MR output in wallet '{}'; sync and select an \
                 available P2MR output",
                input.issue_seal, profile.wallet_id
            )));
        }

        let mut params = if profile.network.is_testnet() {
            CreateParams::new_tondi_testnet(codex_id, contract_type)
        } else {
            CreateParams::new_tondi_mainnet(codex_id, contract_type)
        };
        params.push_owned_unlocked(
            VariantName::from(RGB_AMOUNT_STATE),
            Assignment::new_internal(issue_outpoint, StrictVal::num(issued_supply)),
        );

        let contract_id = runtime
            .issue(params)
            .map_err(|err| ApiError::bad_request(format!("unable to issue RGB contract: {err}")))?;
        let consignment_path = self.contract_consignment_path(contract_id);
        if let Some(parent) = consignment_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                ApiError::internal(format!(
                    "unable to create contract consignment directory '{}': {err}",
                    parent.display()
                ))
            })?;
        }
        runtime
            .contracts
            .export_to_file(&consignment_path, contract_id)
            .map_err(|err| {
                ApiError::internal(format!(
                    "unable to export RGB contract consignment '{}': {err}",
                    consignment_path.display()
                ))
            })?;

        let mut asset = rgb_asset_from_runtime(&runtime, contract_id);
        asset.ticker = ticker;
        asset.name = name;
        asset.precision = input.precision;
        asset.issued_supply = Some(issued_supply.to_string());
        asset.contract_consignment_ref = Some(self.file_ref(&consignment_path));
        self.save_asset_metadata(&asset)?;
        Ok(asset)
    }

    fn accept_contract(&self, input: AcceptContractInput) -> Result<RgbAsset, ApiError> {
        let path = self.resolve_input_file(&input.consignment_name)?;
        let profile = self.load_profile()?;
        if profile.locked {
            return Err(ApiError::locked());
        }
        let mut runtime = self.load_runtime_cached(&profile)?;
        let before = runtime.contracts.contract_ids().collect::<HashSet<_>>();
        runtime
            .consume_from_file(true, &path, |_, _, _| Result::<_, Infallible>::Ok(()))
            .map_err(|err| {
                ApiError::bad_request(format!(
                    "unable to accept RGB contract consignment '{}': {err}",
                    path.display()
                ))
            })?;
        let contract_id = runtime
            .contracts
            .contract_ids()
            .find(|contract_id| !before.contains(contract_id))
            .ok_or_else(|| {
                ApiError::bad_request(format!(
                    "consignment '{}' did not add a new RGB contract",
                    path.display()
                ))
            })?;
        let mut asset = rgb_asset_from_runtime(&runtime, contract_id);
        asset.contract_consignment_ref = Some(self.file_ref(&path));
        self.save_asset_metadata(&asset)?;
        Ok(asset)
    }

    fn create_invoice(&self, input: CreateInvoiceInput) -> Result<HttpRgbInvoice, ApiError> {
        let amount = parse_u64_amount(&input.amount, "amount")?;
        if amount == 0 {
            return Err(ApiError::bad_request("amount must be greater than zero"));
        }
        let api_name = parse_api_name(&input.api_name)?;
        let method_name = parse_method_name(&input.method_name)?;
        let state_name = parse_state_name(&input.state_name)?;
        let contract_ref = ContractRef::from_str(input.contract_id.trim()).map_err(|err| {
            ApiError::bad_request(format!("invalid RGB contract reference: {err}"))
        })?;

        let profile = self.load_profile()?;
        if profile.locked {
            return Err(ApiError::locked());
        }
        let mut runtime = self.load_runtime_cached(&profile)?;
        let contract_id = runtime
            .contracts
            .find_contract_id(contract_ref)
            .ok_or_else(|| {
                ApiError::bad_request(format!(
                    "unknown RGB contract '{}'; import or issue it before invoicing",
                    input.contract_id
                ))
            })?;
        let auth = runtime.auth_token(None).ok_or_else(|| {
            ApiError::bad_request(
                "wallet has no unspent P2MR outputs; fund and sync a Tondi P2MR output first",
            )
        })?;
        let beneficiary = RgbBeneficiary::Token(auth);
        let receiver_p2mr_seal = beneficiary.to_string();
        let consensus = consensus_for(profile.network);
        let api_name_text = api_name.to_string();
        let method_name_text = method_name.to_string();
        let state_name_text = state_name.to_string();
        let invoice = CoreRgbInvoice::new(
            CallScope::<rgb::invoice::ContractQuery>::ContractId(contract_id),
            consensus,
            profile.network.is_testnet(),
            beneficiary,
            Some(StrictVal::num(amount)),
        )
        .use_api(api_name)
        .use_method(method_name)
        .use_state(state_name);

        Ok(HttpRgbInvoice {
            invoice: invoice.to_string(),
            contract_id: contract_id.to_string(),
            api_name: api_name_text,
            method_name: method_name_text,
            state_name: state_name_text,
            amount: amount.to_string(),
            receiver_p2mr_seal,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        })
    }

    fn pay_invoice(&self, input: PayInvoiceInput) -> Result<RgbTransfer, ApiError> {
        let fee = parse_tondi_amount(&input.fee_tondi, "feeTondi")?;
        if fee.is_zero() {
            return Err(ApiError::bad_request("feeTondi must be greater than zero"));
        }
        let carrier_value = parse_tondi_amount(&input.carrier_value_tondi, "carrierValueTondi")?;
        let giveaway = carrier_value.is_non_zero().then_some(carrier_value);
        let invoice_text = input.invoice.trim().to_owned();
        let invoice = parse_rgb_invoice(&invoice_text)?;
        let contract_id = invoice.scope;
        let contract_amounts = invoice_amounts_by_contract(std::slice::from_ref(&invoice))?;
        let contract_terminals = invoice_terminals_by_contract(std::slice::from_ref(&invoice));

        let profile = self.load_profile()?;
        if profile.locked {
            return Err(ApiError::locked());
        }
        let _wallet_guard = self.wallet_state_guard("payment preparation")?;
        self.ensure_no_live_outgoing_transfer(&profile, "prepare payment")?;
        let mut runtime = self.load_runtime_connected_synced(&profile, "payment preparation")?;
        if !runtime.contracts.has_contract(contract_id) {
            return Err(ApiError::bad_request(format!(
                "unknown RGB contract '{}'; import or issue it before paying",
                contract_id
            )));
        }

        let transfer_id = new_transfer_id()?;
        let params = TxParams::with(fee);
        let (pstt, payment) = runtime
            .pay_invoice(&invoice, CoinselectStrategy::Aggregate, params, giveaway)
            .map_err(|err| ApiError::bad_request(format!("unable to pay RGB invoice: {err}")))?;
        let (contract_transfers, pstt_ref) = self.persist_prepared_transfer(
            &mut runtime,
            &transfer_id,
            &contract_amounts,
            &contract_terminals,
            pstt,
            payment,
        )?;

        let transfer = RgbTransfer {
            id: transfer_id,
            contract_transfers,
            invoices: vec![invoice_text],
            pstt_ref,
            broadcast_txid: None,
            state: TransferState::ConsignmentReady,
            validation: ValidationState::Pending,
            updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };
        self.save_transfer_record(&transfer)?;
        Ok(transfer)
    }

    fn pay_invoices(&self, input: PayInvoicesInput) -> Result<RgbTransfer, ApiError> {
        let fee = parse_tondi_amount(&input.fee_tondi, "feeTondi")?;
        if fee.is_zero() {
            return Err(ApiError::bad_request("feeTondi must be greater than zero"));
        }
        if input.invoices.len() < 2 {
            return Err(ApiError::bad_request(
                "pay-batch requires at least two RGB invoices for a MIMO transfer",
            ));
        }
        let carrier_value = parse_tondi_amount(&input.carrier_value_tondi, "carrierValueTondi")?;
        let giveaway = carrier_value.is_non_zero().then_some(carrier_value);

        let invoice_texts = input
            .invoices
            .iter()
            .map(|invoice| invoice.trim().to_owned())
            .collect::<Vec<_>>();
        if invoice_texts.iter().any(|invoice| invoice.is_empty()) {
            return Err(ApiError::bad_request("invoices must not contain empty entries"));
        }
        let invoices = invoice_texts
            .iter()
            .map(|invoice| parse_rgb_invoice(invoice))
            .collect::<Result<Vec<_>, _>>()?;
        let contract_amounts = invoice_amounts_by_contract(&invoices)?;
        let contract_terminals = invoice_terminals_by_contract(&invoices);

        let profile = self.load_profile()?;
        if profile.locked {
            return Err(ApiError::locked());
        }
        let _wallet_guard = self.wallet_state_guard("batch payment preparation")?;
        self.ensure_no_live_outgoing_transfer(&profile, "prepare batch payment")?;
        let mut runtime =
            self.load_runtime_connected_synced(&profile, "batch payment preparation")?;
        for contract_id in contract_amounts.keys().copied() {
            if !runtime.contracts.has_contract(contract_id) {
                return Err(ApiError::bad_request(format!(
                    "unknown RGB contract '{}'; import or issue it before paying",
                    contract_id
                )));
            }
        }

        let transfer_id = new_transfer_id()?;
        let params = TxParams::with(fee);
        let (pstt, payment) = runtime
            .pay_invoices(&invoices, CoinselectStrategy::Aggregate, params, giveaway)
            .map_err(|err| {
                ApiError::bad_request(format!("unable to pay RGB invoice batch: {err}"))
            })?;
        let (contract_transfers, pstt_ref) = self.persist_prepared_transfer(
            &mut runtime,
            &transfer_id,
            &contract_amounts,
            &contract_terminals,
            pstt,
            payment,
        )?;

        let transfer = RgbTransfer {
            id: transfer_id,
            contract_transfers,
            invoices: invoice_texts,
            pstt_ref,
            broadcast_txid: None,
            state: TransferState::ConsignmentReady,
            validation: ValidationState::Pending,
            updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        };
        self.save_transfer_record(&transfer)?;
        Ok(transfer)
    }

    fn persist_prepared_transfer(
        &self,
        runtime: &mut RgbpRuntimeDir<MultiResolver>,
        transfer_id: &str,
        contract_amounts: &BTreeMap<ContractId, u128>,
        contract_terminals: &BTreeMap<ContractId, BTreeSet<AuthToken>>,
        pstt: Pstt,
        payment: Payment,
    ) -> Result<(Vec<RgbTransferContract>, String), ApiError> {
        let transfer_dir = self.config.data_dir.join("transfers");
        fs::create_dir_all(&transfer_dir).map_err(|err| {
            ApiError::internal(format!(
                "unable to create transfer directory '{}': {err}",
                transfer_dir.display()
            ))
        })?;
        let pstt_path = transfer_dir.join(format!("{transfer_id}.pstt"));
        let bundle_path = transfer_dir.join(format!("{transfer_id}.bundle"));

        let mut pstt_file = File::create_new(&pstt_path).map_err(|err| {
            ApiError::internal(format!("unable to create PSTT '{}': {err}", pstt_path.display()))
        })?;
        pstt.encode(PsttVer::V0, &mut pstt_file).map_err(|err| {
            ApiError::internal(format!("unable to encode PSTT '{}': {err}", pstt_path.display()))
        })?;
        payment.bundle.save(&bundle_path).map_err(|err| {
            ApiError::internal(format!(
                "unable to write RGB operation bundle '{}': {err}",
                bundle_path.display()
            ))
        })?;

        let single_contract = contract_amounts.len() == 1;
        let mut contract_transfers = Vec::with_capacity(contract_amounts.len());
        for (index, (contract_id, amount)) in contract_amounts.iter().enumerate() {
            let consignment_path = if single_contract {
                transfer_dir.join(format!("{transfer_id}.rgb"))
            } else {
                transfer_dir.join(format!("{transfer_id}.contract-{index}.rgb"))
            };
            let terminals = contract_terminals
                .get(contract_id)
                .cloned()
                .unwrap_or_else(|| payment.terminals.clone());
            runtime
                .contracts
                .consign_to_file(&consignment_path, *contract_id, terminals)
                .map_err(|err| {
                    ApiError::internal(format!(
                        "unable to create RGB transfer consignment '{}': {err}",
                        consignment_path.display()
                    ))
                })?;
            contract_transfers.push(RgbTransferContract {
                contract_id: contract_id.to_string(),
                amount: amount.to_string(),
                consignment_ref: self.file_ref(&consignment_path),
            });
        }

        Ok((contract_transfers, self.file_ref(&pstt_path)))
    }

    fn accept_transfer(&self, input: AcceptTransferInput) -> Result<RgbTransfer, ApiError> {
        let consignment_path = self.resolve_input_file(&input.consignment_ref)?;
        let profile = self.load_profile()?;
        if profile.locked {
            return Err(ApiError::locked());
        }
        let _wallet_guard = self.wallet_state_guard("transfer acceptance")?;
        let mut runtime = self.load_runtime_cached(&profile)?;
        let contract_id = consignment_contract_id(&consignment_path)?;
        if !runtime.contracts.has_contract(contract_id) {
            return Err(ApiError::bad_request(format!(
                "unknown RGB contract '{}'; accept the contract consignment before accepting this \
                 transfer",
                contract_id
            )));
        }
        let transfer_id = transfer_id_from_file(&consignment_path)?;
        let consignment_ref = self.file_ref(&consignment_path);
        let contract_id_text = contract_id.to_string();
        if let Some(transfer) =
            self.idempotent_accepted_transfer(&transfer_id, &contract_id_text, &consignment_ref)?
        {
            return Ok(transfer);
        }

        let before = owned_balance_value(&runtime.wallet_contract_state(contract_id));
        runtime
            .consume_from_file(false, &consignment_path, |_, _, _| Result::<_, Infallible>::Ok(()))
            .map_err(|err| {
                ApiError::bad_request(format!(
                    "unable to accept RGB transfer consignment '{}': {err}",
                    consignment_path.display()
                ))
            })?;
        let after = owned_balance_value(&runtime.wallet_contract_state(contract_id));
        let amount = after
            .checked_sub(before)
            .filter(|delta| *delta > 0)
            .ok_or_else(|| {
                ApiError::bad_request(format!(
                    "transfer consignment '{}' validated but did not add wallet-owned RGB state \
                     for contract '{}'",
                    consignment_path.display(),
                    contract_id
                ))
            })?;

        let contract_transfer = RgbTransferContract {
            contract_id: contract_id.to_string(),
            amount: amount.to_string(),
            consignment_ref,
        };
        let transfer = match self.load_transfer_record_optional(&transfer_id)? {
            Some(mut transfer) => {
                upsert_contract_transfer(&mut transfer.contract_transfers, contract_transfer);
                transfer.state = TransferState::ReceiverAccepted;
                transfer.validation = ValidationState::Valid;
                transfer.broadcast_txid = None;
                transfer.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
                transfer
            }
            None => RgbTransfer {
                id: transfer_id,
                contract_transfers: vec![contract_transfer],
                invoices: vec![],
                pstt_ref: String::new(),
                broadcast_txid: None,
                state: TransferState::ReceiverAccepted,
                validation: ValidationState::Valid,
                updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            },
        };
        self.save_transfer_record(&transfer)?;
        Ok(transfer)
    }

    fn finalise_transfer(&self, input: FinaliseTransferInput) -> Result<RgbTransfer, ApiError> {
        let pstt_path = self.resolve_input_file(&input.pstt_ref)?;
        let transfer_id = transfer_id_from_file(&pstt_path)?;
        let mut transfer = self.load_transfer_record(&transfer_id)?;
        if finalise_request_is_idempotent(&transfer, input.broadcast) {
            return Ok(transfer);
        }
        let profile = self.load_profile()?;
        if profile.locked {
            return Err(ApiError::locked());
        }
        let _wallet_guard = self.wallet_state_guard("transfer finalisation")?;
        transfer = self.load_transfer_record(&transfer_id)?;
        if finalise_request_is_idempotent(&transfer, input.broadcast) {
            return Ok(transfer);
        }

        let mut pstt_file = File::open(&pstt_path).map_err(|err| {
            ApiError::bad_request(format!("unable to open PSTT '{}': {err}", pstt_path.display()))
        })?;
        let mut pstt = Pstt::decode(&mut pstt_file).map_err(|err| {
            ApiError::bad_request(format!("unable to decode PSTT '{}': {err}", pstt_path.display()))
        })?;
        let signing_material = self.signing_material_for_pstt(&profile, &pstt)?;

        if input.broadcast {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| {
                    ApiError::internal(format!("unable to create broadcast runtime: {err}"))
                })?;
            let mut runtime = rt.block_on(async { self.load_runtime_connected(&profile).await })?;
            let txid = self.finalise_pstt(
                &mut runtime,
                &mut pstt,
                &pstt_path,
                true,
                signing_material.as_deref(),
            )?;
            transfer.broadcast_txid = Some(txid);
        } else {
            let mut runtime = self.load_runtime_cached(&profile)?;
            self.finalise_pstt(
                &mut runtime,
                &mut pstt,
                &pstt_path,
                false,
                signing_material.as_deref(),
            )?;
            transfer.broadcast_txid = None;
        }

        transfer.state =
            if input.broadcast { TransferState::Broadcast } else { TransferState::Finalised };
        transfer.validation = ValidationState::Pending;
        transfer.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        self.save_transfer_record(&transfer)?;
        Ok(transfer)
    }

    fn finalise_pstt(
        &self,
        runtime: &mut RgbpRuntimeDir<MultiResolver>,
        pstt: &mut Pstt,
        pstt_path: &Path,
        broadcast: bool,
        signing_material: Option<&str>,
    ) -> Result<String, ApiError> {
        if let Some(signing_material) = signing_material {
            sign_tondi_p2mr_inputs(pstt, signing_material).map_err(|err| {
                ApiError::bad_request(format!("unable to sign Tondi P2MR inputs: {err}"))
            })?;
        }
        pstt.finalize(runtime.wallet.descriptor());
        if !pstt.is_finalized() {
            return Err(ApiError::bad_request(format!(
                "PSTT '{}' still contains non-finalized inputs; sign the Tondi P2MR inputs before \
                 calling transfer finalisation",
                pstt_path.display()
            )));
        }
        let change = pstt_change_hint(pstt)?;
        let change_tweak = match change {
            Some((vout, _, _)) => pstt_p2mr_tweak(pstt, vout)?,
            None => None,
        };
        let extracted = pstt.extract().map_err(|err| {
            ApiError::bad_request(format!(
                "PSTT '{}' still contains {} non-finalized inputs",
                pstt_path.display(),
                err.0
            ))
        })?;
        let txid = extracted.txid().to_string();
        let tx_path = pstt_path.with_extension("tx");
        let mut tx_file = File::create(&tx_path).map_err(|err| {
            ApiError::internal(format!(
                "unable to create extracted transaction '{}': {err}",
                tx_path.display()
            ))
        })?;
        extracted.consensus_encode(&mut tx_file).map_err(|err| {
            ApiError::internal(format!(
                "unable to write extracted transaction '{}': {err}",
                tx_path.display()
            ))
        })?;
        if broadcast {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| {
                    ApiError::internal(format!("unable to create broadcast runtime: {err}"))
                })?;
            rt.block_on(async {
                runtime.wallet.broadcast_async(&extracted, change).await?;
                if let (Some((vout, _, _)), Some(tweak)) = (change, change_tweak) {
                    OwnerProvider::utxos_mut(&mut *runtime.wallet)
                        .set_p2mr_tweak_async(Outpoint::new(extracted.txid(), vout), Some(tweak))
                        .await;
                }
                Ok::<_, rgbp::resolvers::ResolverError>(())
            })
            .map_err(|err| ApiError::unavailable(format!("unable to broadcast transfer: {err}")))?;
        }
        Ok(txid)
    }

    fn store_unlocked_signer(
        &self,
        profile: &StoredProfile,
        seed: [u8; 64],
    ) -> Result<(), ApiError> {
        let mut signer = self
            .signer_session
            .lock()
            .map_err(|_| ApiError::internal("wallet signer session lock is poisoned"))?;
        if let Some(existing) = signer.as_mut() {
            existing.seed.fill(0);
        }
        *signer = Some(UnlockedSigner {
            wallet_id: profile.wallet_id.clone(),
            network: profile.network,
            seed,
        });
        Ok(())
    }

    fn clear_unlocked_signer(&self) -> Result<(), ApiError> {
        let mut signer = self
            .signer_session
            .lock()
            .map_err(|_| ApiError::internal("wallet signer session lock is poisoned"))?;
        if let Some(existing) = signer.as_mut() {
            existing.seed.fill(0);
        }
        *signer = None;
        Ok(())
    }

    fn signing_material_for_pstt(
        &self,
        profile: &StoredProfile,
        pstt: &Pstt,
    ) -> Result<Option<String>, ApiError> {
        let runtime = self.load_runtime_cached(profile)?;
        let mut inputs = Vec::<(Terminal, Option<[u8; 32]>)>::new();
        for input in pstt.inputs() {
            if input.final_script_sig.is_some() {
                continue;
            }
            let tweak = OwnerProvider::utxos(&*runtime.wallet).p2mr_tweak(input.previous_outpoint);
            let terminal = input_terminal_derivation(input).or_else(|| {
                OwnerProvider::utxos(&*runtime.wallet)
                    .get(input.previous_outpoint)
                    .map(|(_, terminal)| terminal)
            });
            let Some(terminal) = terminal else {
                continue;
            };
            let signing_input = (terminal, tweak);
            if !inputs.contains(&signing_input) {
                inputs.push(signing_input);
            }
        }
        if inputs.is_empty() {
            return Ok(None);
        }

        let signer = self
            .signer_session
            .lock()
            .map_err(|_| ApiError::internal("wallet signer session lock is poisoned"))?;
        let signer = signer.as_ref().ok_or_else(|| {
            ApiError::bad_request(
                "wallet signer session is unavailable; unlock the RGB-on-Tondi wallet before \
                 finalising PSTT inputs",
            )
        })?;
        if signer.wallet_id != profile.wallet_id || signer.network != profile.network {
            return Err(ApiError::bad_request(
                "wallet signer session does not match the active wallet profile",
            ));
        }

        let account = account_from_seed(signer.network, &signer.seed);
        let derived_keys = inputs
            .into_iter()
            .map(|(terminal, tweak)| {
                let mut key =
                    hex_encode(&account.derive_terminal_secret_key(terminal).secret_bytes());
                if let Some(tweak) = tweak {
                    key.push(':');
                    key.push_str(&hex_encode(&tweak));
                }
                key
            })
            .collect::<Vec<_>>()
            .join(",");
        Ok(Some(derived_keys))
    }

    fn load_profile(&self) -> Result<StoredProfile, ApiError> {
        let _guard = self
            .profile_lock
            .lock()
            .map_err(|_| ApiError::internal("wallet profile lock is poisoned"))?;
        self.load_profile_unlocked()
    }

    fn load_profile_unlocked(&self) -> Result<StoredProfile, ApiError> {
        let path = self.profile_path();
        let bytes = fs::read(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("wallet_profile_unavailable", "No wallet profile exists.")
            } else {
                ApiError::internal(format!(
                    "unable to read wallet profile '{}': {err}",
                    path.display()
                ))
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|err| {
            ApiError::internal(format!("wallet profile '{}' is invalid: {err}", path.display()))
        })
    }

    fn save_profile_unlocked(&self, profile: &StoredProfile) -> Result<(), ApiError> {
        fs::create_dir_all(&self.config.data_dir).map_err(|err| {
            ApiError::internal(format!("unable to create RGB data directory: {err}"))
        })?;
        let bytes = serde_json::to_vec_pretty(profile)
            .map_err(|err| ApiError::internal(format!("unable to encode wallet profile: {err}")))?;
        fs::write(self.profile_path(), bytes)
            .map_err(|err| ApiError::internal(format!("unable to write wallet profile: {err}")))
    }

    fn dashboard_snapshot_synced(&self) -> Result<DashboardSnapshot, ApiError> {
        let profile = match self.load_profile() {
            Ok(profile) => profile,
            Err(err) if err.status == StatusCode::NOT_FOUND => {
                return Ok(self.unavailable_dashboard(
                    WalletLifecycle::NotCreated,
                    "No RGB-on-Tondi wallet profile is configured for the HTTP service yet.",
                ));
            }
            Err(err) => return Err(err),
        };

        if profile.locked {
            return Ok(self.unavailable_dashboard(
                WalletLifecycle::Locked,
                "The RGB-on-Tondi wallet profile is locked.",
            ));
        }

        let _wallet_guard = self.wallet_state_guard("wallet sync")?;
        let mut runtime = self.load_runtime_connected_synced(&profile, "wallet sync")?;
        self.recover_broadcast_transfer_records(&profile)?;
        self.dashboard_from_runtime(&profile, &mut runtime)
    }

    fn load_runtime_cached(
        &self,
        profile: &StoredProfile,
    ) -> Result<RgbpRuntimeDir<MultiResolver>, ApiError> {
        let resolver = MultiResolver::new_absent().map_err(|err| {
            ApiError::internal(format!("unable to create absent resolver: {err}"))
        })?;
        self.load_runtime_with_resolver(profile, resolver)
    }

    async fn load_runtime_connected(
        &self,
        profile: &StoredProfile,
    ) -> Result<RgbpRuntimeDir<MultiResolver>, ApiError> {
        let resolver = MultiResolver::new_tondi_with_min_blue_score_depth(
            &profile.tondi_rpc_endpoint,
            profile.network.into(),
            u64::from(self.config.min_confirmations),
        )
        .await
        .map_err(|err| ApiError::unavailable(format!("unable to connect to Tondi RPC: {err}")))?;
        self.load_runtime_with_resolver(profile, resolver)
    }

    fn load_runtime_connected_synced(
        &self,
        profile: &StoredProfile,
        operation: &'static str,
    ) -> Result<RgbpRuntimeDir<MultiResolver>, ApiError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                ApiError::internal(format!("unable to create {operation} runtime: {err}"))
            })?;
        let mut runtime = rt.block_on(async { self.load_runtime_connected(profile).await })?;
        rt.block_on(async {
            runtime
                .update_async(self.config.min_confirmations)
                .await
                .map_err(|err| {
                    ApiError::unavailable(format!(
                        "unable to synchronize wallet for {operation}: {err}"
                    ))
                })
        })?;
        Ok(runtime)
    }

    fn recover_broadcast_transfer_records(&self, profile: &StoredProfile) -> Result<(), ApiError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                ApiError::internal(format!("unable to create transfer recovery runtime: {err}"))
            })?;
        let resolver = rt
            .block_on(async {
                MultiResolver::new_tondi_with_min_blue_score_depth(
                    &profile.tondi_rpc_endpoint,
                    profile.network.into(),
                    u64::from(self.config.min_confirmations),
                )
                .await
            })
            .map_err(|err| {
                ApiError::unavailable(format!(
                    "unable to connect to Tondi RPC for transfer recovery: {err}"
                ))
            })?;

        self.recover_broadcast_transfer_records_with_status(|txid| {
            rt.block_on(async { resolver.resolve_tx_status_async(txid).await })
                .map_err(|err| {
                    ApiError::unavailable(format!(
                        "unable to resolve transfer transaction status for '{}': {err}",
                        txid
                    ))
                })
        })
    }

    fn recover_broadcast_transfer_records_with_status(
        &self,
        mut resolve_status: impl FnMut(Txid) -> Result<WitnessStatus, ApiError>,
    ) -> Result<(), ApiError> {
        let dir = self.config.data_dir.join("transfers");
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(());
        };

        for path in entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        {
            let bytes = fs::read(&path).map_err(|err| {
                ApiError::internal(format!(
                    "unable to read transfer record '{}': {err}",
                    path.display()
                ))
            })?;
            let mut transfer = serde_json::from_slice::<RgbTransfer>(&bytes).map_err(|err| {
                ApiError::internal(format!(
                    "transfer record '{}' is invalid: {err}",
                    path.display()
                ))
            })?;
            let had_broadcast_txid = transfer.broadcast_txid.is_some();
            let Some(txid) = self.transfer_record_txid(&transfer)? else {
                continue;
            };
            let status = resolve_status(txid)?;
            let changed = if had_broadcast_txid {
                refresh_broadcast_transfer_status(&mut transfer, status)
            } else {
                recover_broadcast_transfer(&mut transfer, &txid.to_string(), status)
            };
            if changed {
                self.save_transfer_record(&transfer)?;
            }
        }

        Ok(())
    }

    fn transfer_record_txid(&self, transfer: &RgbTransfer) -> Result<Option<Txid>, ApiError> {
        if let Some(txid) = transfer.broadcast_txid.as_deref() {
            let txid = Txid::from_str(txid).map_err(|err| {
                ApiError::internal(format!(
                    "transfer record '{}' has invalid broadcastTxid '{}': {err}",
                    transfer.id, txid
                ))
            })?;
            return Ok(Some(txid));
        }
        self.extracted_transfer_tx(transfer)
            .map(|tx| tx.map(|tx| tx.txid()))
    }

    fn extracted_transfer_tx(&self, transfer: &RgbTransfer) -> Result<Option<Tx>, ApiError> {
        if transfer.pstt_ref.trim().is_empty() {
            return Ok(None);
        }
        let tx_path = self
            .resolve_input_file(&transfer.pstt_ref)?
            .with_extension("tx");
        let bytes = match fs::read(&tx_path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(ApiError::internal(format!(
                    "unable to read extracted transaction '{}': {err}",
                    tx_path.display()
                )));
            }
        };
        Tx::consensus_deserialize(bytes).map(Some).map_err(|err| {
            ApiError::bad_request(format!(
                "unable to decode extracted transaction '{}': {err}",
                tx_path.display()
            ))
        })
    }

    fn load_runtime_with_resolver(
        &self,
        profile: &StoredProfile,
        resolver: MultiResolver,
    ) -> Result<RgbpRuntimeDir<MultiResolver>, ApiError> {
        let holder = FileHolder::load(self.wallet_dir(&profile.wallet_id)).map_err(|err| {
            ApiError::internal(format!("unable to load wallet '{}': {err}", profile.wallet_id))
        })?;
        let wallet = Owner::with_components(profile.network, holder, resolver);
        Ok(RgbpRuntimeDir::from(RgbWallet::with_components(wallet, self.load_contracts()?)))
    }

    fn wallet_profile(&self, profile: &StoredProfile) -> Result<WalletProfile, ApiError> {
        if profile.p2mr_address.is_empty() {
            return Err(ApiError::internal(format!(
                "wallet '{}' has no persisted P2MR address",
                profile.wallet_id
            )));
        }
        Ok(WalletProfile {
            wallet_id: profile.wallet_id.clone(),
            network: profile.network.into(),
            tondi_rpc_endpoint: profile.tondi_rpc_endpoint.clone(),
            p2mr_address: profile.p2mr_address.clone(),
            lifecycle: if profile.locked {
                WalletLifecycle::Locked
            } else {
                WalletLifecycle::Ready
            },
        })
    }

    fn next_p2mr_address(&self, profile: &StoredProfile) -> Result<String, ApiError> {
        let holder = FileHolder::load(self.wallet_dir(&profile.wallet_id)).map_err(|err| {
            ApiError::internal(format!("unable to load wallet '{}': {err}", profile.wallet_id))
        })?;
        let resolver = MultiResolver::new_absent().map_err(|err| {
            ApiError::internal(format!("unable to create absent resolver: {err}"))
        })?;
        let mut owner = Owner::with_components(profile.network, holder, resolver);
        owner.next_tondi_p2mr_address().map_err(ApiError::internal)
    }

    fn load_contracts(&self) -> Result<Contracts<StockpileDir<TxoSeal>>, ApiError> {
        let consensus = consensus_for(self.config.network);
        let stockpile = StockpileDir::<TxoSeal>::load(
            self.config.data_dir.clone(),
            consensus,
            self.config.network.is_testnet(),
        )
        .map_err(|err| ApiError::internal(format!("unable to load RGB stockpile: {err}")))?;
        Ok(Contracts::load(stockpile))
    }

    fn resolve_input_file(&self, name: &str) -> Result<PathBuf, ApiError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("input file name must not be empty"));
        }
        let path = PathBuf::from(name);
        if path.is_absolute() {
            return Ok(path);
        }
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        {
            return Err(ApiError::bad_request(
                "relative input files must stay under the RGB data directory",
            ));
        }
        Ok(self.config.data_dir.join(path))
    }

    fn file_ref(&self, path: &Path) -> String {
        path.strip_prefix(&self.config.data_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    fn asset_metadata_path(&self, contract_id: &str) -> PathBuf {
        let file_name =
            contract_id
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '~') {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
        self.config
            .data_dir
            .join(ASSET_METADATA_DIR)
            .join(format!("{file_name}.json"))
    }

    fn save_asset_metadata(&self, asset: &RgbAsset) -> Result<(), ApiError> {
        let metadata = StoredAssetMetadata {
            contract_id: asset.contract_id.clone(),
            ticker: asset.ticker.clone(),
            name: asset.name.clone(),
            precision: asset.precision,
            issued_supply: asset.issued_supply.clone(),
            contract_consignment_ref: asset.contract_consignment_ref.clone(),
        };
        let path = self.asset_metadata_path(&asset.contract_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                ApiError::internal(format!(
                    "unable to create asset metadata directory '{}': {err}",
                    parent.display()
                ))
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&metadata)
            .map_err(|err| ApiError::internal(format!("unable to encode asset metadata: {err}")))?;
        fs::write(&path, bytes).map_err(|err| {
            ApiError::internal(format!(
                "unable to write asset metadata '{}': {err}",
                path.display()
            ))
        })
    }

    fn load_asset_metadata(&self) -> Result<BTreeMap<String, StoredAssetMetadata>, ApiError> {
        let dir = self.config.data_dir.join(ASSET_METADATA_DIR);
        let mut metadata = BTreeMap::new();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(metadata),
            Err(err) => {
                return Err(ApiError::internal(format!(
                    "unable to read asset metadata directory '{}': {err}",
                    dir.display()
                )));
            }
        };
        for entry in entries {
            let entry = entry.map_err(|err| {
                ApiError::internal(format!(
                    "unable to read asset metadata entry in '{}': {err}",
                    dir.display()
                ))
            })?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(|err| {
                ApiError::internal(format!(
                    "unable to read asset metadata '{}': {err}",
                    path.display()
                ))
            })?;
            let asset: StoredAssetMetadata = serde_json::from_slice(&bytes).map_err(|err| {
                ApiError::internal(format!("asset metadata '{}' is invalid: {err}", path.display()))
            })?;
            metadata.insert(asset.contract_id.clone(), asset);
        }
        Ok(metadata)
    }

    fn apply_asset_metadata(&self, asset: &mut RgbAsset, metadata: &StoredAssetMetadata) {
        asset.ticker = metadata.ticker.clone();
        asset.name = metadata.name.clone();
        asset.precision = metadata.precision;
        if metadata.issued_supply.is_some() {
            asset.issued_supply = metadata.issued_supply.clone();
        }
        if metadata.contract_consignment_ref.is_some() {
            asset.contract_consignment_ref = metadata.contract_consignment_ref.clone();
        }
    }

    fn transfer_record_path(&self, transfer_id: &str) -> PathBuf {
        self.config
            .data_dir
            .join("transfers")
            .join(format!("{transfer_id}.json"))
    }

    fn contract_consignment_path(&self, contract_id: ContractId) -> PathBuf {
        self.config
            .data_dir
            .join("contracts")
            .join(format!("{contract_id}.rgb"))
    }

    fn save_transfer_record(&self, transfer: &RgbTransfer) -> Result<(), ApiError> {
        let path = self.transfer_record_path(&transfer.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                ApiError::internal(format!(
                    "unable to create transfer record directory '{}': {err}",
                    parent.display()
                ))
            })?;
        }
        let bytes = serde_json::to_vec_pretty(transfer).map_err(|err| {
            ApiError::internal(format!("unable to encode transfer record '{}': {err}", transfer.id))
        })?;
        let tmp_path = path.with_file_name(format!("{}.tmp", transfer.id));
        fs::write(&tmp_path, bytes).map_err(|err| {
            ApiError::internal(format!(
                "unable to write temporary transfer record '{}': {err}",
                tmp_path.display()
            ))
        })?;
        fs::rename(&tmp_path, &path).map_err(|err| {
            let _ = fs::remove_file(&tmp_path);
            ApiError::internal(format!(
                "unable to commit transfer record '{}': {err}",
                path.display()
            ))
        })
    }

    fn load_transfer_record(&self, transfer_id: &str) -> Result<RgbTransfer, ApiError> {
        let path = self.transfer_record_path(transfer_id);
        let bytes = fs::read(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                ApiError::bad_request(format!(
                    "transfer record '{}' is missing; finalise only transfers prepared through \
                     this HTTP service",
                    path.display()
                ))
            } else {
                ApiError::internal(format!(
                    "unable to read transfer record '{}': {err}",
                    path.display()
                ))
            }
        })?;
        serde_json::from_slice(&bytes).map_err(|err| {
            ApiError::internal(format!("transfer record '{}' is invalid: {err}", path.display()))
        })
    }

    fn load_transfer_record_optional(
        &self,
        transfer_id: &str,
    ) -> Result<Option<RgbTransfer>, ApiError> {
        let path = self.transfer_record_path(transfer_id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(ApiError::internal(format!(
                    "unable to read transfer record '{}': {err}",
                    path.display()
                )));
            }
        };
        serde_json::from_slice(&bytes).map(Some).map_err(|err| {
            ApiError::internal(format!("transfer record '{}' is invalid: {err}", path.display()))
        })
    }

    fn idempotent_accepted_transfer(
        &self,
        transfer_id: &str,
        contract_id: &str,
        consignment_ref: &str,
    ) -> Result<Option<RgbTransfer>, ApiError> {
        let Some(transfer) = self.load_transfer_record_optional(transfer_id)? else {
            return Ok(None);
        };
        let already_accepted = transfer.state == TransferState::ReceiverAccepted
            && transfer.validation == ValidationState::Valid
            && transfer.contract_transfers.iter().any(|entry| {
                entry.contract_id == contract_id && entry.consignment_ref == consignment_ref
            });
        Ok(already_accepted.then_some(transfer))
    }

    fn wallet_state_guard(&self, operation: &'static str) -> Result<MutexGuard<'_, ()>, ApiError> {
        self.wallet_state_lock.lock().map_err(|_| {
            ApiError::internal(format!("wallet state lock is poisoned during {operation}"))
        })
    }

    fn ensure_no_live_outgoing_transfer(
        &self,
        profile: &StoredProfile,
        operation: &'static str,
    ) -> Result<(), ApiError> {
        let transfers = self.load_transfer_records()?;
        if let Some(transfer) = transfers
            .iter()
            .find(|transfer| live_outgoing_transfer_without_broadcast(transfer))
        {
            return Err(ApiError::conflict(format!(
                "cannot {operation}: transfer '{}' is still {:?} without a broadcastTxid; \
                 finalise, broadcast, or recover it with wallet sync before preparing another \
                 outgoing transfer",
                transfer.id, transfer.state
            )));
        }
        let broadcast_transfers = transfers
            .iter()
            .filter(|transfer| live_outgoing_broadcast_transfer(transfer))
            .collect::<Vec<_>>();
        if broadcast_transfers.is_empty() {
            return Ok(());
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                ApiError::internal(format!("unable to create transfer state-lock runtime: {err}"))
            })?;
        let resolver = rt
            .block_on(async {
                MultiResolver::new_tondi_with_min_blue_score_depth(
                    &profile.tondi_rpc_endpoint,
                    profile.network.into(),
                    u64::from(self.config.min_confirmations),
                )
                .await
            })
            .map_err(|err| {
                ApiError::unavailable(format!(
                    "unable to connect to Tondi RPC for transfer state-lock: {err}"
                ))
            })?;
        for transfer in broadcast_transfers {
            let txid_text = transfer.broadcast_txid.as_deref().unwrap_or_default();
            let txid = Txid::from_str(txid_text).map_err(|err| {
                ApiError::internal(format!(
                    "transfer record '{}' has invalid broadcastTxid '{}': {err}",
                    transfer.id, txid_text
                ))
            })?;
            let status = rt
                .block_on(async { resolver.resolve_tx_status_async(txid).await })
                .map_err(|err| {
                    ApiError::unavailable(format!(
                        "unable to resolve transfer transaction status for '{}': {err}",
                        txid
                    ))
                })?;
            if live_outgoing_broadcast_status_blocks_new_pay(transfer, status) {
                return Err(ApiError::conflict(format!(
                    "cannot {operation}: transfer '{}' broadcastTxid '{}' is still tentative in \
                     Tondi; wait for acceptance, removal, or explicit recovery before preparing \
                     another outgoing transfer",
                    transfer.id, txid
                )));
            }
        }
        Ok(())
    }

    fn load_transfer_records(&self) -> Result<Vec<RgbTransfer>, ApiError> {
        let dir = self.config.data_dir.join("transfers");
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(vec![]);
        };
        let mut records = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| {
                ApiError::internal(format!("unable to read transfer record directory: {err}"))
            })?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(|err| {
                ApiError::internal(format!(
                    "unable to read transfer record '{}': {err}",
                    path.display()
                ))
            })?;
            let record = serde_json::from_slice::<RgbTransfer>(&bytes).map_err(|err| {
                ApiError::internal(format!(
                    "transfer record '{}' is invalid: {err}",
                    path.display()
                ))
            })?;
            records.push(record);
        }
        records.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(records)
    }

    fn dashboard_from_runtime(
        &self,
        profile: &StoredProfile,
        runtime: &mut RgbpRuntimeDir<MultiResolver>,
    ) -> Result<DashboardSnapshot, ApiError> {
        let wallet_profile = self.wallet_profile(profile)?;
        let mut chain_balance_sats = 0u128;
        let p2mr_outputs = runtime
            .wallet
            .utxos()
            .map(|outpoint| {
                let value_sats = runtime
                    .wallet
                    .utxo(outpoint)
                    .map(|(utxo, _)| u128::from(utxo.value.sats()))
                    .unwrap_or(0);
                chain_balance_sats += value_sats;
                P2mrOutput {
                    seal: format!("{}:p2mr-seal:{outpoint}", network_label(profile.network)),
                    outpoint: outpoint.to_string(),
                    value_tondi: format_tondi_sats(value_sats),
                    state: P2mrOutputState::Available,
                    bound_contract_ids: vec![],
                }
            })
            .collect::<Vec<_>>();
        let asset_metadata = self.load_asset_metadata()?;
        let assets = runtime
            .contracts
            .contract_ids()
            .map(|contract_id| {
                let mut asset = rgb_asset_from_runtime(runtime, contract_id);
                if let Some(metadata) = asset_metadata.get(&asset.contract_id) {
                    self.apply_asset_metadata(&mut asset, metadata);
                }
                asset
            })
            .collect();

        Ok(DashboardSnapshot {
            profile: wallet_profile,
            chain_balance_tondi: format_tondi_sats(chain_balance_sats),
            p2mr_outputs,
            assets,
            invoices: vec![],
            transfers: self.load_transfer_records()?,
            service_mode: DashboardServiceMode::Connected,
            service_notice: "Connected to RGB-on-Tondi service.".to_owned(),
        })
    }

    fn unavailable_dashboard(
        &self,
        lifecycle: WalletLifecycle,
        notice: impl Into<String>,
    ) -> DashboardSnapshot {
        DashboardSnapshot {
            profile: WalletProfile {
                wallet_id: "unconfigured".to_owned(),
                network: self.config.network.into(),
                tondi_rpc_endpoint: self.config.tondi_rpc_endpoint.clone(),
                p2mr_address: String::new(),
                lifecycle,
            },
            chain_balance_tondi: "0.00000000".to_owned(),
            p2mr_outputs: vec![],
            assets: vec![],
            invoices: vec![],
            transfers: vec![],
            service_mode: DashboardServiceMode::Unavailable,
            service_notice: notice.into(),
        }
    }
}

fn rgb_asset_from_runtime(
    runtime: &RgbpRuntimeDir<MultiResolver>,
    contract_id: ContractId,
) -> RgbAsset {
    let articles = runtime.contracts.contract_articles(contract_id);
    let info = ContractInfo::new(contract_id, &articles);
    let state = runtime.wallet_contract_state(contract_id);
    let ticker =
        immutable_state_text(&state, "ticker").unwrap_or_else(|| info.codex.name.to_string());
    let name = immutable_state_text(&state, "name").unwrap_or_else(|| info.name.to_string());
    let precision = immutable_state_text(&state, "precision")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    let issued_supply = immutable_state_text(&state, "issued");
    RgbAsset {
        contract_id: contract_id.to_string(),
        ticker,
        name,
        precision,
        balance: owned_balance(&state),
        issued_supply,
        contract_consignment_ref: None,
        icon_url: None,
        issuer: info.issuer.to_string(),
        validation: ValidationState::Valid,
        last_transition: Some(info.timestamp.to_rfc3339()),
    }
}

fn immutable_state_text(
    state: &rgb::ContractState<Outpoint>,
    name: &'static str,
) -> Option<String> {
    state
        .immutable
        .get(&VariantName::from(name))
        .and_then(|items| items.last())
        .map(|item| strict_value_text(&item.data.verified))
}

fn strict_value_text(value: &StrictVal) -> String {
    match value {
        StrictVal::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn parse_rgb_invoice(value: &str) -> Result<CoreRgbInvoice<ContractId>, ApiError> {
    CoreRgbInvoice::<ContractId>::from_str(value.trim())
        .map_err(|err| ApiError::bad_request(format!("invalid RGB invoice: {err}")))
}

fn invoice_amounts_by_contract(
    invoices: &[CoreRgbInvoice<ContractId>],
) -> Result<BTreeMap<ContractId, u128>, ApiError> {
    let mut totals = BTreeMap::<ContractId, u128>::new();
    for invoice in invoices {
        let value = invoice
            .data
            .as_ref()
            .ok_or_else(|| ApiError::bad_request("all batch invoices must include an amount"))?;
        let amount = strict_value_text(value).parse::<u128>().map_err(|err| {
            ApiError::bad_request(format!(
                "batch invoices must use numeric amount state values: {err}"
            ))
        })?;
        let total = totals.entry(invoice.scope).or_insert(0);
        *total = total
            .checked_add(amount)
            .ok_or_else(|| ApiError::bad_request("batch invoice amount exceeds supported range"))?;
    }
    Ok(totals)
}

fn invoice_terminals_by_contract(
    invoices: &[CoreRgbInvoice<ContractId>],
) -> BTreeMap<ContractId, BTreeSet<AuthToken>> {
    let mut terminals = BTreeMap::<ContractId, BTreeSet<AuthToken>>::new();
    for invoice in invoices {
        let terminal = match invoice.auth {
            RgbBeneficiary::Token(auth) => auth,
        };
        terminals.entry(invoice.scope).or_default().insert(terminal);
    }
    terminals
}

fn upsert_contract_transfer(
    transfers: &mut Vec<RgbTransferContract>,
    accepted: RgbTransferContract,
) {
    if let Some(existing) = transfers
        .iter_mut()
        .find(|item| item.contract_id == accepted.contract_id)
    {
        *existing = accepted;
    } else {
        transfers.push(accepted);
        transfers.sort_by(|left, right| left.contract_id.cmp(&right.contract_id));
    }
}

fn finalise_request_is_idempotent(transfer: &RgbTransfer, broadcast: bool) -> bool {
    match (broadcast, &transfer.state) {
        (true, TransferState::Broadcast) => transfer.broadcast_txid.is_some(),
        (false, TransferState::Finalised | TransferState::Broadcast) => true,
        _ => false,
    }
}

fn live_outgoing_transfer_without_broadcast(transfer: &RgbTransfer) -> bool {
    !transfer.pstt_ref.trim().is_empty()
        && transfer.broadcast_txid.is_none()
        && matches!(
            transfer.state,
            TransferState::ConsignmentReady | TransferState::Finalised | TransferState::Broadcast
        )
}

fn live_outgoing_broadcast_transfer(transfer: &RgbTransfer) -> bool {
    !transfer.pstt_ref.trim().is_empty()
        && transfer.broadcast_txid.is_some()
        && transfer.state == TransferState::Broadcast
}

fn live_outgoing_broadcast_status_blocks_new_pay(
    transfer: &RgbTransfer,
    status: WitnessStatus,
) -> bool {
    live_outgoing_broadcast_transfer(transfer) && status == WitnessStatus::Tentative
}

fn recover_broadcast_transfer(
    transfer: &mut RgbTransfer,
    txid: &str,
    status: WitnessStatus,
) -> bool {
    if transfer.broadcast_txid.is_some() || transfer.pstt_ref.trim().is_empty() {
        return false;
    }
    if !matches!(
        status,
        WitnessStatus::Tentative | WitnessStatus::Mined(_) | WitnessStatus::Genesis
    ) {
        return false;
    }
    transfer.broadcast_txid = Some(txid.to_owned());
    transfer.state = TransferState::Broadcast;
    transfer.validation = transfer_validation_for_witness_status(status);
    transfer.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    true
}

fn refresh_broadcast_transfer_status(transfer: &mut RgbTransfer, status: WitnessStatus) -> bool {
    if !live_outgoing_broadcast_transfer(transfer) {
        return false;
    }
    let validation = refresh_transfer_validation(transfer.validation, status);
    if transfer.validation == validation {
        return false;
    }
    transfer.validation = validation;
    transfer.updated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    true
}

fn refresh_transfer_validation(current: ValidationState, status: WitnessStatus) -> ValidationState {
    match (current, status) {
        (_, WitnessStatus::Archived) => ValidationState::Rejected,
        (ValidationState::Valid, WitnessStatus::Tentative) => ValidationState::Valid,
        (_, status) => transfer_validation_for_witness_status(status),
    }
}

fn transfer_validation_for_witness_status(status: WitnessStatus) -> ValidationState {
    match status {
        WitnessStatus::Archived => ValidationState::Rejected,
        WitnessStatus::Tentative => ValidationState::Pending,
        WitnessStatus::Genesis | WitnessStatus::Mined(_) | WitnessStatus::Offchain => {
            ValidationState::Valid
        }
    }
}

fn owned_balance<Seal>(state: &rgb::ContractState<Seal>) -> String {
    owned_balance_value(state).to_string()
}

fn owned_balance_value<Seal>(state: &rgb::ContractState<Seal>) -> u128 {
    state
        .owned
        .values()
        .flatten()
        .filter_map(|owned| owned.assignment.data.to_string().parse::<u128>().ok())
        .sum::<u128>()
}

fn consensus_for(network: Network) -> Consensus {
    if network.is_testnet() {
        Consensus::TondiTestnet
    } else {
        Consensus::Tondi
    }
}

fn network_from_contract(network: TondiNetwork) -> Network {
    match network {
        TondiNetwork::Mainnet => Network::Mainnet,
        TondiNetwork::Testnet => Network::Testnet,
        TondiNetwork::Devnet => Network::Devnet,
    }
}

fn network_label(network: Network) -> &'static str {
    match network {
        Network::Mainnet => "mainnet",
        Network::Testnet | Network::Simnet => "testnet",
        Network::Devnet => "devnet",
    }
}

fn format_tondi_sats(sats: u128) -> String {
    let scale = u128::from(Sats::TONDI_COIN.0);
    let whole = sats / scale;
    let fractional = sats % scale;
    format!("{whole}.{fractional:08}")
}

impl From<Network> for TondiNetwork {
    fn from(network: Network) -> Self {
        match network {
            Network::Mainnet => Self::Mainnet,
            Network::Testnet | Network::Simnet => Self::Testnet,
            Network::Devnet => Self::Devnet,
        }
    }
}

fn parse_asset_ticker(value: &str) -> Result<String, ApiError> {
    let ticker = value.trim();
    if ticker.is_empty() || ticker.len() > 16 {
        return Err(ApiError::bad_request("ticker must be between 1 and 16 characters"));
    }
    if !ticker
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ApiError::bad_request(
            "ticker must use uppercase ASCII letters, digits, or '_'",
        ));
    }
    Ok(ticker.to_owned())
}

fn parse_asset_name(value: &str) -> Result<String, ApiError> {
    let name = value.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(ApiError::bad_request("asset name must be between 1 and 64 characters"));
    }
    if !name.is_ascii() {
        return Err(ApiError::bad_request(
            "asset name must be ASCII until issuer metadata supports Unicode normalisation",
        ));
    }
    Ok(name.to_owned())
}

fn parse_state_name(value: &str) -> Result<VariantName, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request("stateName must not be empty"));
    }
    VariantName::from_str(value).map_err(|err| {
        ApiError::bad_request(format!(
            "stateName must be a strict RGB state name such as amount: {err}"
        ))
    })
}

fn parse_api_name(value: &str) -> Result<TypeName, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request("apiName must not be empty"));
    }
    TypeName::from_str(value).map_err(|err| {
        ApiError::bad_request(format!(
            "apiName must be a strict RGB API type name such as Test: {err}"
        ))
    })
}

fn parse_method_name(value: &str) -> Result<VariantName, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request("methodName must not be empty"));
    }
    VariantName::from_str(value).map_err(|err| {
        ApiError::bad_request(format!(
            "methodName must be a strict RGB method name such as transfer: {err}"
        ))
    })
}

fn pstt_change_hint(pstt: &Pstt) -> Result<Option<(Vout, u32, u32)>, ApiError> {
    let wallet_outputs = pstt
        .outputs()
        .filter_map(|output| {
            let terminal = output.terminal_derivation()?;
            Some((output.vout(), terminal.keychain.index(), terminal.index.index()))
        })
        .collect::<Vec<_>>();

    match wallet_outputs.as_slice() {
        [] => Ok(None),
        [change] => Ok(Some(*change)),
        _ => Err(ApiError::bad_request(
            "PSTT contains multiple wallet-derived outputs; HTTP finalisation currently supports \
             a single Tondi P2MR change output",
        )),
    }
}

fn pstt_p2mr_tweak(pstt: &Pstt, vout: Vout) -> Result<Option<[u8; 32]>, ApiError> {
    let output = pstt
        .output(vout.into_usize())
        .ok_or_else(|| ApiError::bad_request(format!("PSTT change output {vout} is missing")))?;
    let Some(original_pubkey) = output.proprietary(&p2mr_original_pubkey_key()) else {
        return Ok(None);
    };
    let original_pubkey: [u8; 33] = original_pubkey.as_slice().try_into().map_err(|_| {
        ApiError::bad_request("PSTT P2MR change output has invalid original pubkey metadata")
    })?;
    let original_pubkey = CompressedPk::from_byte_array(original_pubkey).map_err(|_| {
        ApiError::bad_request("PSTT P2MR change output has invalid original pubkey metadata")
    })?;
    let commitment = output
        .proprietary(&PropKey::mpc_commitment())
        .ok_or_else(|| ApiError::bad_request("PSTT P2MR change output misses MPC commitment"))?;
    let commitment = mpc::Commitment::copy_from_slice(commitment.as_slice())
        .map_err(|_| ApiError::bad_request("PSTT P2MR change output has invalid MPC commitment"))?;
    let tweak = TondiP2mrProof::new(original_pubkey)
        .rgb_tweak(commitment)
        .map_err(|err| ApiError::bad_request(format!("invalid P2MR RGB tweak: {err}")))?;
    Ok(Some(tweak.to_be_bytes()))
}

fn parse_u64_amount(value: &str, field: &'static str) -> Result<u64, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request(format!("{field} must not be empty")));
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::bad_request(format!(
            "{field} must be a non-negative integer in the contract's smallest unit"
        )));
    }
    value.parse::<u64>().map_err(|err| {
        ApiError::bad_request(format!("{field} is too large for the current RGB runtime: {err}"))
    })
}

fn parse_tondi_amount(value: &str, field: &'static str) -> Result<Sats, ApiError> {
    const SCALE: u64 = Sats::TONDI_COIN.0;
    const DECIMALS: usize = 8;

    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request(format!("{field} must not be empty")));
    }
    let (whole, fractional) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::bad_request(format!("{field} must be a decimal Tondi amount")));
    }
    if fractional.len() > DECIMALS || !fractional.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::bad_request(format!(
            "{field} supports at most {DECIMALS} decimal places"
        )));
    }

    let whole = whole
        .parse::<u64>()
        .map_err(|err| ApiError::bad_request(format!("{field} whole value is invalid: {err}")))?;
    let whole_sats = whole
        .checked_mul(SCALE)
        .ok_or_else(|| ApiError::bad_request(format!("{field} exceeds the supported range")))?;
    let mut frac_text = fractional.to_owned();
    while frac_text.len() < DECIMALS {
        frac_text.push('0');
    }
    let frac_sats = if frac_text.is_empty() {
        0
    } else {
        frac_text.parse::<u64>().map_err(|err| {
            ApiError::bad_request(format!("{field} fractional value is invalid: {err}"))
        })?
    };
    whole_sats
        .checked_add(frac_sats)
        .map(Sats::from_sats)
        .ok_or_else(|| ApiError::bad_request(format!("{field} exceeds the supported range")))
}

fn parse_p2mr_issue_seal(value: &str) -> Result<Outpoint, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request("issueSeal must not be empty"));
    }
    let outpoint = value
        .split_once(":p2mr-seal:")
        .map(|(_, outpoint)| outpoint)
        .unwrap_or(value);
    Outpoint::from_str(outpoint).map_err(|err| {
        ApiError::bad_request(format!(
            "issueSeal must be a Tondi outpoint or '<network>:p2mr-seal:<outpoint>': {err}"
        ))
    })
}

fn consignment_contract_id(path: &Path) -> Result<ContractId, ApiError> {
    let file = BinFile::<CONSIGN_MAGIC_NUMBER, CONSIGN_VERSION>::open(path).map_err(|err| {
        ApiError::bad_request(format!("unable to open RGB consignment '{}': {err}", path.display()))
    })?;
    let mut reader = StrictReader::with(StreamReader::new::<{ usize::MAX }>(file));
    parse_consignment(&mut reader).map_err(|err| {
        ApiError::bad_request(format!(
            "unable to read RGB consignment header '{}': {err}",
            path.display()
        ))
    })
}

fn transfer_id_from_file(path: &Path) -> Result<String, ApiError> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "transfer file '{}' does not have a valid UTF-8 file stem",
                path.display()
            ))
        })?;
    let transfer_id = stem
        .rsplit_once(".contract-")
        .filter(|(_, suffix)| suffix.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(prefix, _)| prefix)
        .unwrap_or(stem);
    Ok(transfer_id.to_owned())
}

fn new_transfer_id() -> Result<String, ApiError> {
    let mut nonce = [0u8; 8];
    fill_random(&mut nonce)
        .map_err(|err| ApiError::internal(format!("unable to generate transfer id: {err}")))?;
    Ok(format!("transfer-{}-{}", Utc::now().format("%Y%m%dT%H%M%SZ"), hex_encode(&nonce)))
}

fn validate_wallet_id(wallet_id: &str) -> Result<(), ApiError> {
    if wallet_id.is_empty() || wallet_id.len() > 64 {
        return Err(ApiError::bad_request("wallet id must be between 1 and 64 characters"));
    }
    if !wallet_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(ApiError::bad_request(
            "wallet id may contain only ASCII letters, digits, '-' and '_'",
        ));
    }
    Ok(())
}

fn validate_recovery_phrase(words: &[String]) -> Result<(), ApiError> {
    if !matches!(words.len(), 12 | 15 | 18 | 21 | 24) {
        return Err(ApiError::bad_request(
            "recovery phrase must contain 12, 15, 18, 21, or 24 words",
        ));
    }
    if words
        .iter()
        .any(|word| word.is_empty() || !word.bytes().all(|byte| byte.is_ascii_lowercase()))
    {
        return Err(ApiError::bad_request(
            "recovery phrase words must be lowercase ASCII BIP39 words",
        ));
    }
    Ok(())
}

fn validate_passphrase(passphrase: &str) -> Result<(), ApiError> {
    if passphrase.len() < 8 || passphrase.len() > 128 {
        return Err(ApiError::bad_request("wallet passphrase must be between 8 and 128 bytes"));
    }
    if !passphrase.is_ascii() {
        return Err(ApiError::bad_request(
            "wallet passphrase must be ASCII until signer storage supports Unicode normalisation",
        ));
    }
    Ok(())
}

fn bip39_seed(words: &[String]) -> Result<[u8; 64], ApiError> {
    let phrase = words.join(" ");
    let mut seed = [0u8; 64];
    pbkdf2_hmac::<Sha512>(phrase.as_bytes(), b"mnemonic", BIP39_SEED_ROUNDS, &mut seed);
    Ok(seed)
}

fn account_from_seed(network: Network, seed: &[u8; 64]) -> XprivAccount {
    if network.is_testnet() {
        XprivAccount::with_seed(true, seed).derive(h![86, 1, 0])
    } else {
        XprivAccount::with_seed(false, seed).derive(h![86, 0, 0])
    }
}

fn input_terminal_derivation(input: &PsttInput) -> Option<Terminal> {
    let terminals = input
        .bip32_derivation
        .values()
        .flat_map(|origin| origin.as_derivation().terminal())
        .chain(
            input
                .tap_bip32_derivation
                .values()
                .flat_map(|derivation| derivation.origin.as_derivation().terminal()),
        )
        .collect::<BTreeSet<_>>();
    if terminals.len() != 1 {
        return None;
    }
    terminals.first().copied()
}

#[derive(Clone, PartialEq, Eq)]
struct UnlockedSigner {
    wallet_id: String,
    network: Network,
    seed: [u8; 64],
}

impl fmt::Debug for UnlockedSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnlockedSigner")
            .field("wallet_id", &self.wallet_id)
            .field("network", &self.network)
            .field("seed", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProfile {
    wallet_id: String,
    network: Network,
    tondi_rpc_endpoint: String,
    p2mr_address: String,
    password: PasswordVerifier,
    encrypted_seed: EncryptedSeed,
    locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EncryptedSeed {
    salt_hex: String,
    nonce_hex: String,
    ciphertext_hex: String,
}

impl EncryptedSeed {
    fn new(seed: &[u8; 64], passphrase: &str) -> Result<Self, ApiError> {
        let mut salt = [0u8; 16];
        fill_random(&mut salt).map_err(|err| {
            ApiError::internal(format!("unable to generate seed encryption salt: {err}"))
        })?;
        let mut nonce = [0u8; 24];
        fill_random(&mut nonce).map_err(|err| {
            ApiError::internal(format!("unable to generate seed encryption nonce: {err}"))
        })?;
        let key = seed_encryption_key(passphrase, &salt);
        let cipher = XChaCha20Poly1305::new((&key).into());
        let ciphertext = cipher
            .encrypt(XNonce::from_slice(&nonce), seed.as_slice())
            .map_err(|_| ApiError::internal("unable to encrypt wallet seed"))?;
        Ok(Self {
            salt_hex: hex_encode(&salt),
            nonce_hex: hex_encode(&nonce),
            ciphertext_hex: hex_encode(&ciphertext),
        })
    }

    fn decrypt(&self, passphrase: &str) -> Result<[u8; 64], ApiError> {
        let salt = hex_decode(&self.salt_hex)
            .map_err(|_| ApiError::internal("stored seed salt is not valid hex"))?;
        let nonce = hex_decode(&self.nonce_hex)
            .map_err(|_| ApiError::internal("stored seed nonce is not valid hex"))?;
        let ciphertext = hex_decode(&self.ciphertext_hex)
            .map_err(|_| ApiError::internal("stored encrypted seed is not valid hex"))?;
        if salt.len() != 16 {
            return Err(ApiError::internal("stored seed salt has invalid length"));
        }
        if nonce.len() != 24 {
            return Err(ApiError::internal("stored seed nonce has invalid length"));
        }
        let key = seed_encryption_key(passphrase, &salt);
        let cipher = XChaCha20Poly1305::new((&key).into());
        let seed = cipher
            .decrypt(XNonce::from_slice(&nonce), ciphertext.as_slice())
            .map_err(|_| {
                ApiError::bad_request("wallet passphrase could not decrypt signer seed")
            })?;
        if seed.len() != 64 {
            return Err(ApiError::internal("decrypted wallet seed has invalid length"));
        }
        let mut fixed = [0u8; 64];
        fixed.copy_from_slice(&seed);
        Ok(fixed)
    }
}

fn seed_encryption_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha512>(passphrase.as_bytes(), salt, PASSWORD_VERIFIER_ROUNDS, &mut key);
    key
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordVerifier {
    salt_hex: String,
    verifier_hex: String,
}

impl PasswordVerifier {
    fn new(passphrase: &str) -> Result<Self, ApiError> {
        let mut salt = [0u8; 16];
        fill_random(&mut salt).map_err(|err| {
            ApiError::internal(format!("unable to generate password salt: {err}"))
        })?;
        let verifier = password_hash(passphrase, &salt);
        Ok(Self {
            salt_hex: hex_encode(&salt),
            verifier_hex: hex_encode(&verifier),
        })
    }

    fn verify(&self, passphrase: &str) -> bool {
        let Ok(salt) = hex_decode(&self.salt_hex) else {
            return false;
        };
        let Ok(expected) = hex_decode(&self.verifier_hex) else {
            return false;
        };
        let actual = password_hash(passphrase, &salt);
        constant_time_eq(expected.as_slice(), actual.as_slice())
    }
}

fn password_hash(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut verifier = [0u8; 32];
    pbkdf2_hmac::<Sha512>(passphrase.as_bytes(), salt, PASSWORD_VERIFIER_ROUNDS, &mut verifier);
    verifier
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_decode(value: &str) -> Result<Vec<u8>, ()> {
    if value.len() % 2 != 0 {
        return Err(());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let high = hex_nibble(chunk[0])?;
            let low = hex_nibble(chunk[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    error: &'static str,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: "bad_request",
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            error: "wallet_conflict",
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: "internal_error",
            message: message.into(),
        }
    }

    fn locked() -> Self {
        Self {
            status: StatusCode::LOCKED,
            error: "wallet_locked",
            message: "The RGB-on-Tondi wallet profile is locked.".to_owned(),
        }
    }

    fn not_found(error: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error,
            message: message.into(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            error: "service_unavailable",
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status;
        let body = Json(ApiErrorBody { error: self.error, message: self.message });
        (status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiErrorBody {
    error: &'static str,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_dashboard_is_not_dev_preview() {
        let state = AppState::new(HttpServerConfig {
            data_dir: PathBuf::from("/tmp/rgb-http-test"),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        let dashboard = state.unavailable_dashboard(WalletLifecycle::NotCreated, "not configured");

        assert_eq!(dashboard.profile.network, TondiNetwork::Devnet);
        assert_eq!(dashboard.profile.lifecycle, WalletLifecycle::NotCreated);
        assert_eq!(dashboard.service_mode, DashboardServiceMode::Unavailable);
        assert!(dashboard.assets.is_empty());
        assert!(dashboard.transfers.is_empty());
    }

    #[test]
    fn missing_transfer_record_is_bad_request() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        let err = state
            .load_transfer_record("missing")
            .expect_err("missing record must fail");

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn malformed_transfer_record_is_explicit_dashboard_error() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        let transfer_dir = state.config.data_dir.join("transfers");
        fs::create_dir_all(&transfer_dir).expect("transfer dir");
        fs::write(
            transfer_dir.join("transfer-corrupt.json"),
            r#"{"id":"transfer-corrupt","contractTransfers":[{"contractId":"contract:test"}]}"#,
        )
        .expect("write corrupt transfer");

        let err = state
            .load_transfer_records()
            .expect_err("malformed transfer record must fail explicitly");

        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(err.message.contains("transfer record"));
        assert!(err.message.contains("invalid"));
    }

    #[test]
    fn asset_metadata_survives_dashboard_rebuild() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        let mut asset = RgbAsset {
            contract_id: "contract:asset_metadata_test".to_owned(),
            ticker: "META".to_owned(),
            name: "Metadata Persist Asset".to_owned(),
            precision: 2,
            balance: "12345".to_owned(),
            issued_supply: Some("12345".to_owned()),
            contract_consignment_ref: Some("contracts/asset_metadata_test.rgb".to_owned()),
            icon_url: None,
            issuer: "ssi:anonymous".to_owned(),
            validation: ValidationState::Valid,
            last_transition: None,
        };

        state
            .save_asset_metadata(&asset)
            .expect("save asset metadata");
        let metadata = state.load_asset_metadata().expect("load asset metadata");
        let persisted = metadata
            .get("contract:asset_metadata_test")
            .expect("persisted metadata");

        asset.ticker = "FungibleToken".to_owned();
        asset.name = "META".to_owned();
        asset.precision = 0;
        asset.issued_supply = None;
        asset.contract_consignment_ref = None;
        state.apply_asset_metadata(&mut asset, persisted);

        assert_eq!(asset.ticker, "META");
        assert_eq!(asset.name, "Metadata Persist Asset");
        assert_eq!(asset.precision, 2);
        assert_eq!(asset.issued_supply.as_deref(), Some("12345"));
        assert_eq!(
            asset.contract_consignment_ref.as_deref(),
            Some("contracts/asset_metadata_test.rgb")
        );
    }

    #[test]
    fn finalise_transfer_reports_missing_pstt_as_bad_request() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        state
            .create_wallet(CreateWalletInput {
                wallet_id: "alice".to_owned(),
                network: TondiNetwork::Devnet,
                tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
                passphrase: "avato-devnet-passphrase".to_owned(),
                recovery_phrase: "abandon abandon abandon abandon abandon abandon abandon abandon \
                                  abandon abandon abandon about"
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
            })
            .expect("create wallet");
        let transfer = RgbTransfer {
            id: "transfer-missing-pstt".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-missing-pstt.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-missing-pstt.pstt".to_owned(),
            broadcast_txid: None,
            state: TransferState::ConsignmentReady,
            validation: ValidationState::Pending,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };
        state
            .save_transfer_record(&transfer)
            .expect("save transfer");
        fs::create_dir_all(state.config.data_dir.join("transfers")).expect("transfer dir");
        fs::write(
            state
                .config
                .data_dir
                .join("transfers")
                .join("transfer-missing-pstt.rgb"),
            b"not a real consignment",
        )
        .expect("write placeholder consignment");

        let err = state
            .finalise_transfer(FinaliseTransferInput {
                pstt_ref: "transfers/transfer-missing-pstt.pstt".to_owned(),
                broadcast: false,
            })
            .expect_err("missing PSTT must fail");

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("unable to open PSTT"));
    }

    #[test]
    fn network_mapping_preserves_devnet() {
        assert_eq!(TondiNetwork::from(Network::Devnet), TondiNetwork::Devnet);
    }

    #[test]
    fn password_verifier_rejects_wrong_passphrase() {
        let verifier = PasswordVerifier::new("correct horse battery staple").expect("verifier");

        assert!(verifier.verify("correct horse battery staple"));
        assert!(!verifier.verify("wrong horse battery staple"));
    }

    #[test]
    fn accepted_transfer_contract_entries_are_merged() {
        let mut transfers = vec![RgbTransferContract {
            contract_id: "contract:z".to_owned(),
            amount: "5".to_owned(),
            consignment_ref: "z.rgb".to_owned(),
        }];

        upsert_contract_transfer(&mut transfers, RgbTransferContract {
            contract_id: "contract:a".to_owned(),
            amount: "7".to_owned(),
            consignment_ref: "a.rgb".to_owned(),
        });
        upsert_contract_transfer(&mut transfers, RgbTransferContract {
            contract_id: "contract:z".to_owned(),
            amount: "9".to_owned(),
            consignment_ref: "z-new.rgb".to_owned(),
        });

        assert_eq!(transfers.len(), 2);
        assert_eq!(transfers[0].contract_id, "contract:a");
        assert_eq!(transfers[1].amount, "9");
        assert_eq!(transfers[1].consignment_ref, "z-new.rgb");
    }

    #[test]
    fn accepted_transfer_record_is_idempotent_for_same_consignment() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        let transfer = RgbTransfer {
            id: "transfer-test".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "42".to_owned(),
                consignment_ref: "/tmp/test.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: String::new(),
            broadcast_txid: None,
            state: TransferState::ReceiverAccepted,
            validation: ValidationState::Valid,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };
        state.save_transfer_record(&transfer).expect("save record");

        let recovered = state
            .idempotent_accepted_transfer("transfer-test", "contract:test", "/tmp/test.rgb")
            .expect("load idempotent record")
            .expect("same consignment should be idempotent");
        assert_eq!(recovered, transfer);
        assert!(state
            .idempotent_accepted_transfer("transfer-test", "contract:other", "/tmp/test.rgb")
            .expect("load record")
            .is_none());
    }

    #[test]
    fn finalise_request_idempotency_depends_on_terminal_record_state() {
        let mut transfer = RgbTransfer {
            id: "transfer-test".to_owned(),
            contract_transfers: vec![],
            invoices: vec![],
            pstt_ref: "transfers/transfer-test.pstt".to_owned(),
            broadcast_txid: None,
            state: TransferState::ConsignmentReady,
            validation: ValidationState::Pending,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };
        assert!(!finalise_request_is_idempotent(&transfer, false));
        assert!(!finalise_request_is_idempotent(&transfer, true));

        transfer.state = TransferState::Finalised;
        assert!(finalise_request_is_idempotent(&transfer, false));
        assert!(!finalise_request_is_idempotent(&transfer, true));

        transfer.state = TransferState::Broadcast;
        assert!(finalise_request_is_idempotent(&transfer, false));
        assert!(!finalise_request_is_idempotent(&transfer, true));

        transfer.broadcast_txid = Some("abc123".to_owned());
        assert!(finalise_request_is_idempotent(&transfer, true));
    }

    #[test]
    fn live_outgoing_transfer_blocks_new_pay_until_broadcast_is_known() {
        let mut transfer = RgbTransfer {
            id: "transfer-test".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-test.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-test.pstt".to_owned(),
            broadcast_txid: None,
            state: TransferState::ConsignmentReady,
            validation: ValidationState::Pending,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };

        assert!(live_outgoing_transfer_without_broadcast(&transfer));

        transfer.state = TransferState::Finalised;
        assert!(live_outgoing_transfer_without_broadcast(&transfer));

        transfer.state = TransferState::Broadcast;
        assert!(live_outgoing_transfer_without_broadcast(&transfer));

        transfer.broadcast_txid = Some("abc123".to_owned());
        assert!(!live_outgoing_transfer_without_broadcast(&transfer));

        transfer.broadcast_txid = None;
        transfer.pstt_ref.clear();
        transfer.state = TransferState::ReceiverAccepted;
        assert!(!live_outgoing_transfer_without_broadcast(&transfer));
    }

    #[test]
    fn live_outgoing_transfer_guard_reports_conflict() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        state
            .create_wallet(CreateWalletInput {
                wallet_id: "alice".to_owned(),
                network: TondiNetwork::Devnet,
                tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
                passphrase: "avato-devnet-passphrase".to_owned(),
                recovery_phrase: "abandon abandon abandon abandon abandon abandon abandon abandon \
                                  abandon abandon abandon about"
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
            })
            .expect("create wallet");
        let profile = state.load_profile().expect("load profile");
        state
            .save_transfer_record(&RgbTransfer {
                id: "transfer-live".to_owned(),
                contract_transfers: vec![RgbTransferContract {
                    contract_id: "contract:test".to_owned(),
                    amount: "1".to_owned(),
                    consignment_ref: "transfers/transfer-live.rgb".to_owned(),
                }],
                invoices: vec![],
                pstt_ref: "transfers/transfer-live.pstt".to_owned(),
                broadcast_txid: None,
                state: TransferState::ConsignmentReady,
                validation: ValidationState::Pending,
                updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
            })
            .expect("save transfer record");

        let err = state
            .ensure_no_live_outgoing_transfer(&profile, "prepare payment")
            .expect_err("live outgoing transfer must block new pay");

        assert_eq!(err.status, StatusCode::CONFLICT);
        assert!(err.message.contains("transfer-live"));
        assert!(err.message.contains("broadcastTxid"));
    }

    #[test]
    fn tentative_broadcast_transfer_blocks_new_pay_until_public_status_resolves() {
        let mut transfer = RgbTransfer {
            id: "transfer-broadcast".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-broadcast.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-broadcast.pstt".to_owned(),
            broadcast_txid: Some("abc123".to_owned()),
            state: TransferState::Broadcast,
            validation: ValidationState::Pending,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };

        assert!(
            live_outgoing_broadcast_status_blocks_new_pay(&transfer, WitnessStatus::Tentative,)
        );
        assert!(!live_outgoing_broadcast_status_blocks_new_pay(
            &transfer,
            WitnessStatus::Mined(std::num::NonZeroU64::new(42).unwrap()),
        ));
        assert!(!live_outgoing_broadcast_status_blocks_new_pay(&transfer, WitnessStatus::Genesis,));
        assert!(
            !live_outgoing_broadcast_status_blocks_new_pay(&transfer, WitnessStatus::Archived,)
        );

        transfer.broadcast_txid = None;
        assert!(!live_outgoing_broadcast_status_blocks_new_pay(
            &transfer,
            WitnessStatus::Tentative,
        ));
    }

    #[test]
    fn broadcast_recovery_only_uses_live_public_statuses() {
        let mut transfer = RgbTransfer {
            id: "transfer-test".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-test.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-test.pstt".to_owned(),
            broadcast_txid: None,
            state: TransferState::Finalised,
            validation: ValidationState::Pending,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };

        assert!(!recover_broadcast_transfer(
            &mut transfer,
            "archived-txid",
            WitnessStatus::Archived,
        ));
        assert_eq!(transfer.broadcast_txid, None);
        assert_eq!(transfer.state, TransferState::Finalised);

        assert!(recover_broadcast_transfer(
            &mut transfer,
            "tentative-txid",
            WitnessStatus::Tentative,
        ));
        assert_eq!(transfer.broadcast_txid.as_deref(), Some("tentative-txid"));
        assert_eq!(transfer.state, TransferState::Broadcast);
        assert_eq!(transfer.validation, ValidationState::Pending);

        let mut mined = transfer.clone();
        mined.broadcast_txid = None;
        mined.state = TransferState::Finalised;
        assert!(recover_broadcast_transfer(
            &mut mined,
            "mined-txid",
            WitnessStatus::Mined(std::num::NonZeroU64::new(42).unwrap()),
        ));
        assert_eq!(mined.broadcast_txid.as_deref(), Some("mined-txid"));
        assert_eq!(mined.validation, ValidationState::Valid);
    }

    #[test]
    fn broadcast_status_refresh_marks_archived_as_rejected() {
        let mut transfer = RgbTransfer {
            id: "transfer-broadcast".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-broadcast.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-broadcast.pstt".to_owned(),
            broadcast_txid: Some("abc123".to_owned()),
            state: TransferState::Broadcast,
            validation: ValidationState::Pending,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };

        assert!(refresh_broadcast_transfer_status(&mut transfer, WitnessStatus::Archived,));
        assert_eq!(transfer.broadcast_txid.as_deref(), Some("abc123"));
        assert_eq!(transfer.state, TransferState::Broadcast);
        assert_eq!(transfer.validation, ValidationState::Rejected);
        assert_ne!(transfer.updated_at, "2026-07-01T00:00:00.000Z");
        assert!(!refresh_broadcast_transfer_status(&mut transfer, WitnessStatus::Archived,));
    }

    #[test]
    fn broadcast_status_refresh_marks_mined_as_valid() {
        let mut transfer = RgbTransfer {
            id: "transfer-broadcast".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-broadcast.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-broadcast.pstt".to_owned(),
            broadcast_txid: Some("abc123".to_owned()),
            state: TransferState::Broadcast,
            validation: ValidationState::Pending,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };

        assert!(refresh_broadcast_transfer_status(
            &mut transfer,
            WitnessStatus::Mined(std::num::NonZeroU64::new(42).unwrap()),
        ));
        assert_eq!(transfer.validation, ValidationState::Valid);
        assert!(!refresh_broadcast_transfer_status(
            &mut transfer,
            WitnessStatus::Mined(std::num::NonZeroU64::new(42).unwrap()),
        ));

        transfer.validation = ValidationState::Valid;
        transfer.state = TransferState::Finalised;
        assert!(!refresh_broadcast_transfer_status(&mut transfer, WitnessStatus::Archived,));
        assert_eq!(transfer.validation, ValidationState::Valid);
    }

    #[test]
    fn broadcast_status_refresh_does_not_downgrade_valid_to_pending() {
        let mut transfer = RgbTransfer {
            id: "transfer-broadcast".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-broadcast.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-broadcast.pstt".to_owned(),
            broadcast_txid: Some("abc123".to_owned()),
            state: TransferState::Broadcast,
            validation: ValidationState::Valid,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };

        assert!(!refresh_broadcast_transfer_status(&mut transfer, WitnessStatus::Tentative,));
        assert_eq!(transfer.validation, ValidationState::Valid);
        assert_eq!(transfer.updated_at, "2026-07-01T00:00:00.000Z");
    }

    #[test]
    fn broadcast_status_refresh_rejects_valid_record_when_archived() {
        let mut transfer = RgbTransfer {
            id: "transfer-broadcast".to_owned(),
            contract_transfers: vec![RgbTransferContract {
                contract_id: "contract:test".to_owned(),
                amount: "1".to_owned(),
                consignment_ref: "transfers/transfer-broadcast.rgb".to_owned(),
            }],
            invoices: vec![],
            pstt_ref: "transfers/transfer-broadcast.pstt".to_owned(),
            broadcast_txid: Some("abc123".to_owned()),
            state: TransferState::Broadcast,
            validation: ValidationState::Valid,
            updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
        };

        assert!(refresh_broadcast_transfer_status(&mut transfer, WitnessStatus::Archived,));
        assert_eq!(transfer.validation, ValidationState::Rejected);
        assert_ne!(transfer.updated_at, "2026-07-01T00:00:00.000Z");
    }

    #[test]
    fn broadcast_recovery_file_scan_marks_removed_transfer_rejected() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        let txid = "1111111111111111111111111111111111111111111111111111111111111111";
        state
            .save_transfer_record(&RgbTransfer {
                id: "transfer-removed".to_owned(),
                contract_transfers: vec![RgbTransferContract {
                    contract_id: "contract:test".to_owned(),
                    amount: "1".to_owned(),
                    consignment_ref: "transfers/transfer-removed.rgb".to_owned(),
                }],
                invoices: vec![],
                pstt_ref: "transfers/transfer-removed.pstt".to_owned(),
                broadcast_txid: Some(txid.to_owned()),
                state: TransferState::Broadcast,
                validation: ValidationState::Pending,
                updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
            })
            .expect("save transfer");

        state
            .recover_broadcast_transfer_records_with_status(|actual_txid| {
                assert_eq!(actual_txid.to_string(), txid);
                Ok(WitnessStatus::Archived)
            })
            .expect("recover transfer statuses");

        let recovered = state
            .load_transfer_record("transfer-removed")
            .expect("load transfer");
        assert_eq!(recovered.broadcast_txid.as_deref(), Some(txid));
        assert_eq!(recovered.state, TransferState::Broadcast);
        assert_eq!(recovered.validation, ValidationState::Rejected);
        assert_ne!(recovered.updated_at, "2026-07-01T00:00:00.000Z");
    }

    #[test]
    fn broadcast_recovery_file_scan_keeps_accepted_transfer_valid_during_reorg_observation() {
        let state = AppState::new(HttpServerConfig {
            data_dir: std::env::temp_dir().join(new_transfer_id().expect("transfer id")),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });
        let txid = "2222222222222222222222222222222222222222222222222222222222222222";
        state
            .save_transfer_record(&RgbTransfer {
                id: "transfer-valid".to_owned(),
                contract_transfers: vec![RgbTransferContract {
                    contract_id: "contract:test".to_owned(),
                    amount: "1".to_owned(),
                    consignment_ref: "transfers/transfer-valid.rgb".to_owned(),
                }],
                invoices: vec![],
                pstt_ref: "transfers/transfer-valid.pstt".to_owned(),
                broadcast_txid: Some(txid.to_owned()),
                state: TransferState::Broadcast,
                validation: ValidationState::Valid,
                updated_at: "2026-07-01T00:00:00.000Z".to_owned(),
            })
            .expect("save transfer");

        state
            .recover_broadcast_transfer_records_with_status(|actual_txid| {
                assert_eq!(actual_txid.to_string(), txid);
                Ok(WitnessStatus::Tentative)
            })
            .expect("recover transfer statuses");

        let recovered = state
            .load_transfer_record("transfer-valid")
            .expect("load transfer");
        assert_eq!(recovered.broadcast_txid.as_deref(), Some(txid));
        assert_eq!(recovered.state, TransferState::Broadcast);
        assert_eq!(recovered.validation, ValidationState::Valid);
        assert_eq!(recovered.updated_at, "2026-07-01T00:00:00.000Z");
    }

    #[test]
    fn relative_input_files_stay_under_data_dir() {
        let state = AppState::new(HttpServerConfig {
            data_dir: PathBuf::from("/tmp/rgb-http-test"),
            network: Network::Devnet,
            min_confirmations: 1,
            tondi_rpc_endpoint: "127.0.0.1:17110".to_owned(),
        });

        assert_eq!(
            state
                .resolve_input_file("imports/contract.rgb")
                .expect("valid file"),
            PathBuf::from("/tmp/rgb-http-test/imports/contract.rgb")
        );
        assert_eq!(
            state
                .resolve_input_file("/var/tmp/contract.rgb")
                .expect("absolute file allowed"),
            PathBuf::from("/var/tmp/contract.rgb")
        );
        assert!(state.resolve_input_file("../contract.rgb").is_err());
    }

    #[test]
    fn amount_parsers_are_integer_safe() {
        assert_eq!(parse_u64_amount("1000000", "issuedSupply").unwrap(), 1_000_000);
        assert!(parse_u64_amount("1.5", "issuedSupply").is_err());

        assert_eq!(parse_tondi_amount("0.0001", "feeTondi").unwrap().sats(), 10_000);
        assert_eq!(format_tondi_sats(10_000), "0.00010000");
        assert_eq!(format_tondi_sats(123_456_789), "1.23456789");
        assert_eq!(
            parse_tondi_amount("1.23456789", "carrierValueTondi")
                .unwrap()
                .sats(),
            123_456_789
        );
        assert!(parse_tondi_amount("1.234567891", "feeTondi").is_err());
    }

    #[test]
    fn rgb_names_and_p2mr_seals_are_strictly_parsed() {
        assert_eq!(parse_asset_ticker(" TEST ").unwrap(), "TEST");
        assert!(parse_asset_ticker("test").is_err());
        assert_eq!(parse_state_name("amount").unwrap().to_string(), "amount");
        assert!(parse_state_name("Amount").is_err());

        let txid = "1111111111111111111111111111111111111111111111111111111111111111";
        assert_eq!(
            parse_p2mr_issue_seal(&format!("devnet:p2mr-seal:{txid}:0"))
                .unwrap()
                .to_string(),
            format!("{txid}:0")
        );
    }

    #[test]
    fn bip39_seed_uses_standard_empty_passphrase_vector() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                     abandon abandon about"
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let seed = bip39_seed(&words).expect("seed");

        assert_eq!(
            hex_encode(&seed),
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        );
    }
}
