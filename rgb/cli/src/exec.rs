// Wallet Library for RGB smart contracts
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 LNP/BP Laboratories,
//                         Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
// Copyright (C) 2025 RGB Consortium, Switzerland.
// Copyright (C) 2019-2025 Dr Maxim Orlovsky.
// All rights under the above copyrights are reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

extern crate alloc;

use std::convert::Infallible;
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;

use amplify::hex::{FromHex, ToHex};
use anyhow::Context;
use bpstd::pstt::TxParams;
use bpstd::{
    ConsensusEncode, Idx, NormalIndex, Outpoint, Pstt, PsttVer, Terminal, XprivAccount,
    XpubDerivable,
};
use rgb::invoice::{RgbBeneficiary, RgbInvoice};
use rgb::popls::bp::{PaymentScript, PrefabBundle, WalletProvider};
use rgb::{CallScope, Consensus, CreateParams, Issuer};
use rgbp::descriptors::RgbDescr;
use rgbp::{ContractInfo, FileHolder};
use strict_types::StrictVal;

use crate::args::Args;
use crate::cmd::Cmd;
use crate::http_server::HttpServerConfig;
use crate::opts::{WalletOpts, DEFAULT_TONDI};

impl Args {
    pub fn exec(&self) -> anyhow::Result<()> {
        self.check_data_dir()?;
        match &self.command {
            Cmd::Init { .. } => {
                // Do nothing; the directory is already initialized in `check_data_dir`
            }

            // =====================================================================================
            // I. Wallet management
            Cmd::Wallets => {
                let data_dir = self.data_dir();
                let mut count = 0usize;
                for dir in data_dir
                    .read_dir()
                    .context("Unable to read data directory")?
                {
                    let dir = match dir {
                        Ok(dir) => dir,
                        Err(err) => {
                            eprintln!("Can't read directory entry: {err}");
                            continue;
                        }
                    };
                    let path = dir.path();
                    if dir
                        .file_type()
                        .context("Unable to read directory entry")?
                        .is_dir()
                        && path.extension() == Some("wallet".as_ref())
                    {
                        let Some(wallet_name) = path
                            .file_stem()
                            .context("Unable to parse wallet file name")?
                            .to_str()
                        else {
                            continue;
                        };
                        println!("{wallet_name}");
                        count += 1;
                    }
                }
                if count == 0 {
                    eprintln!("No wallets found");
                }
            }

            Cmd::Create { name, descriptor } => {
                let xpub = XpubDerivable::from_str(descriptor)?;
                let descriptor = RgbDescr::p2mr_unfunded(xpub, [0u8; 32]);
                let path = self.wallet_dir(Some(name));
                FileHolder::create(path.clone(), descriptor).with_context(|| {
                    format!("unable to create Tondi P2MR wallet at '{}'", path.display())
                })?;
            }

            Cmd::DevKeys { seed_hex, mainnet } => {
                let seed = decode_hex(seed_hex, "seed")?;
                let account = if *mainnet {
                    XprivAccount::with_seed(false, &seed).derive(bpstd::h![86, 0, 0])
                } else {
                    XprivAccount::with_seed(true, &seed).derive(bpstd::h![86, 1, 0])
                };
                let xpub = XpubDerivable::from(account.to_xpub_account());
                let outer0 =
                    account.derive_terminal_secret_key(Terminal::new(0u8, NormalIndex::ZERO));
                let inner0 =
                    account.derive_terminal_secret_key(Terminal::change(NormalIndex::ZERO));

                println!("xpriv={account}");
                println!("xpub={xpub}");
                println!("outer0_tondi_p2mr_secret_key={}", outer0.secret_bytes().to_hex());
                println!("inner0_tondi_p2mr_secret_key={}", inner0.secret_bytes().to_hex());
            }

            Cmd::DevIssuer { source, force, output } => {
                let source = source.clone().unwrap_or_else(default_dev_issuer_source);
                let issuer = Issuer::load(&source, |_, _, _| Result::<_, Infallible>::Ok(()))
                    .with_context(|| {
                        format!("unable to generate development issuer from '{}'", source.display())
                    })?;
                if output.exists() {
                    anyhow::ensure!(
                        *force,
                        "issuer output '{}' already exists; pass --force to overwrite it",
                        output.display()
                    );
                    fs::remove_file(output)?;
                }
                issuer.save(output)?;
                println!("issuer_file={}", output.display());
                println!("codex_id={}", issuer.codex_id());
            }

            Cmd::Sync { wallet, resolver } => {
                let mut runtime = self.runtime(&WalletOpts::with(wallet, resolver));

                let rt = tokio::runtime::Runtime::new().expect("Unable to create tokio runtime");
                rt.block_on(async { runtime.update_async(self.min_confirmations).await })?;

                println!();
            }

            Cmd::Fund { wallet } => {
                let mut runtime = self.runtime(&WalletOpts::default_with_name(wallet));
                let addr = runtime
                    .wallet
                    .next_tondi_p2mr_address()
                    .map_err(anyhow::Error::msg)?;
                println!("{addr}");
            }

            Cmd::Seals { wallet } => {
                let runtime = self.runtime(wallet);
                for utxo in runtime.wallet.utxos() {
                    println!("{utxo}");
                }
            }

            Cmd::ServeHttp { bind } => {
                let config = HttpServerConfig {
                    data_dir: self.data_dir(),
                    network: self.network,
                    min_confirmations: self.min_confirmations,
                    tondi_rpc_endpoint: self
                        .resolver
                        .tondi
                        .clone()
                        .unwrap_or_else(|| DEFAULT_TONDI.to_owned()),
                };
                let rt = tokio::runtime::Runtime::new().expect("Unable to create tokio runtime");
                rt.block_on(async { crate::http_server::serve(*bind, config).await })?;
            }

            // =====================================================================================
            // II. Contract management
            Cmd::Contracts { issuers } => {
                let contracts = self.contracts();

                if *issuers {
                    #[allow(clippy::print_literal)]
                    if contracts.issuers_count() > 0 {
                        println!("Contract issuers:");
                        println!(
                            "{:<72}\t{:<32}\t{:<16}\t{}",
                            "Codex ID", "Codex name", "Standard", "Developer"
                        );
                    } else {
                        eprintln!("No contract issuers found");
                    }
                    // TODO: Print codex and API information in separate blocks
                    for (codex_id, issuer) in contracts.issuers() {
                        let api = &issuer.default_api();
                        println!(
                            "{:<72}\t{:<32}\t{:<16}\t{}",
                            codex_id.to_string(),
                            issuer.codex_name(),
                            api.conforms()
                                .iter()
                                .map(|no| format!("RGB-{no}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                            issuer.codex().developer,
                        );
                    }
                    println!();
                }

                if contracts.contracts_count() == 0 {
                    eprintln!("No contracts found");
                } else {
                    println!("Contracts:");
                }
                for id in contracts.contract_ids() {
                    let articles = contracts.contract_articles(id);
                    let info = ContractInfo::new(id, &articles);
                    println!("---");
                    println!(
                        "{}",
                        serde_yaml::to_string(&info).context("Unable to generate YAML")?
                    );
                }
            }

            Cmd::Issue { params: None, wallet: _, quiet: _ } => {
                println!(
                    "To issue a new contract, please specify a parameter file. A contract may be \
                     issued under one of the codex listed below."
                );
                println!();
                println!("{:<32}\t{:<64}\tDeveloper", "Name", "ID");
                for (codex_id, issuer) in self.contracts().issuers() {
                    println!(
                        "{:<32}\t{codex_id}\t{}",
                        issuer.codex_name(),
                        issuer.codex().developer
                    );
                }
            }
            Cmd::Issue { params: Some(params), wallet, quiet } => {
                let mut runtime = self.runtime(&WalletOpts::default_with_name(wallet));
                let file = File::open(params).context("Unable to open the parameter file")?;
                let params = serde_yaml::from_reader::<_, CreateParams<Outpoint>>(file)?;
                match runtime.issue(params) {
                    Ok(contract_id) => println!("A new contract issued with ID {contract_id}"),
                    Err(_err) if *quiet => return Ok(()),
                    Err(err) => return Err(err.into()),
                };
            }

            Cmd::Purge { contract } => {
                self.contracts().purge(*contract)?;
            }

            Cmd::Export { force, codex, file } => {
                let contracts = self.contracts();
                let issuer = contracts
                    .issuer(*codex)
                    .ok_or(anyhow::anyhow!("unknown issuer '{codex}'"))?;
                if *force && fs::exists(file)? {
                    fs::remove_file(file)?;
                }
                issuer.save(file)?;
            }

            Cmd::Import { file: files } => {
                for src in files {
                    let mut contracts = self.contracts();
                    let Some(filename) = src.file_name() else {
                        eprintln!("Warning: '{}' is not a file, ignoring", src.display());
                        continue;
                    };
                    print!("Processing '{}' ... ", filename.to_string_lossy());

                    let issuer = Issuer::load(src, |_, _, _| Result::<_, Infallible>::Ok(()))?;
                    let codex_id = issuer.codex_id();
                    print!("codex id {codex_id} ... ");
                    if contracts.has_issuer(codex_id) {
                        println!("already known, skipping");
                        continue;
                    }
                    contracts.import_issuer(issuer)?;
                    println!("success");
                }
            }

            Cmd::Backup { force, contract, file } => {
                let contracts = self.contracts();
                let contract_id = contracts
                    .find_contract_id(contract.clone())
                    .ok_or(anyhow::anyhow!("unknown contract '{contract}'"))?;
                if *force && fs::exists(file)? {
                    fs::remove_file(file)?;
                }
                contracts.export_to_file(file, contract_id)?;
            }

            // =====================================================================================
            // III. Combined contract/wallet operations
            Cmd::State { wallet, all, global, owned, contract } => {
                let mut runtime = self.runtime(wallet);
                if wallet.sync {
                    let rt =
                        tokio::runtime::Runtime::new().expect("Unable to create tokio runtime");
                    rt.block_on(async { runtime.update_async(self.min_confirmations).await })?;
                    println!();
                }
                let contract_id = contract
                    .as_ref()
                    .map(|r| {
                        runtime
                            .contracts
                            .find_contract_id(r.clone())
                            .ok_or(anyhow::anyhow!("unknown contract '{r}'"))
                    })
                    .transpose()?;
                let contract_ids = if let Some(contract_id) = contract_id {
                    set![contract_id]
                } else {
                    runtime.contracts.contract_ids().collect()
                };
                for contract_id in contract_ids {
                    let state = if *all {
                        runtime
                            .contract_state_full(contract_id)
                            .map(|seal| seal.primary)
                    } else {
                        runtime.wallet_contract_state(contract_id)
                    };
                    let articles = runtime.contracts.contract_articles(contract_id);
                    println!("{contract_id}\t{}", articles.issue().meta.name);
                    if *global {
                        if state.immutable.is_empty() {
                            println!("Global: # no known global state");
                        } else {
                            println!(
                                "Global: {:<16}\t{:<12}\t{:<32}\t{:<32}\tRGB output",
                                "State name",
                                "Conf. height",
                                "Verifiable state",
                                "Unverifiable state"
                            );
                        }
                        for (name, map) in &state.immutable {
                            for state in map {
                                print!("\t{:<16}", name.as_str());
                                print!("\t{:<12}", state.status.to_string());
                                print!("\t{:<32}", state.data.verified.to_string());
                                if let Some(unverified) = &state.data.unverified {
                                    print!("\t{:<32}", unverified.to_string());
                                } else {
                                    print!("\t{:<32}", "~")
                                }
                                println!("\t{}", state.addr);
                            }
                        }

                        if state.aggregated.is_empty() {
                            println!("Aggr.:  # no known aggregated state");
                        } else {
                            print!("Aggr.:  {:<16}\t{:<32}", "State name", "Value");
                        }
                        for (name, val) in &state.aggregated {
                            println!("\t{name:<16}\t{val}");
                        }
                    }
                    if *owned {
                        if state.owned.is_empty() {
                            println!("Owned:  # no known owned state");
                        } else {
                            println!(
                                "Owned:  {:<16}\t{:<12}\t{:<32}\t{:<46}\tTondi outpoint",
                                "State name", "Conf. height", "Value", "RGB output"
                            );
                        }
                        for (name, map) in &state.owned {
                            for state in map {
                                print!("\t{:<16}", name.as_str());
                                print!("\t{:<12}", state.status.to_string());
                                print!("\t{:<32}", state.assignment.data.to_string());
                                print!("\t{:<46}", state.addr);
                                println!("\t{}", state.assignment.seal);
                            }
                        }
                    }
                }
            }

            Cmd::Invoice {
                wallet,
                seal_only,
                nonce,
                contract,
                api,
                method,
                state,
                value,
            } => {
                let mut runtime = self.runtime(&WalletOpts::default_with_name(wallet));
                let auth = runtime.auth_token(*nonce).ok_or(anyhow::anyhow!(
                    "Wallet has no unspent P2MR outputs; fund a Tondi P2MR wallet output first"
                ))?;
                let beneficiary = RgbBeneficiary::Token(auth);
                if *seal_only {
                    println!("{beneficiary}");
                    return Ok(());
                }

                let contract_id = if let Some(contract) = contract {
                    let id = runtime
                        .contracts
                        .find_contract_id(contract.clone())
                        .ok_or(anyhow::anyhow!("unknown contract '{contract}'"))?;
                    CallScope::ContractId(id)
                } else {
                    CallScope::ContractQuery(s!(""))
                };
                // TODO: Support parsing values with the new StrictTypes update
                let value = value.map(StrictVal::num);
                let consensus = if self.network.is_testnet() {
                    Consensus::TondiTestnet
                } else {
                    Consensus::Tondi
                };
                let mut invoice = RgbInvoice::new(
                    contract_id,
                    consensus,
                    self.network.is_testnet(),
                    beneficiary,
                    value,
                );
                if let Some(api) = api {
                    invoice = invoice.use_api(api.clone());
                }
                if let Some(method) = method {
                    invoice = invoice.use_method(method.clone());
                }
                if let Some(state) = state {
                    anyhow::ensure!(
                        method.is_some(),
                        "--state requires --method; pass the RGB API method that owns the \
                         invoiced state"
                    );
                    invoice = invoice.use_state(state.clone());
                }

                println!("{invoice}");
            }

            Cmd::Pay {
                wallet,
                strategy,
                invoice,
                sats,
                fee,
                print,
                force,
                pstt: pstt_filename,
                consignment: consignment_path,
            } => {
                let mut runtime = self.runtime(wallet);
                // TODO: sync wallet if needed
                // TODO: Add params and giveway to arguments
                let params = TxParams::with(*fee);
                let (mut pstt, payment) = runtime.pay_invoice(invoice, *strategy, params, *sats)?;
                let ver = PsttVer::V4;

                let pstt_filename = pstt_filename
                    .as_ref()
                    .unwrap_or(consignment_path)
                    .with_extension("pstt");
                let mut pstt_file = if *force {
                    File::create(pstt_filename).context("Unable to create PSTT")?
                } else {
                    File::create_new(pstt_filename)
                        .context("Unable to create PSTT; try `--force`")?
                };
                pstt.encode(ver, &mut pstt_file)?;
                if *print {
                    pstt.version = ver;
                    println!("{pstt}");
                }
                let bundle_filename = consignment_path.with_extension("bundle");
                if *force {
                    let _ = fs::remove_file(&bundle_filename);
                }
                payment
                    .bundle
                    .save(&bundle_filename)
                    .context("Unable to write prefabricated RGB operation bundle")?;
                if *force {
                    let _ = fs::remove_file(consignment_path);
                }
                runtime
                    .contracts
                    .consign_to_file(consignment_path, invoice.scope, payment.terminals)
                    .context("Unable to create the consignment file; try `--force`")?;
            }

            Cmd::Script { wallet, sats, strategy, invoice, output } => {
                let mut runtime = self.runtime(wallet);
                let script = runtime.script(invoice, *strategy, *sats)?;
                let file = File::create_new(output).context("Unable to open script file")?;
                serde_yaml::to_writer(file, &script).context("Unable to write script")?;
            }

            Cmd::Exec {
                wallet,
                print,
                script,
                fee,
                bundle: bundle_filename,
                pstt: pstt_filename,
            } => {
                let mut runtime = self.runtime(&WalletOpts::default_with_name(wallet));
                let src = File::open(script).context("Unable to open script file")?;
                let script = serde_yaml::from_reader::<_, PaymentScript>(src)?;

                let params = TxParams::with(*fee);
                let mut payment = runtime.exec(script, params)?;
                let pstt_filename = pstt_filename
                    .as_ref()
                    .unwrap_or(bundle_filename)
                    .with_extension("pstt");
                let mut pstt_file =
                    File::create_new(pstt_filename).context("Unable to create PSTT")?;

                payment
                    .bundle
                    .save(bundle_filename)
                    .context("Unable to write to the output file")?;

                // This PSTT can be sent to other payjoin parties so they add their inputs and
                // outputs, or even re-order existing ones
                let ver = PsttVer::V4;
                payment
                    .uncomit_pstt
                    .encode(ver, &mut pstt_file)
                    .context("Unable to write PSTT")?;
                if *print {
                    payment.uncomit_pstt.version = ver;
                    println!("{}", payment.uncomit_pstt);
                }
            }

            Cmd::Complete { wallet, bundle, pstt: pstt_filename } => {
                let mut runtime = self.runtime(&WalletOpts::default_with_name(wallet));
                let bundle = PrefabBundle::load(bundle)?;
                let mut pstt_file = File::open(pstt_filename).context("Unable to open PSTT")?;
                let pstt = Pstt::decode(&mut pstt_file)?;

                let pstt = runtime.complete(pstt, &bundle)?;

                let mut pstt_file = File::create(pstt_filename).context("Unable to write PSTT")?;
                pstt.encode(pstt.version, &mut pstt_file)?;
            }

            Cmd::Consign { contract, terminals, output: consignment_path } => {
                let contracts = self.contracts();
                let contract_id = contracts
                    .find_contract_id(contract.clone())
                    .ok_or(anyhow::anyhow!("unknown contract '{contract}'"))?;
                contracts
                    .consign_to_file(consignment_path, contract_id, terminals)
                    .context("Unable to consign the contract")?;
            }

            Cmd::Finalize {
                broadcast,
                wallet,
                tondi_p2mr_secret_key,
                pstt: pstt_filename,
                tx: tx_filename,
            } => {
                let mut runtime = self.runtime(wallet);
                let mut pstt_file = File::open(pstt_filename).context("Unable to open PSTT")?;
                let mut pstt = Pstt::decode(&mut pstt_file)?;

                if let Some(secret_key) = tondi_p2mr_secret_key {
                    let signed = sign_tondi_p2mr_inputs(&mut pstt, secret_key)?;
                    eprintln!("{signed} Tondi P2MR inputs signed");
                }

                anyhow::ensure!(
                    pstt.network == self.network.native_domain(),
                    "PSTT network differs from selected wallet"
                );
                let clock = native_clock(
                    wallet.resolver.tondi.as_deref().unwrap_or(DEFAULT_TONDI),
                    self.network,
                )?;

                eprint!("Extracting signed transaction ... ");
                let extracted = match pstt.extract(clock) {
                    Ok(extracted) => {
                        eprintln!("success");
                        if !*broadcast && tx_filename.is_none() {
                            println!("{extracted}");
                        }
                        if let Some(file) = tx_filename {
                            eprint!("Saving transaction to file {} ...", file.display());
                            let mut file = File::create(file)?;
                            extracted.consensus_encode(&mut file)?;
                            eprintln!("success");
                        }
                        extracted
                    }
                    Err(e) if *broadcast || tx_filename.is_some() => {
                        anyhow::bail!(
                            "PSTT still contains {} non-finalized inputs, failing to extract \
                             transaction",
                            e.0
                        );
                    }
                    Err(e) => {
                        anyhow::bail!("{} more inputs still have to be finalized", e.0);
                    }
                };

                if *broadcast {
                    let rt =
                        tokio::runtime::Runtime::new().expect("Unable to create tokio runtime");
                    rt.block_on(async { runtime.wallet.broadcast_async(&extracted, None).await })?;
                    println!("Broadcast transaction ID: {}", extracted.txid());
                }
            }

            Cmd::Accept { unknown, wallet, input } => {
                let mut runtime = self.runtime(&WalletOpts::default_with_name(wallet));
                runtime
                    .consume_from_file(*unknown, input, |_, _, _| Result::<_, Infallible>::Ok(()))
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }
        }
        Ok(())
    }
}

fn default_dev_issuer_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rgb-std/tests/data/Test.issuer")
}

/// Sign native ALL commitments using the shared wallet implementation.
pub(crate) fn sign_tondi_p2mr_inputs(pstt: &mut Pstt, secret_keys: &str) -> anyhow::Result<usize> {
    let keys = secret_keys
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(decode_secret_key)
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(!keys.is_empty(), "at least one native Auth secret key is required");
    Ok(pstt.sign(&keys)?)
}
/// Resolve the selected node's virtual clock within the lifetime of its Tokio runtime.
pub(crate) fn native_clock(
    endpoint: &str,
    network: bpstd::Network,
) -> anyhow::Result<bpstd::native::ClockContext> {
    let selected = match network {
        bpstd::Network::Mainnet => tondi::NetworkType::Mainnet,
        bpstd::Network::Testnet => tondi::NetworkType::Testnet,
        bpstd::Network::Simnet => tondi::NetworkType::Simnet,
        bpstd::Network::Devnet => tondi::NetworkType::Devnet,
    };
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let client =
            tondi::TondiClient::connect(tondi::TondiConfig::new(endpoint).with_network(selected))
                .await?;
        let result = client.clock().await;
        client.disconnect().await?;
        result.map_err(anyhow::Error::from)
    })
}

fn decode_secret_key(value: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = decode_hex(value, "Tondi P2MR secret key")?;
    anyhow::ensure!(
        bytes.len() == 32,
        "Tondi P2MR secret key must be 32 bytes, got {}",
        bytes.len()
    );
    let mut secret_key = [0u8; 32];
    secret_key.copy_from_slice(&bytes);
    Ok(secret_key)
}

fn decode_hex(value: &str, what: &str) -> anyhow::Result<Vec<u8>> {
    let value = value.trim().trim_start_matches("0x");
    Vec::<u8>::from_hex(value).with_context(|| format!("invalid {what} hex"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_secret_rejects_tweaks_and_invalid_lengths() {
        assert!(decode_secret_key(&"ab".repeat(32)).is_ok());
        assert!(decode_secret_key(&"ab".repeat(31)).is_err());
        assert!(decode_secret_key(&format!("{}:{}", "ab".repeat(32), "cd".repeat(32))).is_err());
    }
}
