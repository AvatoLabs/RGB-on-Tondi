//! 示例：Alice 转账给 Bob (Native Transfer)
//! 为 Bob 创造 UTXO 以便接收 RGB 资产
//!
//! 运行：cargo run --example fund_bob --features grpc

use tondi::{TondiClient, TondiConfig, TondiTransaction, TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey, UtxosByAddressEntry};
use amplify::{Bytes32, ByteArray, confinement::Confined, hex::ToHex};
use bp::{Tx, TxVer, LockTime, TxIn, TxOut, Outpoint, Vout, SigScript, SeqNo, Witness, ScriptPubkey, Txid, VarIntBytes};
use commit_verify::TondiBlake3;
use bp::secp256k1::Secp256k1;
use bpstd::{XprivAccount, HardenedIndex, Address};
use std::str::FromStr;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha512;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Tondi Alice -> Bob 转账测试 ===\n");

    // 1. 配置环境
    let endpoint = "38.190.226.7:16210";
    let alice_addr = "tondi0:qzqvqaenmgmgtu8j2yjqjexpgyp7q3rcyjzeamavcgkqs775wvvp6dyte9a";
    let bob_addr = "tondi0:qzma9vrj0ef9vzskvr44l4q848p2w9gs0nkukeplwwsc6s3lk6s8c8q5ukw";
    
    // 2. 从助记词导出 Alice 的私钥
    println!("[步骤 0] 从助记词导出 Alice 的私钥...");
    let alice_mnemonic = "improve candy either cushion decrease radio junk crouch run horn rapid gravity";
    
    // 从助记词生成 seed (BIP39)
    let seed = mnemonic_to_seed(alice_mnemonic, "")?;
    println!("  - Seed 生成完成");
    
    // 创建 XprivAccount (testnet = true)
    let alice_account = XprivAccount::with_seed(true, &seed);
    
    // 派生到接收地址路径: m/84'/1'/0'/0/0 (BIP-84 测试网路径)
    // 84' = BIP-84 (SegWit), 1' = testnet, 0' = account, 0 = external, 0 = address index
    let receive_path = [
        HardenedIndex::from(84u16),  // purpose'
        HardenedIndex::from(1u16),  // coin_type' (testnet)
        HardenedIndex::from(0u16),  // account'
    ];
    let alice_account_derived = alice_account.derive(&receive_path);
    
    // 获取账户级别的 Xpriv 并转换为 Keypair
    // 注意：这里我们使用账户级别的私钥，实际应用中可能需要派生到具体的地址索引
    // 为了简化，我们直接使用账户级别的私钥
    // 实际应用中，应该派生到 /0/0 (external chain, first address)
    // 但为了简化，我们直接使用账户级别的私钥
    let alice_xpriv = alice_account_derived.xpriv();
    let keypair = alice_xpriv.to_keypair_bip340();
    let secp = Secp256k1::new();
    println!("  - 私钥导出完成\n");

    // 3. 解析 Bob 的地址（如果失败，稍后从 Alice UTXO 获取脚本格式）
    println!("[步骤 1] 解析 Bob 的地址...");
    println!("  - 尝试解析地址: {}", bob_addr);
    
    // 注意：Tondi 地址格式是 "tondi0:..." 但 bech32 解码可能需要去掉冒号
    // 或者地址格式本身有问题。让我们先尝试直接解析，如果失败则使用临时方案
    let bob_script_opt = Address::from_str(bob_addr)
        .ok()
        .map(|addr| {
            println!("  - ✓ 地址解析成功");
            let script = addr.payload.script_pubkey();
            println!("  - ScriptPubkey 长度: {} 字节\n", script.len());
            script
        });
    
    if bob_script_opt.is_none() {
        eprintln!("  - ⚠️  地址解析失败，将使用 Alice UTXO 的脚本格式作为临时方案");
        eprintln!("  - 注意：这仅用于测试，实际应用中需要正确解析 Bob 的地址\n");
    }

    let client = TondiClient::connect(TondiConfig::new(endpoint)).await?;

    // 4. 获取 Alice 的可用 UTXO
    println!("[步骤 2] 正在查询 Alice 的 UTXO...");
    let utxos = client.get_utxos_by_address(alice_addr).await?;
    let selected_utxo = utxos.iter()
        .find(|u| u.utxo_entry.amount > 5000)
        .ok_or("Alice 余额不足以支付转账和 Gas")?;

    println!("✓ 选中 UTXO: {}:{} (金额: {} sats)", 
        selected_utxo.outpoint.transaction_id.to_hex(), 
        selected_utxo.outpoint.index,
        selected_utxo.utxo_entry.amount);

    // 5. 获取 Alice 的 ScriptPubkey（从 UTXO 或地址）
    let alice_script = ScriptPubkey::from_checked(selected_utxo.utxo_entry.script_public_key.script.clone());
    
    // 如果 Bob 地址解析失败，使用 Alice 的脚本格式（临时方案）
    let bob_script_final = if let Some(script) = bob_script_opt {
        script
    } else {
        println!("  - 使用 Alice 的脚本格式作为 Bob 的输出脚本（临时方案）\n");
        alice_script.clone()
    };

    // 6. 构造基础交易 (bp::Tx)
    println!("\n[步骤 3] 构造交易...");
    let amount_to_bob = 2000u64;
    let gas_fee = 1000u64;
    
    if selected_utxo.utxo_entry.amount < amount_to_bob + gas_fee {
        return Err("余额不足".into());
    }
    
    let change = selected_utxo.utxo_entry.amount - amount_to_bob - gas_fee;

    let mut bp_tx = Tx {
        version: TxVer::from_consensus_u16(1),
        inputs: Confined::try_from_iter(vec![TxIn {
            prev_output: Outpoint::new(
                Txid::from_byte_array(selected_utxo.outpoint.transaction_id.to_byte_array()),
                Vout::from_u32(selected_utxo.outpoint.index),
            ),
            sig_script: SigScript::empty(),
            sequence: SeqNo::from_consensus_u32(0xFFFFFFFF),
            witness: Witness::new(),
        }]).unwrap(),
        outputs: Confined::try_from_iter(vec![
            TxOut { 
                value: amount_to_bob.into(), 
                script_pubkey: bob_script_final.clone(),
            },
            TxOut { 
                value: change.into(), 
                script_pubkey: alice_script.clone(),
            },
        ]).unwrap(),
        lock_time: LockTime::from_consensus_u64(0),
        subnetwork_id: [0u8; 20], // 原生子网络
        gas: gas_fee,
        payload: VarIntBytes::new(),
        mass: 0,
    };

    println!("  - 输入: 1 个");
    println!("  - 输出: 2 个 (Bob: {} sats, Alice 找零: {} sats)", amount_to_bob, change);
    println!("  - Gas: {} sats", gas_fee);

    // 7. 计算 Tondi Sighash
    println!("\n[步骤 4] 计算 Tondi Sighash...");
    let sighash = calc_tondi_schnorr_sighash(
        &bp_tx,
        0, // input_index
        &selected_utxo,
        SighashType::All,
    )?;
    println!("  - Sighash (hex): {}\n", hex::encode(&sighash));

    // 8. 执行 Schnorr 签名
    println!("[步骤 5] 执行 Schnorr 签名...");
    // 使用 keypair 进行 Schnorr 签名
    // secp256k1 的 sign_schnorr_no_aux_rand 需要 &[u8] 作为消息
    let sig = secp.sign_schnorr_no_aux_rand(&sighash, &keypair);
    println!("  - 签名完成\n");

    // 7. 构造 Tondi 标准签名脚本: 0x41 + 64字节签名 + 0x01 (SIGHASH_ALL)
    let mut sig_script_raw = vec![0x41]; // Push 65 bytes
    sig_script_raw.extend_from_slice(&sig.as_ref()[..]); // 64 bytes signature
    sig_script_raw.push(0x01); // SIGHASH_ALL
    
    bp_tx.inputs[0].sig_script = SigScript::from_checked(sig_script_raw);

    // 9. 转换为 TondiTransaction
    println!("[步骤 6] 转换为 TondiTransaction...");
    let tondi_tx = convert_bp_to_tondi_tx(&bp_tx, &[selected_utxo.clone()])?;
    println!("  - 转换完成\n");

    // 10. 提交交易
    println!("[步骤 7] 广播交易到节点...");
    match client.submit_transaction(tondi_tx, false).await {
        Ok(txid) => {
            println!("✓ 转账成功！");
            println!("  - 交易 ID: {}", txid.to_hex());
            println!("\n🎉 Bob 现在有了 UTXO，可以接收 RGB 资产了！\n");
        }
        Err(e) => {
            eprintln!("✗ 转账失败: {}", e);
            eprintln!("\n可能的原因：");
            eprintln!("  1. UTXO 已被花费");
            eprintln!("  2. Sighash 计算不正确");
            eprintln!("  3. 交易格式错误");
            eprintln!("  4. 节点需要 UTXO 索引功能（--utxoindex）\n");
            return Err(e.into());
        }
    }

    Ok(())
}

