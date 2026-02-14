//! 普通的 Tondi Taproot (Copperoot) 交易测试示例
//!
//! 这个示例展示了在 Tondi 上构造和广播普通交易的流程：
//! 1. 从助记词导出私钥
//! 2. 获取 UTXO
//! 3. 构造普通交易（无 RGB，无 Annex 数据）
//! 4. 计算签名并广播交易
//!
//! 运行方式：
//! ```bash
//! cd bp-tondi-client
//! cargo run --example rgb_issue_real --features grpc
//! ```

use tondi::{TondiClient, TondiConfig, TondiTransaction, TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey, UtxosByAddressEntry};
use amplify::{Bytes32, ByteArray, confinement::Confined, hex::ToHex};
use bp::{Tx, TxVer, LockTime, TxIn, TxOut, Outpoint as BpOutpoint, Vout, SigScript, SeqNo, Witness, ScriptPubkey, Txid, VarIntBytes};
use bp::secp256k1::Secp256k1;
use bpstd::{XprivAccount, HardenedIndex};
use pbkdf2::pbkdf2_hmac;
use sha2::{Sha256, Sha512, Digest};
use blake3;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 普通的 Tondi Taproot (Copperoot) 交易测试 ===\n");

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
        .ok_or("Alice 余额不足以支付交易和 Gas")?;
    
    println!("✓ 选中 UTXO: {}:{} (金额: {} sats)", 
        selected_utxo.outpoint.transaction_id.to_hex(), 
        selected_utxo.outpoint.index,
        selected_utxo.utxo_entry.amount);

    // ========================================================================
    // 步骤 2 (RGB Genesis) - 已注释，普通交易不需要
    // ========================================================================
    // 普通交易不需要创建 RGB Genesis，跳过此步骤

    // ========================================================================
    // 步骤 3 (Annex Data) - 已注释，普通交易不需要
    // ========================================================================
    // 普通交易不需要 Annex 数据，跳过此步骤

    // 4. 构造普通交易
    println!("\n[步骤 4] 构造普通交易...");
    let amount_to_send = 5000u64; // 随便转一小笔给自己
    let gas_fee = 0u64; // 普通交易不需要 Gas
    
    if selected_utxo.utxo_entry.amount < amount_to_send + gas_fee {
        return Err("余额不足".into());
    }
    
    let change = selected_utxo.utxo_entry.amount - amount_to_send - gas_fee;
    let alice_script = ScriptPubkey::from_checked(selected_utxo.utxo_entry.script_public_key.script.clone());

    // --- 修改为普通交易构造 ---
    let mut bp_tx = Tx {
        // 版本号检查：尝试将交易版本设为 0 或 1 分别测试
        // 当前使用版本 1，如需测试版本 0，改为：TxVer::from_consensus_u16(0)
        version: TxVer::from_consensus_u16(1),
        inputs: Confined::try_from_iter(vec![TxIn {
            prev_output: BpOutpoint::new(
                Txid::from_byte_array(selected_utxo.outpoint.transaction_id.to_byte_array()),
                Vout::from_u32(selected_utxo.outpoint.index),
            ),
            sig_script: SigScript::empty(),
            sequence: SeqNo::from_consensus_u32(0xFFFFFFFF),
            witness: Witness::new(), // 普通交易不需要 Annex 数据
        }]).unwrap(),
        outputs: Confined::try_from_iter(vec![
            TxOut { 
                value: amount_to_send.into(), // 随便转一小笔给自己
                script_pubkey: alice_script.clone(),
            },
            TxOut { 
                value: change.into(), 
                script_pubkey: alice_script.clone(),
            },
        ]).unwrap(),
        lock_time: LockTime::from_consensus_u64(0),
        subnetwork_id: [0u8; 20], // 普通交易使用全零子网
        gas: gas_fee,
        payload: VarIntBytes::new(), // Payload 必须为空
        mass: 1000, // 给一个非零的 Mass 值
    };

    println!("  - 输入: 1 个");
    println!("  - 输出: 2 个 (发送: {} sats, 找零: {} sats)", amount_to_send, change);
    println!("  - Gas: {} sats", gas_fee);
    println!("  - Subnetwork ID: {} (全零，主网络)", hex::encode(&bp_tx.subnetwork_id));
    println!("  - Payload 长度: {} 字节 (必须为空)", bp_tx.payload.len());
    println!("  - Mass: {}", bp_tx.mass);

    // 在 [步驟 4] 構造完 bp_tx 後加入這行
    let txid = bp_tx.txid();
    println!("  - 本地計算的交易 ID (Txid): {}", txid.to_hex());

    // 5. 计算 Tondi Sighash 并签名
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

    // 6. 构造签名脚本
    // ========================================================================
    // 关键修复：Tondi 脚本引擎需要 OP_PUSHBYTES_65 (0x41) 前缀来正确解析签名数据
    // 根据 Tondi 源代码 (wallet/core/src/tx/mass.rs): SIGNATURE_SIZE = 1 + 64 + 1
    // ========================================================================
    let mut sig_script_raw = Vec::with_capacity(66);
    sig_script_raw.push(0x41); // OP_PUSHBYTES_65 - 告诉脚本引擎接下来是 65 字节数据
    sig_script_raw.extend_from_slice(&sig.as_ref()[..]); // 64 bytes signature
    sig_script_raw.push(0x01); // 1 byte SIGHASH_ALL
    bp_tx.inputs[0].sig_script = SigScript::from_checked(sig_script_raw);

    // 7. 转换为 TondiTransaction
    println!("\n[步骤 6] 转换为 TondiTransaction...");
    let tondi_tx = convert_bp_to_tondi_tx(&bp_tx, &[selected_utxo.clone()])?;
    println!("  - ✓ 转换完成\n");

    // ========================================================================
    // 调试信息：打印完整的 TondiTransaction 内容
    // ========================================================================
    println!("[调试信息] 完整的 TondiTransaction 结构体内容:\n{:#?}\n", tondi_tx);

    // 验证关键字段
    println!("[验证] 关键字段检查:");
    println!("  - Subnetwork ID 长度: {} 字节 (应为 20)", tondi_tx.subnetwork_id.to_byte_array()[0..20].len());
    println!("  - Payload 长度: {} 字节 (应为 0)", tondi_tx.payload.len());
    println!("  - Mass: {} (应 >= 1000)", tondi_tx.mass);
    println!();

    // 8. 提交交易
    println!("[步骤 7] 广播交易到节点...");
    match client.submit_transaction(tondi_tx, false).await {
        Ok(txid) => {
            println!("✓ 普通交易提交成功！");
            println!("  - 交易 ID: {}", txid.to_hex());
            println!("\n🎉 普通 Tondi 交易已成功广播！\n");
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
/// 
/// 普通交易不需要处理 Annex 的哈希分支。
/// 使用 Tondi 规定的上下文标签：
/// - Schnorr 签名标签："tondi.schnorr.signing"
/// - 交易签名标签："tondi.transaction.signing"
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
        // ========================================================================
        // 修正 2B：mass 值的保守设置（避免节点拒绝交易）
        // ========================================================================
        // 对于包含 payload 的非标准交易，Tondi 节点通常设置了最低的 mass 阈值
        // 如果 mass 过低（如 550），节点可能将其视为垃圾交易而直接丢弃请求
        // 这里使用一个保守的大值（3000），以确保交易能够通过节点的最低阈值检查
        // 如果 mass 设置正确，节点会返回明确的错误响应（如 InvalidSignature），
        // 而不是直接断开连接（No response received）
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

/// 从助记词生成 BIP39 seed
fn mnemonic_to_seed(mnemonic: &str, passphrase: &str) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    let salt = format!("mnemonic{}", passphrase);
    let mut seed = [0u8; 64];
    pbkdf2_hmac::<Sha512>(mnemonic.as_bytes(), salt.as_bytes(), 2048, &mut seed);
    Ok(seed)
}
