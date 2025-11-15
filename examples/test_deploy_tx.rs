// Manual Deploy transaction test
// Tests the skip_preflight fix without waiting for round timing

use anyhow::Result;
use ore_api::instruction::Deploy;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use std::str::FromStr;

const ORE_PROGRAM_ID: &str = "oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv";
const ENTROPY_PROGRAM_ID: &str = "3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X";
const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

const BOARD: &[u8] = b"board";
const MINER: &[u8] = b"miner";
const AUTOMATION: &[u8] = b"automation";
const ROUND: &[u8] = b"round";
const ENTROPY_VAR: &[u8] = b"var";

fn main() -> Result<()> {
    println!("🧪 Deploy Transaction Test");
    println!("==========================\n");

    // Load wallet from environment
    let wallet_key = std::env::var("WALLET_PRIVATE_KEY").expect("WALLET_PRIVATE_KEY not set");
    let wallet_bytes = bs58::decode(&wallet_key)
        .into_vec()
        .expect("Invalid wallet key");
    let wallet = Keypair::from_bytes(&wallet_bytes).expect("Invalid keypair");

    let authority = wallet.pubkey();
    println!("📍 Wallet: {}", authority);

    // RPC client
    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| {
        "https://edge.erpc.global?api-key=507c3fff-6dc7-4d6d-8915-596be560814f".to_string()
    });
    let rpc = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());
    println!("📡 RPC: {}\n", rpc_url);

    // Get current balance
    let balance = rpc.get_balance(&authority)?;
    println!("💰 Current balance: {} SOL\n", balance as f64 / 1e9);

    // Program IDs
    let ore_program = Pubkey::from_str(ORE_PROGRAM_ID)?;
    let entropy_program = Pubkey::from_str(ENTROPY_PROGRAM_ID)?;
    let system_program = Pubkey::from_str(SYSTEM_PROGRAM)?;

    // Derive PDAs
    let (board_pda, _) = Pubkey::find_program_address(&[BOARD], &ore_program);
    let (miner_pda, _) =
        Pubkey::find_program_address(&[MINER, &authority.to_bytes()], &ore_program);
    let (automation_pda, _) =
        Pubkey::find_program_address(&[AUTOMATION, &authority.to_bytes()], &ore_program);

    // Get current round from board
    let board_data = rpc.get_account_data(&board_pda)?;
    if board_data.len() < 33 {
        anyhow::bail!("Invalid board data length: {}", board_data.len());
    }

    // Extract round PDA from board (bytes 1-33)
    let round_pda_bytes: [u8; 32] = board_data[1..33].try_into()?;
    let round_pda = Pubkey::new_from_array(round_pda_bytes);

    println!("📊 PDAs:");
    println!("   Board: {}", board_pda);
    println!("   Miner: {} (may not exist yet)", miner_pda);
    println!("   Automation: {} (may not exist yet)", automation_pda);
    println!("   Round: {}", round_pda);

    // Check if miner account exists
    match rpc.get_account(&miner_pda) {
        Ok(account) => {
            println!("\n✅ Miner account EXISTS (owner: {})", account.owner);
            println!("   This wallet has already deployed before");
        }
        Err(_) => {
            println!("\n📝 Miner account DOES NOT EXIST");
            println!("   This is a first-time wallet - account will be created");
        }
    }

    // Derive entropy VAR PDA
    let (entropy_var_pda, _) = Pubkey::find_program_address(&[ENTROPY_VAR], &entropy_program);
    println!("   Entropy VAR: {}\n", entropy_var_pda);

    // Build Deploy instruction
    let deploy_ix = Instruction {
        program_id: ore_program,
        accounts: vec![
            AccountMeta::new(authority, true),       // 0: signer (authority)
            AccountMeta::new(authority, false),      // 1: authority
            AccountMeta::new(automation_pda, false), // 2: automation PDA
            AccountMeta::new(board_pda, false),      // 3: board PDA
            AccountMeta::new(miner_pda, false),      // 4: miner PDA
            AccountMeta::new(round_pda, false),      // 5: round PDA
            AccountMeta::new_readonly(system_program, false), // 6: system program
            AccountMeta::new(entropy_var_pda, false), // 7: entropy VAR
            AccountMeta::new_readonly(entropy_program, false), // 8: entropy program
        ],
        data: Deploy {
            cell_indices: vec![0, 1, 2, 3, 4], // Deploy to first 5 cells
        }
        .to_bytes(),
    };

    println!("🔨 Built Deploy instruction:");
    println!("   Program: {}", ore_program);
    println!("   Deploying to cells: [0, 1, 2, 3, 4]");
    println!("   Total cost: ~0.01 SOL (5 cells × 0.002 SOL)");

    // Get recent blockhash
    let blockhash = rpc.get_latest_blockhash()?;

    // Build transaction
    let tx =
        Transaction::new_signed_with_payer(&[deploy_ix], Some(&authority), &[&wallet], blockhash);

    println!("\n🚀 Submitting transaction with skip_preflight=true");
    println!("   (This bypasses simulation for first-time wallet account creation)\n");

    // Send transaction with skip_preflight
    let config = RpcSendTransactionConfig {
        skip_preflight: true,
        ..Default::default()
    };

    match rpc.send_transaction_with_config(&tx, config) {
        Ok(signature) => {
            println!("✅ Transaction submitted successfully!");
            println!("   Signature: {}", signature);
            println!("\n📊 View on Solscan:");
            println!("   https://solscan.io/tx/{}", signature);
            println!("\n⏳ Waiting for confirmation (60s timeout)...");

            // Wait for confirmation
            match rpc.confirm_transaction_with_spinner(
                &signature,
                &blockhash,
                CommitmentConfig::confirmed(),
            ) {
                Ok(_) => {
                    println!("✅ Transaction CONFIRMED!");

                    // Check new balance
                    let new_balance = rpc.get_balance(&authority)?;
                    let cost = (balance - new_balance) as f64 / 1e9;
                    println!("\n💰 New balance: {} SOL", new_balance as f64 / 1e9);
                    println!("   Transaction cost: {} SOL", cost);

                    // Check if miner account was created
                    match rpc.get_account(&miner_pda) {
                        Ok(account) => {
                            println!("\n✅ Miner account created successfully!");
                            println!("   Address: {}", miner_pda);
                            println!("   Owner: {}", account.owner);
                            println!("   Data length: {} bytes", account.data.len());
                        }
                        Err(e) => {
                            println!("\n⚠️  Miner account check failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Transaction confirmation failed: {}", e);
                    println!("\n💡 The transaction may still succeed on-chain");
                    println!("   Check the signature on Solscan (link above)");
                }
            }
        }
        Err(e) => {
            println!("❌ Transaction submission failed: {}", e);
            println!("\n🔍 Error details:");
            println!("{:#?}", e);
        }
    }

    Ok(())
}
