//! 示例：Alice 转账给 Bob (Native Transfer)
//! 为 Bob 创造 UTXO 以便接收 RGB 资产
//!
//! 运行：cargo run --example fund_bob --features grpc

use tondi::{TondiClient, TondiConfig, TondiTransaction, TondiTxInput, TondiTxOutput, TondiOutpoint, ScriptPublicKey, UtxosByAddressEntry};
use amplify::{Bytes32, ByteArray, confinement::Confined, hex::ToHex};
use bp::{Tx, TxVer, LockTime, TxIn, TxOut, Outpoint, Vout, SigScript, SeqNo, Witness, ScriptPubkey, Txid, VarIntBytes};
use bp::secp256k1::Secp256k1;
use bpstd::{XprivAccount, HardenedIndex, Address, Keychain, NormalIndex, Idx, Xpriv, DerivationIndex};
use std::str::FromStr;
use pbkdf2::pbkdf2_hmac;
use sha2::{Sha256, Sha512, Digest};
use blake3;
use commit_verify::Ripemd160;
use bp::secp256k1::SecretKey;

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
    // 尝试不同的 passphrase 和 seed string 组合
    let passphrases = vec!["", "123456"];
    let mut found_match = false;
    let mut matched_passphrase = "";
    let mut use_bitcoin_seed = false;
    
    for passphrase in &passphrases {
        println!("  - 尝试 passphrase: \"{}\"", if passphrase.is_empty() { "(空)" } else { passphrase });
        let seed = mnemonic_to_seed(alice_mnemonic, passphrase)?;
        
        // 测试使用 "Tondi seed" 的情况（当前实现）
        println!("    [使用 \"Tondi seed\"]");
        let alice_account_tondi = XprivAccount::with_seed(true, &seed);
        let master_xpriv_tondi = alice_account_tondi.xpriv();
        
        // 验证这是 Master Key（depth 应该是 0）
        println!("      - Master Key depth: {}", master_xpriv_tondi.depth());
        assert_eq!(master_xpriv_tondi.depth(), 0, "应该是 Master Key (depth=0)");
        
        let master_pubkey_tondi = master_xpriv_tondi.to_compr_pk();
        let master_pubkey_bytes_tondi = master_pubkey_tondi.serialize();
        
        // Tondi 使用标准 BIP32 指纹计算：RIPEMD160(SHA256(公钥))
        // 这是 Master Key (m/) 的指纹
        let bip32_fp_tondi = calc_bip32_fingerprint(&master_pubkey_bytes_tondi);
        let bip32_fp_hex_tondi = hex::encode(&bip32_fp_tondi);
        
        // 对比：XprivAccount::master_fp() 使用的是 BLAKE3，不是标准 BIP32
        let stored_master_fp = alice_account_tondi.master_fp();
        let stored_master_fp_bytes: [u8; 4] = stored_master_fp.into();
        let stored_master_fp_hex = hex::encode(&stored_master_fp_bytes);
        
        println!("      - Master Key 公钥 (hex): {}", hex::encode(&master_pubkey_bytes_tondi));
        println!("      - Master Key BIP32 指纹 (RIPEMD160(SHA256)): {}", bip32_fp_hex_tondi);
        println!("      - XprivAccount::master_fp() (BLAKE3): {}", stored_master_fp_hex);
        
        // 测试使用 "Bitcoin seed" 的情况（标准 BIP32）
        println!("    [使用 \"Bitcoin seed\"]");
        let master_xpriv_bitcoin = create_master_key_with_seed_string(true, &seed, b"Bitcoin seed")?;
        
        // 验证这是 Master Key（depth 应该是 0）
        println!("      - Master Key depth: {}", master_xpriv_bitcoin.depth());
        assert_eq!(master_xpriv_bitcoin.depth(), 0, "应该是 Master Key (depth=0)");
        
        let master_pubkey_bitcoin = master_xpriv_bitcoin.to_compr_pk();
        let master_pubkey_bytes_bitcoin = master_pubkey_bitcoin.serialize();
        
        // 这是 Master Key (m/) 的指纹
        let bip32_fp_bitcoin = calc_bip32_fingerprint(&master_pubkey_bytes_bitcoin);
        let bip32_fp_hex_bitcoin = hex::encode(&bip32_fp_bitcoin);
        
        println!("      - Master Key 公钥 (hex): {}", hex::encode(&master_pubkey_bytes_bitcoin));
        println!("      - Master Key BIP32 指纹 (RIPEMD160(SHA256)): {}", bip32_fp_hex_bitcoin);
        
        // 检查是否匹配（Tondi 使用标准 BIP32 指纹计算方式）
        if bip32_fp_hex_bitcoin == "fd0d9f42" {
            println!("      - ✓✓✓ 找到匹配！使用 \"Bitcoin seed\" + passphrase: \"{}\"", passphrase);
            found_match = true;
            matched_passphrase = passphrase;
            use_bitcoin_seed = true;
            break;
        } else if bip32_fp_hex_tondi == "fd0d9f42" {
            println!("      - ✓✓✓ 找到匹配！使用 \"Tondi seed\" + passphrase: \"{}\"", passphrase);
            found_match = true;
            matched_passphrase = passphrase;
            use_bitcoin_seed = false;
            break;
        }
        println!();
    }
    
    if !found_match {
        println!("  - ✗ 所有组合都未找到匹配的指纹");
        println!("  - 钱包显示的实际指纹: fd0d9f42");
        println!("  - 可能的原因：");
        println!("     1. 助记词不正确");
        println!("     2. 钱包使用了不同的 passphrase（未测试的）");
        println!("     3. 钱包使用了不同的主密钥生成方式");
        return Err("无法找到匹配的指纹配置".into());
    }
    
    // 使用匹配的配置创建账户
    let seed = mnemonic_to_seed(alice_mnemonic, matched_passphrase)?;
    let alice_account = if use_bitcoin_seed {
        create_xpriv_account_with_seed_string(true, &seed, b"Bitcoin seed")?
    } else {
        XprivAccount::with_seed(true, &seed)
    };
    println!();
    
    // 派生到接收地址路径: m/84'/1'/0' (BIP-84 测试网路径)
    // 84' = BIP-84 (SegWit), 1' = testnet, 0' = account
    let receive_path = [
        HardenedIndex::from(84u16),  // purpose'
        HardenedIndex::from(1u16),  // coin_type' (testnet)
        HardenedIndex::from(0u16),  // account'
    ];
    let alice_account_derived = alice_account.derive(&receive_path);
    let account_xpriv = alice_account_derived.xpriv();
    
    // 先连接到节点获取 UTXO，以便知道需要匹配的公钥
    let client = TondiClient::connect(TondiConfig::new(endpoint)).await?;
    println!("[步骤 1.5] 查询 UTXO 以确定正确的派生路径...");
    let utxos = client.get_utxos_by_address(alice_addr).await?;
    let selected_utxo = utxos.iter()
        .find(|u| u.utxo_entry.amount > 5000)
        .ok_or("Alice 余额不足以支付转账和 Gas")?
        .clone(); // 克隆以便后续使用
    
    let utxo_pubkey_bytes = &selected_utxo.utxo_entry.script_public_key.script[1..33]; // 跳过 0x20，取32字节公钥
    println!("  - UTXO 公钥 (hex): {}", hex::encode(utxo_pubkey_bytes));
    println!("  - 注意：如果找不到匹配，请检查 UTXO 是否使用了非标准的派生路径");
    println!("  - 或者 UTXO 是否由其他私钥/助记词创建\n");
    
    // 首先尝试 account 级别的 key（不继续派生到地址）
    println!("  - 首先尝试 account 级别的 key（不派生到地址）...");
    let account_keypair = account_xpriv.to_keypair_bip340();
    let account_pubkey_xonly = account_keypair.public_key().x_only_public_key().0;
    let account_pubkey_bytes = account_pubkey_xonly.serialize();
    println!("  - Account 公钥 (hex): {}", hex::encode(&account_pubkey_bytes));
    
    // 显示前几个尝试的公钥以便调试
    println!("  - 调试：显示前 3 个派生地址的公钥...");
    for i in 0..3 {
        let index = NormalIndex::from(i as u16);
        let test_xpriv = account_xpriv.derive_priv([Keychain::OUTER.into(), index]);
        let test_keypair = test_xpriv.to_keypair_bip340();
        let test_pubkey_xonly = test_keypair.public_key().x_only_public_key().0;
        let test_pubkey_bytes = test_pubkey_xonly.serialize();
        println!("    - m/84'/1'/0'/0/{}: {}", i, hex::encode(&test_pubkey_bytes));
    }
    println!();
    
    let mut found_keypair = if account_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
        println!("  - ✓ 找到匹配的公钥！使用 account 级别的 key (m/84'/1'/0')");
        Some(account_keypair)
    } else {
        None
    };
    
    // 如果 account key 不匹配，尝试多个派生路径来找到匹配的公钥
    // 路径格式: m/84'/1'/0'/keychain/index
    // keychain: 0 = external (OUTER), 1 = internal (INNER)
    // index: 0, 1, 2, ...
    let max_index = 100;
    let mut tried_count = 0;
    
    if found_keypair.is_none() {
        println!("  - Account key 不匹配，尝试派生到地址级别（最多尝试 {} 个索引）...", max_index * 2);
    
    for keychain_val in 0..=1 {
        let keychain = if keychain_val == 0 { Keychain::OUTER } else { Keychain::INNER };
        for index_val in 0..max_index {
            tried_count += 1;
            if tried_count % 20 == 0 {
                println!("  - 已尝试 {} 个路径...", tried_count);
            }
            
            let index = NormalIndex::from(index_val as u16);
            let address_xpriv = account_xpriv.derive_priv([keychain.into(), index]);
            let test_keypair = address_xpriv.to_keypair_bip340();
            let test_pubkey_xonly = test_keypair.public_key().x_only_public_key().0;
            let test_pubkey_bytes = test_pubkey_xonly.serialize();
            
            if test_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
                found_keypair = Some(test_keypair);
                println!("  - ✓ 找到匹配的公钥！路径: m/84'/1'/0'/{}/{} (共尝试了 {} 个路径)", 
                    keychain_val, index_val, tried_count);
                break;
            }
        }
        if found_keypair.is_some() {
            break;
        }
    }
    
        // 如果 BIP-84 路径没找到，尝试 BIP-44 路径 (m/44'/1'/0'/0/index)
        if found_keypair.is_none() {
            println!("  - BIP-84 路径未找到匹配，尝试 BIP-44 路径 (m/44'/1'/0'/0/index)...");
            let bip44_path = [
                HardenedIndex::from(44u16),  // purpose' (BIP-44)
                HardenedIndex::from(1u16),  // coin_type' (testnet)
                HardenedIndex::from(0u16),  // account'
            ];
            let bip44_account = alice_account.derive(&bip44_path);
            let bip44_account_xpriv = bip44_account.xpriv();
            
            // 先尝试 account 级别的 key
            let bip44_account_keypair = bip44_account_xpriv.to_keypair_bip340();
            let bip44_account_pubkey_xonly = bip44_account_keypair.public_key().x_only_public_key().0;
            let bip44_account_pubkey_bytes = bip44_account_pubkey_xonly.serialize();
            if bip44_account_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
                found_keypair = Some(bip44_account_keypair);
                println!("  - ✓ 找到匹配的公钥！使用 BIP-44 account 级别的 key (m/44'/1'/0')");
            } else {
                // 尝试派生到地址级别
                for keychain_val in 0..=1 {
                    let keychain = if keychain_val == 0 { Keychain::OUTER } else { Keychain::INNER };
                    for index_val in 0..max_index {
                        tried_count += 1;
                        if tried_count % 20 == 0 {
                            println!("  - 已尝试 {} 个路径...", tried_count);
                        }
                        
                        let index = NormalIndex::from(index_val as u16);
                        let address_xpriv = bip44_account_xpriv.derive_priv([keychain.into(), index]);
                        let test_keypair = address_xpriv.to_keypair_bip340();
                        let test_pubkey_xonly = test_keypair.public_key().x_only_public_key().0;
                        let test_pubkey_bytes = test_pubkey_xonly.serialize();
                        
                        if test_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
                            found_keypair = Some(test_keypair);
                            println!("  - ✓ 找到匹配的公钥！路径: m/44'/1'/0'/{}/{} (共尝试了 {} 个路径)", 
                                keychain_val, index_val, tried_count);
                            break;
                        }
                    }
                    if found_keypair.is_some() {
                        break;
                    }
                }
            }
        }
        
        // 如果还是没找到，尝试不同的 account 索引（1, 2, 3...）
        if found_keypair.is_none() {
            println!("  - 尝试不同的 account 索引（1-5）...");
            for account_idx in 1u16..=5u16 {
                for purpose in [44u16, 84u16] {
                    let alt_path = [
                        HardenedIndex::from(purpose),
                        HardenedIndex::from(1u16),  // coin_type' (testnet)
                        HardenedIndex::from(account_idx),
                    ];
                    let alt_account = alice_account.derive(&alt_path);
                    let alt_account_xpriv = alt_account.xpriv();
                    
                    // 尝试 account 级别的 key
                    let alt_keypair = alt_account_xpriv.to_keypair_bip340();
                    let alt_pubkey_xonly = alt_keypair.public_key().x_only_public_key().0;
                    let alt_pubkey_bytes = alt_pubkey_xonly.serialize();
                    if alt_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
                        found_keypair = Some(alt_keypair);
                        println!("  - ✓ 找到匹配的公钥！路径: m/{}/1'/{}'", purpose, account_idx);
                        break;
                    }
                    
                    // 也尝试派生到地址级别（只尝试前几个索引）
                    for keychain_val in 0..=1 {
                        for addr_idx in 0..10 {
                            let keychain = if keychain_val == 0 { Keychain::OUTER } else { Keychain::INNER };
                            let index = NormalIndex::from(addr_idx as u16);
                            let address_xpriv = alt_account_xpriv.derive_priv([keychain.into(), index]);
                            let test_keypair = address_xpriv.to_keypair_bip340();
                            let test_pubkey_xonly = test_keypair.public_key().x_only_public_key().0;
                            let test_pubkey_bytes = test_pubkey_xonly.serialize();
                            
                            if test_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
                                found_keypair = Some(test_keypair);
                                println!("  - ✓ 找到匹配的公钥！路径: m/{}/1'/{}'/{}/{}", 
                                    purpose, account_idx, keychain_val, addr_idx);
                                break;
                            }
                        }
                        if found_keypair.is_some() {
                            break;
                        }
                    }
                }
                if found_keypair.is_some() {
                    break;
                }
            }
        }
        
        // 如果还是没找到，尝试不同的 coin_type
        // Tondi 官方使用 7890，但 Kaspa 使用 111111，都尝试一下
        if found_keypair.is_none() {
            println!("  - 尝试不同的 coin_type (0, 1, 111111, 7890)...");
            // 使用 u32 数组，然后转换为 HardenedIndex
            let coin_types: Vec<u32> = vec![0, 1, 111111, 7890];
            for coin_type_val in coin_types {
                for purpose in [44u16, 84u16] {
                    // 对于大于 u16 的值，使用 try_from_child_number
                    let coin_type = if coin_type_val <= u16::MAX as u32 {
                        HardenedIndex::from(coin_type_val as u16)
                    } else {
                        HardenedIndex::try_from_child_number(coin_type_val)
                            .map_err(|e| format!("Invalid coin_type {}: {}", coin_type_val, e))?
                    };
                    let alt_path = [
                        HardenedIndex::from(purpose),
                        coin_type,
                        HardenedIndex::from(0u16),  // account'
                    ];
                    let alt_account = alice_account.derive(&alt_path);
                    let alt_account_xpriv = alt_account.xpriv();
                    
                    // 尝试 account 级别的 key
                    let alt_keypair = alt_account_xpriv.to_keypair_bip340();
                    let alt_pubkey_xonly = alt_keypair.public_key().x_only_public_key().0;
                    let alt_pubkey_bytes = alt_pubkey_xonly.serialize();
                    if alt_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
                        found_keypair = Some(alt_keypair);
                        println!("  - ✓ 找到匹配的公钥！路径: m/{}/{}'/0'", purpose, coin_type_val);
                        break;
                    }
                    
                    // 也尝试派生到地址级别（只尝试前几个索引）
                    for keychain_val in 0..=1 {
                        for addr_idx in 0..10 {
                            let keychain = if keychain_val == 0 { Keychain::OUTER } else { Keychain::INNER };
                            let index = NormalIndex::from(addr_idx as u16);
                            let address_xpriv = alt_account_xpriv.derive_priv([keychain.into(), index]);
                            let test_keypair = address_xpriv.to_keypair_bip340();
                            let test_pubkey_xonly = test_keypair.public_key().x_only_public_key().0;
                            let test_pubkey_bytes = test_pubkey_xonly.serialize();
                            
                            if test_pubkey_bytes.as_slice() == utxo_pubkey_bytes {
                                found_keypair = Some(test_keypair);
                                println!("  - ✓ 找到匹配的公钥！路径: m/{}/{}'/0'/{}/{}", 
                                    purpose, coin_type_val, keychain_val, addr_idx);
                                break;
                            }
                        }
                        if found_keypair.is_some() {
                            break;
                        }
                    }
                }
                if found_keypair.is_some() {
                    break;
                }
            }
        }
    }
    
    let (keypair, secp) = if let Some(kp) = found_keypair {
        (kp, Secp256k1::new())
    } else {
        eprintln!("  - ✗ 已尝试多个路径，未找到匹配的公钥");
        eprintln!("  - 可能的原因：");
        eprintln!("    1. 助记词不正确");
        eprintln!("    2. UTXO 使用了不同的派生路径（非标准 BIP-84/BIP-44）");
        eprintln!("    3. UTXO 由其他私钥创建");
        eprintln!("    4. 使用了不同的 account 索引（非 0）");
        return Err("无法找到匹配的派生路径。请检查助记词是否正确，或 UTXO 是否由其他私钥创建。".into());
    };
    
    println!("  - 私钥导出完成\n");

    // 3. 解析 Bob 的地址（如果失败，稍后从 Alice UTXO 获取脚本格式）
    println!("[步骤 1] 解析 Bob 的地址...");
    println!("  - 尝试解析地址: {}", bob_addr);
    
    // Tondi 地址格式可能是 "tondi0:..." 但 bech32 标准使用 "1" 作为分隔符
    // 先移除 "tondi0:" 前缀，提取 payload，然后重新组合为标准 bech32 格式
    let normalized_addr = if let Some(payload_part) = bob_addr.strip_prefix("tondi0:") {
        // 移除前缀后，重新组合为标准 bech32 格式: hrp + "1" + payload
        format!("tondi01{}", payload_part)
    } else if let Some(payload_part) = bob_addr.strip_prefix("tondi:") {
        format!("tondi1{}", payload_part)
    } else if bob_addr.contains(':') {
        // 其他带冒号的格式，尝试替换
        bob_addr.replace(':', "1")
    } else {
        bob_addr.to_string()
    };
    
    println!("  - 标准化后的地址: {}", normalized_addr);
    
    let bob_script_opt = Address::from_str(&normalized_addr)
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

    // 注意：client 和 selected_utxo 已经在步骤 1.5 中获取了，这里直接使用

    println!("✓ 选中 UTXO: {}:{} (金额: {} sats)", 
        selected_utxo.outpoint.transaction_id.to_hex(), 
        selected_utxo.outpoint.index,
        selected_utxo.utxo_entry.amount);
    
    // 调试：打印 UTXO 的 script_public_key 信息
    println!("  - UTXO ScriptPublicKey 版本: {}", selected_utxo.utxo_entry.script_public_key.version);
    println!("  - UTXO ScriptPublicKey 脚本长度: {} 字节", selected_utxo.utxo_entry.script_public_key.script.len());
    println!("  - UTXO ScriptPublicKey 脚本 (hex): {}", hex::encode(&selected_utxo.utxo_entry.script_public_key.script));
    if selected_utxo.utxo_entry.script_public_key.script.len() >= 34 {
        println!("  - UTXO ScriptPublicKey 脚本格式: 0x{:02x} (前32字节) ... 0x{:02x} (最后1字节)", 
            selected_utxo.utxo_entry.script_public_key.script[0],
            selected_utxo.utxo_entry.script_public_key.script[selected_utxo.utxo_entry.script_public_key.script.len() - 1]);
    }

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
        version: TxVer::from_consensus_u16(0),
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
        mass: 1000,
    };

    println!("  - 输入: 1 个");
    println!("  - 输出: 2 个 (Bob: {} sats, Alice 找零: {} sats)", amount_to_bob, change);
    println!("  - Gas: {} sats", gas_fee);

    // 7. 计算 Tondi Sighash
    println!("\n[步骤 4] 计算 Tondi Sighash...");
    println!("  - 交易版本: {}", bp_tx.version.to_consensus_u16());
    println!("  - 输入数量: {}", bp_tx.inputs.len());
    println!("  - 输出数量: {}", bp_tx.outputs.len());
    println!("  - Lock Time: {}", bp_tx.lock_time.to_consensus_u64());
    println!("  - Subnetwork ID: {}", hex::encode(&bp_tx.subnetwork_id));
    println!("  - Gas: {}", bp_tx.gas);
    println!("  - Payload 长度: {} 字节", bp_tx.payload.len());
    let sighash = calc_tondi_schnorr_sighash(
        &bp_tx,
        0, // input_index
        &selected_utxo,
        SighashType::All,
    )?;
    println!("  - Sighash (hex): {}\n", hex::encode(&sighash));

    // 8. 执行 Schnorr 签名
    println!("[步骤 5] 执行 Schnorr 签名...");
    // 注意：公钥已经在步骤 1.5 中验证匹配，这里直接签名
    // 对齐 Tondi 的签名方法：
    // Tondi 使用: let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())?;
    //            let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
    // bp::secp256k1::sign_schnorr_no_aux_rand 需要 &[u8]，直接传入 sighash
    // 虽然方法名不同，但功能相同（都是 Schnorr 签名，不使用辅助随机数）
    let sig = secp.sign_schnorr_no_aux_rand(&sighash, &keypair);
    println!("  - 签名完成\n");

    // 7. 构造 Tondi 标准签名脚本: OP_PUSHBYTES_65 (0x41) + 64字节签名 + 1字节哈希类型 (共66字节)
    // 关键修复：Tondi 脚本引擎需要 OP_PUSHBYTES_65 (0x41) 前缀来正确解析签名数据
    // 根据 Tondi 源代码 (wallet/core/src/tx/mass.rs): SIGNATURE_SIZE = 1 + 64 + 1
    let mut sig_script_raw = Vec::with_capacity(66);
    sig_script_raw.push(0x41); // OP_PUSHBYTES_65 - 告诉脚本引擎接下来是 65 字节数据
    sig_script_raw.extend_from_slice(&sig.as_ref()[..]); // 64 bytes signature
    sig_script_raw.push(0x01); // 1 byte SIGHASH_ALL
    
    println!("  - 签名脚本长度: {} 字节", sig_script_raw.len());
    println!("  - 签名脚本 (hex): {}", hex::encode(&sig_script_raw));
    println!("  - 签名脚本 (前10字节): {:?}", &sig_script_raw[..sig_script_raw.len().min(10)]);
    
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
/// 基于 Tondi 规范：
/// - 外部哈希使用 SHA256，前缀为 "SchnorrSigningHash"
/// - 内部子哈希使用 BLAKE3，使用 update(prefix) 而非 KDF 模式
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