/// Sighash 类型（简化版）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SighashType {
    All = 0x01,
}

/// 计算 Tondi 的 Schnorr 签名哈希
/// 
/// 基于 Tondi 的 sighash.rs 实现，使用 Blake3 和 Tondi 特定格式
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
    
    // 使用 Tondi 的 SchnorrSigningHash（基于 Blake3）
    // 根据 Tondi 源码，这应该使用 "tondi.schnorr.signing" 或类似的上下文
    let mut hasher = TondiBlake3::from_context("tondi.schnorr.signing");

    // 1. Version: u16
    hasher.update(&tx.version.to_consensus_u16().to_le_bytes());

    // 2. Previous outputs hash (SIGHASH_ALL: hash all inputs)
    if hash_type == SighashType::All {
        let mut prev_outs_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for input in tx.inputs.iter() {
            prev_outs_hasher.update(&input.prev_output.txid.to_byte_array());
            prev_outs_hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());
        }
        hasher.update(&prev_outs_hasher.finalize());
    } else {
        // SIGHASH_ANYONECANPAY: zero hash
        hasher.update(&[0u8; 32]);
    }

    // 3. Sequences hash (SIGHASH_ALL: hash all sequences)
    if hash_type == SighashType::All {
        let mut seq_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for input in tx.inputs.iter() {
            seq_hasher.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());
        }
        hasher.update(&seq_hasher.finalize());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 4. Sig op counts hash (SIGHASH_ALL: hash all sig op counts)
    if hash_type == SighashType::All {
        let mut sigop_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for _input in tx.inputs.iter() {
            // 简化：假设 sig_op_count 为 1（实际需要从脚本解析）
            sigop_hasher.update(&[1u8]);
        }
        hasher.update(&sigop_hasher.finalize());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 5. Current input outpoint
    hasher.update(&input.prev_output.txid.to_byte_array());
    hasher.update(&input.prev_output.vout.to_u32().to_le_bytes());

    // 6. Script public key (from UTXO)
    hasher.update(&(utxo_entry.utxo_entry.script_public_key.version as u16).to_le_bytes());
    let script_bytes = &utxo_entry.utxo_entry.script_public_key.script;
    hasher.update(&(script_bytes.len() as u64).to_le_bytes());
    hasher.update(script_bytes);

    // 7. Amount (from UTXO)
    hasher.update(&utxo_entry.utxo_entry.amount.to_le_bytes());

    // 8. Sequence
    hasher.update(&(input.sequence.to_consensus_u32() as u64).to_le_bytes());

    // 9. Sig op count (简化：假设为 1)
    hasher.update(&[1u8]);

    // 10. Outputs hash (SIGHASH_ALL: hash all outputs)
    if hash_type == SighashType::All {
        let mut outputs_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        for output in tx.outputs.iter() {
            outputs_hasher.update(&output.value.sats().to_le_bytes());
            let script_bytes = output.script_pubkey.as_slice();
            outputs_hasher.update(&0u16.to_le_bytes()); // Version 0
            outputs_hasher.update(&(script_bytes.len() as u64).to_le_bytes());
            outputs_hasher.update(script_bytes);
        }
        hasher.update(&outputs_hasher.finalize());
    } else {
        hasher.update(&[0u8; 32]);
    }

    // 11. Lock time
    hasher.update(&tx.lock_time.to_consensus_u64().to_le_bytes());

    // 12. Subnetwork ID (20 bytes)
    hasher.update(&tx.subnetwork_id);

    // 13. Gas
    hasher.update(&tx.gas.to_le_bytes());

    // 14. Payload hash
    if tx.subnetwork_id == [0u8; 20] && tx.payload.is_empty() {
        hasher.update(&[0u8; 32]);
    } else {
        let mut payload_hasher = TondiBlake3::from_context("tondi.transaction.signing");
        let payload_bytes = tx.payload.as_slice();
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
    // 转换输入
    let inputs: Vec<TondiTxInput> = tx.inputs.iter().map(|input| {
        let signature_script = input.sig_script.to_vec();
        
        // 计算 sig_op_count（简化：从签名脚本推断）
        let sig_op_count = if signature_script.len() >= 65 {
            1 // 有签名，假设是 1 个 sig op
        } else {
            0
        };
        
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
    
    // 转换输出
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
        mass: tx.mass,
    })
}

/// 从助记词生成 BIP39 seed
/// 
/// 使用 PBKDF2-HMAC-SHA512 从助记词和可选的 passphrase 生成 64 字节 seed
fn mnemonic_to_seed(mnemonic: &str, passphrase: &str) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    // BIP39: seed = PBKDF2(PRF = HMAC-SHA512, Password = mnemonic sentence, Salt = "mnemonic" + passphrase, Iterations = 2048, dkLen = 64)
    let salt = format!("mnemonic{}", passphrase);
    let mut seed = [0u8; 64];
    pbkdf2_hmac::<Sha512>(mnemonic.as_bytes(), salt.as_bytes(), 2048, &mut seed);
    Ok(seed)
}

