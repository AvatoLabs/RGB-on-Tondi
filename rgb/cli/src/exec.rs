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

use std::convert::Infallible;
use std::fs;
use std::fs::File;
use std::str::FromStr;

use anyhow::Context;
use bpstd::pstt::{PsttConstructor, TxParams};
use bpstd::{ConsensusEncode, Outpoint, Pstt, PsttVer, XpubDerivable};
use rgb::invoice::{RgbBeneficiary, RgbInvoice};
use rgb::popls::bp::{PaymentScript, PrefabBundle, WalletProvider};
use rgb::{CallScope, Consensus, CreateParams, Issuer};
use rgbp::descriptors::RgbDescr;
use rgbp::ContractInfo;
use rgbp::FileHolder;
use strict_types::StrictVal;

use crate::args::Args;
use crate::cmd::Cmd;
use crate::opts::WalletOpts;

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
                pstt2,
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
                let ver = if *pstt2 { PsttVer::V2 } else { PsttVer::V0 };

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
                pstt2,
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
                let ver = if *pstt2 { PsttVer::V2 } else { PsttVer::V0 };
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

            Cmd::Finalize { broadcast, wallet, pstt: pstt_filename, tx: tx_filename } => {
                let mut runtime = self.runtime(wallet);
                let mut pstt_file = File::open(pstt_filename).context("Unable to open PSTT")?;
                let mut pstt = Pstt::decode(&mut pstt_file)?;

                for input in pstt.inputs_mut() {
                    if input.final_script_sig.is_none() {
                        input.final_script_sig = Some(Default::default());
                    }
                }

                let inputs = pstt.finalize(runtime.wallet.descriptor());
                eprint!("{inputs} of {} inputs were finalized", pstt.inputs().count());
                if pstt.is_finalized() {
                    eprintln!(", transaction is ready for the extraction");
                } else {
                    eprintln!(" and some non-finalized inputs remain");
                }

                eprint!("Extracting signed transaction ... ");
                let extracted = match pstt.extract() {
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
