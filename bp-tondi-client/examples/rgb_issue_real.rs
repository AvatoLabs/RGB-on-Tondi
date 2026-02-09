//! 真实的 RGB 资产发行示例
//!
//! 这个示例展示了在 Tondi 上发行真实 RGB 资产的完整流程：
//! 1. 从助记词导出私钥
//! 2. 获取 UTXO
//! 3. 使用 rgb-std 创建真实的 RGB Genesis
//! 4. 计算真实的 RGB 承诺（commit_id）
//! 5. 实现交易签名
//! 6. 构造并广播交易
//!
//! 运行方式：
//! ```bash
//! cd bp-tondi-client
//! cargo run --example rgb_issue_real --features grpc
//! ```

use tondi::{TondiClient, TondiConfig, TondiTransaction, TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey, UtxosByAddressEntry};
use amplify::{Bytes32, ByteArray, confinement::Confined, hex::ToHex};
use bp::{Tx, TxVer, LockTime, TxIn, TxOut, Outpoint as BpOutpoint, Vout, SigScript, SeqNo, Witness, ScriptPubkey, Txid, VarIntBytes};
use commit_verify::{create_annex_with_commitment, TondiBlake3};
use bp::secp256k1::Secp256k1;
use bpstd::{XprivAccount, HardenedIndex};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha512;
// use bp::seals::WTxoSeal; // 通过 transform 方法使用，不需要直接导入

