// Manual Deploy transaction test using bot's own instruction builder
// Tests the skip_preflight fix without waiting for round timing

use anyhow::Result;
use ore_sniper::ore_instructions::{build_deploy_instruction, get_board_address};
use ore_sniper::ore_rpc::fetch_board_and_round;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use std::str::FromStr;

fn main() -> Result<()> {
    println!("🧪 Deploy Transaction Test");
    println!("==========================\n");

    // Load wallet from environment
    let wallet_key = std::env::var("WALLET_PRIVATE_KEY").expect("WALLET_PRIVATE_KEY not set");
    let wallet_bytes = bs58::decode(&wallet_key)
        .into_vec()
        .expect("Invalid wallet key");
    let wallet = Keypair::from_base58_string(&wallet_key);

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

    // Get current board state
    let board_pda = get_board_address()?;
    println!("📊 Fetching current board state...");
    let (round_id, deployed_cells) = fetch_board_and_round(&rpc)?;
    println!("   Board PDA: {}", board_pda);
    println!("   Current round: {}", round_id);
    println!("   Deployed cells: {}/25\n", deployed_cells.len());

    // Select cells to deploy to (first 5 undeploy cells, or [0-4] if testing)
    let mut squares = [false; 25];
    let mut cells_to_deploy = Vec::new();

    // For testing: just try cells 0-4 (they might be deployed already, that's ok)
    for i in 0..5 {
        squares[i] = true;
        cells_to_deploy.push(i);
    }

    let amount_per_cell = 2_000_000u64; // 0.002 SOL per cell
    let total_cost = amount_per_cell * cells_to_deploy.len() as u64;

    println!("🔨 Building Deploy instruction:");
    println!("   Cells: {:?}", cells_to_deploy);
    println!("   Amount per cell: {} SOL", amount_per_cell as f64 / 1e9);
    println!("   Total cost: {} SOL\n", total_cost as f64 / 1e9);

    // Build Deploy instruction using bot's builder
    let deploy_ix = build_deploy_instruction(
        authority, // signer
        authority, // authority
        amount_per_cell,
        round_id,
        squares,
    )?;

    println!("✅ Deploy instruction built");
    println!("   Program: {}", deploy_ix.program_id);
    println!("   Accounts: {}", deploy_ix.accounts.len());
    println!("   Data length: {} bytes\n", deploy_ix.data.len());

    // Get recent blockhash
    let blockhash = rpc.get_latest_blockhash()?;

    // Build transaction
    let tx =
        Transaction::new_signed_with_payer(&[deploy_ix], Some(&authority), &[&wallet], blockhash);

    println!("🚀 Submitting transaction with skip_preflight=true");
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

                    println!("\n✅ DEPLOYMENT TEST SUCCESSFUL!");
                    println!("   The skip_preflight fix worked correctly");
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

            // Check if it's the "Invalid account owner" error
            let error_str = format!("{:?}", e);
            if error_str.contains("Invalid account owner") {
                println!("\n⚠️  This is the 'Invalid account owner' error we're trying to fix!");
                println!("   The skip_preflight fix should have prevented this.");
                println!("   This suggests the fix wasn't applied correctly.");
            }
        }
    }

    Ok(())
}