/// 使用自定义 seed string 创建主密钥
/// 
/// 允许使用 "Bitcoin seed" 或 "Tondi seed" 等不同的 seed string
fn create_master_key_with_seed_string(
    testnet: bool,
    seed: &[u8],
    seed_string: &[u8],
) -> Result<Xpriv, Box<dyn std::error::Error>> {
    Ok(Xpriv::new_master_with_seed_string(testnet, seed, seed_string))
}

/// 使用自定义 seed string 创建 XprivAccount
fn create_xpriv_account_with_seed_string(
    testnet: bool,
    seed: &[u8],
    seed_string: &[u8],
) -> Result<XprivAccount, Box<dyn std::error::Error>> {
    let xpriv = create_master_key_with_seed_string(testnet, seed, seed_string)?;
    Ok(XprivAccount::new_master(xpriv))
}

/// 计算标准 BIP32 指纹
/// 
/// BIP32 指纹计算方式：
/// 1. SHA256(公钥)
/// 2. RIPEMD160(SHA256(公钥))
/// 3. 取前4字节作为指纹
fn calc_bip32_fingerprint(pubkey: &[u8]) -> [u8; 4] {
    // 1. SHA256(公钥)
    let mut hasher = Sha256::new();
    hasher.update(pubkey);
    let sha256_hash = hasher.finalize();
    
    // 2. RIPEMD160(SHA256(公钥))
    let mut ripemd = Ripemd160::new();
    ripemd.update(&sha256_hash);
    let ripemd_hash = ripemd.finalize();
    
    // 3. 取前4字节作为指纹
    let mut fp = [0u8; 4];
    fp.copy_from_slice(&ripemd_hash[..4]);
    fp
}