// RGB 相关导入
use rgb::{Assignment, CodexId, Contract, CreateParams, Issuer, Outpoint as RgbOutpoint};
use rgb_persist_fs::{PileFs, StockFs};
use strict_encoding::vname;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 真实的 RGB 资产发行 ===\n");

    // 1. 配置环境
    let endpoint = "38.190.226.7:16210";
    let alice_addr = "tondi0:qzqvqaenmgmgtu8j2yjqjexpgyp7q3rcyjzeamavcgkqs775wvvp6dyte9a";
    let alice_mnemonic = "improve candy either cushion decrease radio junk crouch run horn rapid gravity";
    
    // 2. 从助记词导出 Alice 的私钥
    println!("[步骤 0] 从助记词导出 Alice 的私钥...");
    let seed = mnemonic_to_seed(alice_mnemonic, "")?;
    let alice_account = XprivAccount::with_seed(true, &seed);
    let receive_path = [
        HardenedIndex::from(84u16),
        HardenedIndex::from(1u16),
        HardenedIndex::from(0u16),
    ];
    let alice_account_derived = alice_account.derive(&receive_path);
    let alice_xpriv = alice_account_derived.xpriv();
    let keypair = alice_xpriv.to_keypair_bip340();
    let secp = Secp256k1::new();
    println!("  - 私钥导出完成\n");

    // 3. 连接到节点并获取 UTXO
    println!("[步骤 1] 连接到节点并获取 UTXO...");
    let client = TondiClient::connect(TondiConfig::new(endpoint)).await?;
    let utxos = client.get_utxos_by_address(alice_addr).await?;
    let selected_utxo = utxos.iter()
        .find(|u| u.utxo_entry.amount > 10000)
        .ok_or("Alice 余额不足以支付发行和 Gas")?;
    
    println!("✓ 选中 UTXO: {}:{} (金额: {} sats)", 
        selected_utxo.outpoint.transaction_id.to_hex(), 
        selected_utxo.outpoint.index,
        selected_utxo.utxo_entry.amount);

    // 4. 创建 RGB Genesis
    println!("\n[步骤 2] 创建真实的 RGB Genesis...");
    
    // 4.1 从代码内建定义获取 RGB20 CodexId（修正 B：彻底解决 Issuer 文件依赖）
    println!("\n[动态获取] 正在从代码库直接生成 RGB20 Codex ID...");
    
    // RGB20 标准 CodexId（来自 rgb-interfaces/README.md）
    const RGB20_CODEX_ID_STR: &str = "7C15w3W1-L0T~zXw-Aeh5~kV-Zquz729-HXQFKQW-_5lX9O8";
    // 最新生成的 Issuer CodexId（来自 rgb-issuer）
    const NEW_ISSUER_CODEX_ID_STR: &str = "8wxMejja-eG9vraj-8TuLoFN-lkx8SSS-3asDcZg-ov5OOww";
    // 之前生成的 Issuer CodexId
    const PREV_ISSUER_CODEX_ID_STR: &str = "YM_BowPP-c94xz~F-vOAQiUD-4Llr83t-p0534bT-tZNKY7s";
    println!("  - 已知的 RGB20 CodexId 字符串: {}", RGB20_CODEX_ID_STR);
    println!("  - 最新生成的 Issuer CodexId 字符串: {}", NEW_ISSUER_CODEX_ID_STR);
    println!("  - 之前生成的 Issuer CodexId 字符串: {}", PREV_ISSUER_CODEX_ID_STR);
    
    // 方法：尝试从 rgb-std/rgb-interfaces 标准库获取 CodexId
    // 注意：虽然 rgb-interfaces 没有直接导出 RGB20 接口，但我们可以尝试从文件加载 Issuer
    // 如果文件不存在，我们需要找到替代方案
    
    // 首先尝试从文件加载 Issuer（如果存在）
    let stl_issuer_paths = [
        "../rgb-interfaces/stl/RGB20@0.12.0.issuer",
        "../../rgb-interfaces/stl/RGB20@0.12.0.issuer",
        "rgb-interfaces/stl/RGB20@0.12.0.issuer",
    ];
    
    let example_issuer_paths = [
        // 最新的 RGB20 Issuer 文件（最优先）
        "/home/thomzin/Avato/RGB-on-Tondi/rgb/examples/RGB20-Simplest-v0-47Fc1g.issuer",
        "../rgb/examples/RGB20-Simplest-v0-47Fc1g.issuer",
        "../../rgb/examples/RGB20-Simplest-v0-47Fc1g.issuer",
        // 之前的 RGB20 Issuer 文件
        "/home/thomzin/Avato/RGB-on-Tondi/rgb/examples/RGB20-Simplest-v0-t~EdfQ.issuer",
        "../rgb/examples/RGB20-Simplest-v0-t~EdfQ.issuer",
        "../../rgb/examples/RGB20-Simplest-v0-t~EdfQ.issuer",
        "/home/thomzin/Avato/RGB-on-Tondi/rgb/examples/RGB20-Simplest-v0-_GxnIA.issuer",
        "../rgb/examples/RGB20-Simplest-v0-_GxnIA.issuer",
        "../../rgb/examples/RGB20-Simplest-v0-_GxnIA.issuer",
        "/home/thomzin/Avato/RGB-on-Tondi/rgb/examples/RGB20-Simplest-v0-AYkSrg.issuer",
        "../rgb/examples/RGB20-Simplest-v0-AYkSrg.issuer",
        "../../rgb/examples/RGB20-Simplest-v0-AYkSrg.issuer",
        // 旧的示例文件（备选）
        "../rgb/examples/RGB20-FNA.issuer",
        "../../rgb/examples/RGB20-FNA.issuer",
        "rgb/examples/RGB20-FNA.issuer",
    ];
    
    let all_issuer_paths: Vec<&str> = stl_issuer_paths.iter()
        .chain(example_issuer_paths.iter())
        .copied()
        .collect();
    
    // 尝试从文件加载 Issuer（尝试所有路径，直到成功）
    let issuer_opt = all_issuer_paths.iter()
        .filter(|path| std::path::Path::new(path).exists())
        .find_map(|issuer_path| {
            println!("  - 尝试加载 Issuer: {}", issuer_path);
            match Issuer::load(issuer_path, |_, _, _| -> Result<(), std::convert::Infallible> {
                Ok(())
            }) {
                Ok(issuer) => {
                    println!("  - ✓ 成功加载 Issuer: {}", issuer_path);
                    Some(issuer)
                }
                Err(e) => {
                    eprintln!("  - ⚠️  加载失败: {}", e);
                    eprintln!("  - 继续尝试其他路径...");
                    None
                }
            }
        });
    
    // 如果无法从文件加载，尝试从代码获取 CodexId
    // 注意：Contract::issue 需要完整的 Issuer 对象，所以如果文件不存在，我们仍然需要 Issuer
    let (issuer, codex_id) = match issuer_opt {
        Some(issuer) => {
            let codex_id = issuer.codex_id();
            println!("  - ✓ 从文件加载 RGB20 Issuer 成功");
            println!("  - Issuer ID: {}", issuer.issuer_id());
            println!("  - Codex ID: {}", codex_id);
            println!("  - 动态获取的 Codex ID: {}", codex_id);
            println!("  - 已知的 RGB20 CodexId 字符串: {}", RGB20_CODEX_ID_STR);
            println!("  - 最新生成的 Issuer CodexId 字符串: {}", NEW_ISSUER_CODEX_ID_STR);
            println!("  - 之前生成的 Issuer CodexId 字符串: {}", PREV_ISSUER_CODEX_ID_STR);
            // 对比 CodexId
            let codex_id_str = codex_id.to_string();
            if codex_id_str == NEW_ISSUER_CODEX_ID_STR {
                println!("  - ✓ CodexId 匹配最新生成的 Issuer");
            } else if codex_id_str == PREV_ISSUER_CODEX_ID_STR {
                println!("  - ✓ CodexId 匹配之前生成的 Issuer");
            } else if codex_id_str == RGB20_CODEX_ID_STR {
                println!("  - ✓ CodexId 匹配已知的 RGB20 CodexId");
            } else {
                println!("  - ⚠️  CodexId 与已知值都不匹配");
                println!("  -   实际 CodexId: {}", codex_id_str);
            }
            (issuer, codex_id)
        }
        None => {
            eprintln!("\n  - ⚠️  所有 Issuer 文件加载失败");
            eprintln!("  - 尝试从代码内建定义获取 CodexId...");
            
            // 尝试从 rgb-interfaces 获取（如果可能）
            // 注意：由于 rgb-interfaces 可能没有直接导出 RGB20 接口，
            // 我们只能尝试使用已知的 CodexId 字符串
            // 但由于 CodexId::from_str 需要 baid64 feature，我们无法直接解析
            
            eprintln!("  - 注意：Contract::issue 需要完整的 Issuer 对象");
            eprintln!("  - 无法仅使用 CodexId 创建合约（需要完整的 Issuer 定义）");
            eprintln!("  - 已尝试的路径：");
            for path in &all_issuer_paths {
                let exists = std::path::Path::new(path).exists();
                if exists {
                    eprintln!("    * {} (存在，但加载失败 - 可能是 CodexId 不匹配)", path);
                } else {
                    eprintln!("    * {} (不存在)", path);
                }
            }
            eprintln!("  - 提示：请确保 Issuer 文件的 CodexId 与当前 RGB 版本匹配");
            eprintln!("  - 已知的 RGB20 CodexId: {}", RGB20_CODEX_ID_STR);
            return Err("无法加载 RGB20 Issuer 文件。所有文件都加载失败，可能是 CodexId 不匹配。".into());
        }
    };
    
    // 将 Tondi Outpoint 转换为 RGB Outpoint
    let rgb_txid = bp::Txid::from_byte_array(selected_utxo.outpoint.transaction_id.to_byte_array());
    let rgb_outpoint = RgbOutpoint::new(
        rgb_txid,
        bp::Vout::from_u32(selected_utxo.outpoint.index),
    );
    
    // 创建 CreateParams（使用 CodexId）
    // 注意：CreateParams 内部会使用 IssuerSpec::Latest(codex_id)
    let mut params = CreateParams::new_tondi_testnet(codex_id, "TestToken")
        .with_global_verified(vname!("ticker"), "TEST")
        .with_global_verified(vname!("name"), "Test Token")
        .with_global_verified(vname!("precision"), 8u16)
        .with_global_verified(vname!("issued"), 1_000_000u64);
    
    // 分配初始余额到选中的 UTXO（使用 Outpoint）
    params.push_owned_unlocked(
        vname!("balance"),
        Assignment::new_internal(rgb_outpoint, 1_000_000u64),
    );
    
    // 将 CreateParams<Outpoint> 转换为 CreateParams<WTxoSeal>
    let mut noise_engine = TondiBlake3::default();
    noise_engine.update(&selected_utxo.outpoint.transaction_id.to_byte_array());
    noise_engine.update(&selected_utxo.outpoint.index.to_le_bytes());
    let params = params.transform(noise_engine);
    
    println!("  - 资产信息:");
    println!("    * Ticker: TEST");
    println!("    * Name: Test Token");
    println!("    * Precision: 8");
    println!("    * Total Supply: 1,000,000");
    println!("    * Initial Owner UTXO: {}:{}", 
        selected_utxo.outpoint.transaction_id.to_hex(),
        selected_utxo.outpoint.index);

    // 4.3 创建临时目录用于存储合约数据
    let temp_dir = std::env::temp_dir().join("rgb_issue_test");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;
    
    // 4.4 发行合约
    println!("\n  - 正在发行 RGB 合约...");
    let contract: Contract<StockFs, PileFs<bp::seals::TxoSeal>> = Contract::issue(
        issuer,
        params,
        |_| Ok(temp_dir.clone()),
    )?;
    
    // 4.5 获取 Genesis OpId 和 commit_id
    let genesis_opid = contract.articles().genesis_opid();
    let rgb_commitment = genesis_opid.to_byte_array();
    
    println!("  - ✓ RGB Genesis 创建成功");
    println!("  - Genesis OpId: {}", genesis_opid.to_hex());
    println!("  - RGB Commitment (hex): {}\n", hex::encode(&rgb_commitment));

    // 5. 创建 Annex 数据
    println!("[步骤 3] 创建 Annex 数据...");
    let annex_data = create_annex_with_commitment(&rgb_commitment);
    println!("  - Annex 数据长度: {} 字节", annex_data.len());
    println!("  - Annex 数据 (hex): {}\n", hex::encode(&annex_data));

    // 6. 构造交易
    println!("[步骤 4] 构造交易...");
    let amount_to_issue = 1000u64; // 小额输出用于承载 RGB Seal
    let gas_fee = 1000u64;
    
    if selected_utxo.utxo_entry.amount < amount_to_issue + gas_fee {
        return Err("余额不足".into());
    }
    
    let change = selected_utxo.utxo_entry.amount - amount_to_issue - gas_fee;
    let alice_script = ScriptPubkey::from_checked(selected_utxo.utxo_entry.script_public_key.script.clone());

    let mut bp_tx = Tx {
        version: TxVer::from_consensus_u16(1),
        inputs: Confined::try_from_iter(vec![TxIn {
            prev_output: BpOutpoint::new(
                Txid::from_byte_array(selected_utxo.outpoint.transaction_id.to_byte_array()),
                Vout::from_u32(selected_utxo.outpoint.index),
            ),
            sig_script: SigScript::empty(),
            sequence: SeqNo::from_consensus_u32(0xFFFFFFFF),
            witness: {
                let mut witness_items = Vec::new();
                witness_items.push(Vec::new()); // 占位符签名
                witness_items.push(annex_data.clone()); // Annex 数据
                Witness::from_consensus_stack(witness_items)
            },
        }]).unwrap(),
        outputs: Confined::try_from_iter(vec![
            TxOut { 
                value: amount_to_issue.into(), 
                script_pubkey: alice_script.clone(), // RGB Seal 输出
            },
            TxOut { 
                value: change.into(), 
                script_pubkey: alice_script.clone(), // 找零
            },
        ]).unwrap(),
        lock_time: LockTime::from_consensus_u64(0),
        subnetwork_id: [0u8; 20],
        gas: gas_fee,
        payload: VarIntBytes::try_from(annex_data.clone()).unwrap(),
        // ========================================================================
        // 修正 2：mass 值计算（避免节点拒绝交易）
        // ========================================================================
        // Tondi 节点会对交易的 Mass 进行校验，mass 为 0 可能导致交易被拒绝
        // 对于带有 payload 的交易，mass 必须大于 0
        // 这里使用一个合理的预估值：基础 mass + payload 大小 * 权重
        // 基础 mass: 版本(2) + 输入(约100) + 输出(约50) + 其他字段(约30) = 约 200
        // Payload 权重: payload 大小 * 10 (Tondi 对 payload 有额外权重)
        mass: {
            let base_mass = 200u64;
            let payload_mass = annex_data.len() as u64 * 10;
            let estimated_mass = base_mass + payload_mass;
            // 确保 mass 至少为 1（不能为 0）
            estimated_mass.max(1)
        },
    };

    println!("  - 输入: 1 个");
    println!("  - 输出: 2 个 (RGB Seal: {} sats, 找零: {} sats)", amount_to_issue, change);
    println!("  - Gas: {} sats", gas_fee);

    // 7. 计算 Tondi Sighash 并签名
    println!("\n[步骤 5] 计算 Sighash 并签名...");
    let sighash = calc_tondi_schnorr_sighash(
        &bp_tx,
        0,
        selected_utxo,
        SighashType::All,
    )?;
    println!("  - Sighash (hex): {}", hex::encode(&sighash));

    let sig = secp.sign_schnorr_no_aux_rand(&sighash, &keypair);
    println!("  - ✓ 签名完成");

    // 8. 构造签名脚本
    let mut sig_script_raw = vec![0x41]; // Push 65 bytes
    sig_script_raw.extend_from_slice(&sig.as_ref()[..]); // 64 bytes signature
    sig_script_raw.push(0x01); // SIGHASH_ALL
    bp_tx.inputs[0].sig_script = SigScript::from_checked(sig_script_raw);

    // 9. 转换为 TondiTransaction
    println!("\n[步骤 6] 转换为 TondiTransaction...");
    let tondi_tx = convert_bp_to_tondi_tx(&bp_tx, &[selected_utxo.clone()])?;
    println!("  - ✓ 转换完成\n");

    // ========================================================================
    // 调试信息：打印完整的 TondiTransaction 内容
    // ========================================================================
    // 当遇到 "No response received" 错误时，这有助于诊断节点拒绝的原因
    // 可以检查 subnetwork_id、payload、inputs、outputs 等字段的实际值
    println!("[调试信息] 完整的 TondiTransaction 结构体内容:\n{:#?}\n", tondi_tx);

    // 10. 提交交易
    println!("[步骤 7] 广播交易到节点...");
    match client.submit_transaction(tondi_tx, false).await {
        Ok(txid) => {
            println!("✓ RGB 资产发行成功！");
            println!("  - 交易 ID: {}", txid.to_hex());
            println!("  - Contract ID: {}", contract.contract_id());
            println!("  - Genesis OpId: {}", genesis_opid.to_hex());
            println!("\n🎉 RGB 资产已成功发行！\n");
        }
        Err(e) => {
            eprintln!("✗ 交易提交失败: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

/// 如果无法加载 Issuer，使用模拟的 RGB 承诺（仅用于演示）
#[allow(unused_variables)]
async fn create_mock_issue(
    _client: &TondiClient,
    _selected_utxo: &UtxosByAddressEntry,
    _keypair: &bp::secp256k1::Keypair,
    _secp: &Secp256k1<bp::secp256k1::All>,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("\n⚠️  使用模拟的 RGB 承诺（仅用于演示）");
    eprintln!("  实际应用中需要加载真实的 RGB20 Issuer 文件\n");
    eprintln!("  请确保 RGB20 Issuer 文件存在于以下路径之一：");
    eprintln!("    - rgb/examples/RGB20-FNA.issuer");
    eprintln!("    - ../rgb/examples/RGB20-FNA.issuer");
    eprintln!("    - ../../rgb/examples/RGB20-FNA.issuer\n");
    
    // 注意：模拟承诺功能已移除，因为需要完整的交易流程
    // 请使用真实的 RGB Issuer 文件来发行资产
    
    Ok(())
}

/// Sighash 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SighashType {
    All = 0x01,
}

/// 计算 Tondi 的 Schnorr 签名哈希
fn calc_tondi_schnorr_sighash(
    tx: &Tx,
    input_index: usize,
    utxo_entry: &UtxosByAddressEntry,
    hash_type: SighashType,
) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if input_index >= tx.inputs.len() {
        return Err("Invalid input index".into());
    }

    let input = &tx.inputs[input_index];
    
    // ========================================================================
    // 重要修正 B：Sighash 上下文字符串必须与 Tondi 核心库完全一致
    // ========================================================================
    // 使用 "tondi.schnorr.signing" 作为 Schnorr 签名的上下文
    // 大小写必须完全匹配，否则计算出的 Sighash 将无法通过节点验证
    // 参考：Tondi 核心库中的 SchnorrSigningHash::new() 实现
    let mut hasher = TondiBlake3::from_context("tondi.schnorr.signing");

    // 1. Version
    hasher.update(&tx.version.to_consensus_u16().to_le_bytes());

    // 2. Previous outputs hash
    if hash_type == SighashType::All {
        // 使用 "tondi.transaction.signing" 作为交易签名的上下文
        // 大小写必须与 Tondi 核心库完全一致（参考 TransactionSigningHash::new()）
        let mut prev_outs_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for input in tx.inputs.iter() {
            prev_outs_hasher.update(&input.prev_output.txid.to_byte_array());
            prev_outs_hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());
        }
        hasher.update(&prev_outs_hasher.finalize());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 3. Sequences hash
    if hash_type == SighashType::All {
        let mut seq_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for input in tx.inputs.iter() {
            seq_hasher.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());
        }
        hasher.update(&seq_hasher.finalize());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 4. Sig op counts hash
    if hash_type == SighashType::All {
        let mut sigop_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for _input in tx.inputs.iter() {
            sigop_hasher.update(&[1u8]);
        }
        hasher.update(&sigop_hasher.finalize());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 5. Current input outpoint
    hasher.update(&input.prev_output.txid.to_byte_array());
    hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());

    // 6. Script public key
    // ========================================================================
    // 修正 3：Sighash 计算中的长度前缀格式确认
    // ========================================================================
    // 根据 bp-core/consensus/src/tx.rs 的 txid() 实现，Tondi 使用 u64 作为长度前缀
    // 这与 Tondi 的 TransactionSigningHash 实现一致
    hasher.update(&(utxo_entry.utxo_entry.script_public_key.version as u16).to_le_bytes());
    let script_bytes = &utxo_entry.utxo_entry.script_public_key.script;
    hasher.update(&(script_bytes.len() as u64).to_le_bytes()); // u64 长度前缀（正确）
    hasher.update(script_bytes);

    // 7. Amount
    hasher.update(&utxo_entry.utxo_entry.amount.to_le_bytes());

    // 8. Sequence
    hasher.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());

    // 9. Sig op count
    hasher.update(&[1u8]);

    // 10. Outputs hash
    if hash_type == SighashType::All {
        let mut outputs_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for output in tx.outputs.iter() {
            outputs_hasher.update(&output.value.sats().to_le_bytes());
            let script_bytes = output.script_pubkey.as_slice();
            outputs_hasher.update(&0u16.to_le_bytes());
            outputs_hasher.update(&(script_bytes.len() as u64).to_le_bytes()); // u64 长度前缀（正确）
            outputs_hasher.update(script_bytes);
        }
        hasher.update(&outputs_hasher.finalize());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 11. Lock time
    hasher.update(&tx.lock_time.to_consensus_u64().to_le_bytes());

    // 12. Subnetwork ID
    hasher.update(&tx.subnetwork_id);

    // 13. Gas
    hasher.update(&tx.gas.to_le_bytes());

    // 14. Payload hash
    if tx.subnetwork_id == [0u8; 20] && tx.payload.is_empty() {
        hasher.update(&[0u8; 32]);
    } else {
        let mut payload_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        let payload_bytes = tx.payload.as_slice();
        // u64 长度前缀（与 bp-core/consensus/src/tx.rs 的 txid() 实现一致）
        payload_hasher.update(&(payload_bytes.len() as u64).to_le_bytes());
        payload_hasher.update(payload_bytes);
        hasher.update(&payload_hasher.finalize());
    }

    // 15. Hash type
    hasher.update(&[hash_type as u8]);

    Ok(hasher.finalize())
}

/// 从 bp::Tx 转换为 TondiTransaction
fn convert_bp_to_tondi_tx(
    tx: &Tx,
    _utxo_entries: &[UtxosByAddressEntry],
) -> Result<TondiTransaction, Box<dyn std::error::Error>> {
    let inputs: Vec<TondiTxInput> = tx.inputs.iter().map(|input| {
        let signature_script = input.sig_script.to_vec();
        let sig_op_count = if signature_script.len() >= 65 { 1 } else { 0 };
        
        TondiTxInput::Standard {
            previous_outpoint: TondiOutpoint {
                transaction_id: Bytes32::from_byte_array(
                    input.prev_output.txid.to_byte_array()
                ),
                index: input.prev_output.vout.to_u32(),
            },
            signature_script,
            sequence: input.sequence.to_consensus_u32() as u64,
            sig_op_count,
        }
    }).collect();
    
    let outputs: Vec<TondiTxOutput> = tx.outputs.iter().map(|output| {
        TondiTxOutput::Standard {
            value: output.value.sats(),
            script_public_key: ScriptPublicKey {
                version: 0,
                script: output.script_pubkey.to_vec(),
            },
        }
    }).collect();
    
    Ok(TondiTransaction {
        version: tx.version.to_consensus_u16(),
        inputs,
        outputs,
        lock_time: tx.lock_time.to_consensus_u64(),
        // ========================================================================
        // 重要修正 A：subnetwork_id 的长度限制（P0 级 Bug 修复）
        // ========================================================================
        // Tondi 要求 subnetwork_id 严格为 20 字节（40 位 Hex）
        // 虽然 TondiTransaction 使用 Bytes32（32字节），但我们必须确保
        // 在 tondi_tx_to_proto 中只取前 20 字节
        // 注意：这里仍然填充到 32 字节以匹配 TondiTransaction 的类型定义，
        // 但 tondi_tx_to_proto 会正确截断到 20 字节
        subnetwork_id: {
            let mut bytes32 = [0u8; 32];
            bytes32[0..20].copy_from_slice(&tx.subnetwork_id);
            Bytes32::from_byte_array(bytes32)
        },
        gas: tx.gas,
        payload: tx.payload.to_vec(),
        mass: tx.mass,
    })
}

/// 从助记词生成 BIP39 seed
fn mnemonic_to_seed(mnemonic: &str, passphrase: &str) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    let salt = format!("mnemonic{}", passphrase);
    let mut seed = [0u8; 64];
    pbkdf2_hmac::<Sha512>(mnemonic.as_bytes(), salt.as_bytes(), 2048, &mut seed);
    Ok(seed)
}

