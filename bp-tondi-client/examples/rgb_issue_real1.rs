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
use commit_verify::create_annex_with_commitment;
use bp::secp256k1::Secp256k1;
use bpstd::{XprivAccount, HardenedIndex};
use pbkdf2::pbkdf2_hmac;
use sha2::{Sha256, Sha512, Digest};
use blake3;
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

    println!("\n[动态获取] 正在从代码库直接生成 RGB20 Codex ID...");

    const RGB20_CODEX_ID_STR: &str = "7C15w3W1-L0T~zXw-Aeh5~kV-Zquz729-HXQFKQW-_5lX9O8";
    const NEW_ISSUER_CODEX_ID_STR: &str = "8wxMejja-eG9vraj-8TuLoFN-lkx8SSS-3asDcZg-ov5OOww";
    const PREV_ISSUER_CODEX_ID_STR: &str = "YM_BowPP-c94xz~F-vOAQiUD-4Llr83t-p0534bT-tZNKY7s";
    println!("  - 已知的 RGB20 CodexId 字符串: {}", RGB20_CODEX_ID_STR);
    println!("  - 最新生成的 Issuer CodexId 字符串: {}", NEW_ISSUER_CODEX_ID_STR);
    println!("  - 之前生成的 Issuer CodexId 字符串: {}", PREV_ISSUER_CODEX_ID_STR);

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
        version: TxVer::from_consensus_u16(0),
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
        mass: {
            let base_mass = 200u64;
            let payload_mass = annex_data.len() as u64 * 10;
            let estimated_mass = base_mass + payload_mass;
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

    // 使用 sign_schnorr_no_aux_rand，直接传入 sighash 字节数组
    let sig = secp.sign_schnorr_no_aux_rand(&sighash, &keypair);
    println!("  - ✓ 签名完成");

    // 8. 构造签名脚本
    // 关键修复：Tondi 脚本引擎需要 OP_PUSHBYTES_65 (0x41) 前缀来正确解析签名数据
    // 根据 Tondi 源代码 (wallet/core/src/tx/mass.rs): SIGNATURE_SIZE = 1 + 64 + 1
    let mut sig_script_raw = Vec::with_capacity(66);
    sig_script_raw.push(0x41); // OP_PUSHBYTES_65 - 告诉脚本引擎接下来是 65 字节数据
    sig_script_raw.extend_from_slice(&sig.as_ref()[..]); // 64 bytes signature
    sig_script_raw.push(0x01); // 1 byte SIGHASH_ALL
    bp_tx.inputs[0].sig_script = SigScript::from_checked(sig_script_raw);

    // ==========================================
    //在这里添加以下代码，手动计算并打印本地 Txid
    // ==========================================
    let local_txid = bp_tx.txid();
    println!("  - 本地计算的交易 ID (Txid): {}", local_txid.to_hex());
    // ==========================================

    // 9. 转换为 TondiTransaction
    println!("\n[步骤 6] 转换为 TondiTransaction...");
    let tondi_tx = convert_bp_to_tondi_tx(&bp_tx, &[selected_utxo.clone()])?;
    println!("  - ✓ 转换完成\n");

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

    // 🛑 核心修改：定义子哈希的辅助闭包，确保使用 update(prefix) 而非 KDF
    let get_sub_hasher = |prefix: &[u8]| {
        let mut h = blake3::Hasher::new();
        h.update(prefix);
        h
    };

    let sub_prefix = b"TransactionSigningHash";

    // 1. 外部 Hasher 使用 SHA256 + SchnorrSigningHash 前缀
    let mut hasher = Sha256::new();
    hasher.update(b"SchnorrSigningHash");

    // 2. Version (u16 LE)
    hasher.update(&tx.version.to_consensus_u16().to_le_bytes());

    // 3. Previous outputs hash
    if hash_type == SighashType::All {
        let mut h = get_sub_hasher(sub_prefix);
        for input in tx.inputs.iter() {
            h.update(&input.prev_output.txid.to_byte_array());
            h.update(&input.prev_output.vout.to_u32().to_le_bytes());
        }
        hasher.update(h.finalize().as_bytes());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 4. Sequences hash
    if hash_type == SighashType::All {
        let mut h = get_sub_hasher(sub_prefix);
        for input in tx.inputs.iter() {
            h.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());
        }
        hasher.update(h.finalize().as_bytes());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 5. Sig op counts hash (Tondi 通常计算为 u8 的集合)
    if hash_type == SighashType::All {
        let mut h = get_sub_hasher(sub_prefix);
        for _input in tx.inputs.iter() {
            h.update(&[1u8]); // 假设 sig op count 为 1
        }
        hasher.update(h.finalize().as_bytes());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 6-10. 当前输入信息
    let input = &tx.inputs[input_index];
    hasher.update(&input.prev_output.txid.to_byte_array());
    hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());
    
    hasher.update(&(utxo_entry.utxo_entry.script_public_key.version as u16).to_le_bytes());
    let script_bytes = &utxo_entry.utxo_entry.script_public_key.script;
    // 🛑 必须加回 u64 长度前缀（Borsh/Kaspa 标准）
    hasher.update(&(script_bytes.len() as u64).to_le_bytes());
    hasher.update(script_bytes);

    hasher.update(&utxo_entry.utxo_entry.amount.to_le_bytes());
    hasher.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());
    hasher.update(&[1u8]); // Sig op count

    // 11. Outputs hash
    if hash_type == SighashType::All {
        let mut h = get_sub_hasher(sub_prefix);
        for output in tx.outputs.iter() {
            h.update(&output.value.sats().to_le_bytes());
            h.update(&0u16.to_le_bytes()); // Version
            let out_script = output.script_pubkey.as_slice();
            // 🛑 必须加回 u64 长度前缀（Borsh/Kaspa 标准）
            h.update(&(out_script.len() as u64).to_le_bytes());
            h.update(out_script);
        }
        hasher.update(h.finalize().as_bytes());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 12-14. Locktime, Subnetwork, Gas
    hasher.update(&tx.lock_time.to_consensus_u64().to_le_bytes());
    hasher.update(&tx.subnetwork_id); // 20 bytes
    hasher.update(&tx.gas.to_le_bytes());

    // 15. Payload hash
    if tx.subnetwork_id == [0u8; 20] && tx.payload.is_empty() {
        hasher.update(&[0u8; 32]);
    } else {
        let mut h = get_sub_hasher(sub_prefix);
        let p_bytes = tx.payload.as_slice();
        // 🛑 必须加回 u64 长度前缀（Borsh/Kaspa 标准）
        h.update(&(p_bytes.len() as u64).to_le_bytes());
        h.update(p_bytes);
        hasher.update(h.finalize().as_bytes());
    }

    // 16. Hash type
    hasher.update(&[hash_type as u8]);

    let final_sighash = hasher.finalize();
    Ok(final_sighash.into())
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
        subnetwork_id: {
            let mut bytes32 = [0u8; 32];
            bytes32[0..20].copy_from_slice(&tx.subnetwork_id);
            Bytes32::from_byte_array(bytes32)
        },
        gas: tx.gas,
        payload: tx.payload.to_vec(),
        mass: {
            // 如果原始 mass 为 0 或过小，使用保守值
            if tx.mass == 0 || tx.mass < 2000 {
                3000u64 // 保守的大值，确保通过节点的最低阈值
            } else {
                tx.mass // 如果已经设置了合理的值，则使用原值
            }
        },
    })
}

fn mnemonic_to_seed(mnemonic: &str, passphrase: &str) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    let salt = format!("mnemonic{}", passphrase);
    let mut seed = [0u8; 64];
    pbkdf2_hmac::<Sha512>(mnemonic.as_bytes(), salt.as_bytes(), 2048, &mut seed);
    Ok(seed)
}
