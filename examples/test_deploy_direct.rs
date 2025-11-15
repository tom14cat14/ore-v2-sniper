// Direct Deploy transaction test
// Tests the skip_preflight fix without waiting for round timing
// Uses bot's instruction builder with minimal dependencies

use anyhow::Result;
use ore_sniper::ore_instructions::{build_deploy_instruction, get_board_address};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;

fn main() -> Result<()> {
    println!("🧪 Deploy Transaction Test");
    println!("==========================\n");

    // Load env
    dotenvy::dotenv().ok();

    // Load wallet from environment
    let wallet_key = std::env::var("WALLET_PRIVATE_KEY").expect("WALLET_PRIVATE_KEY not set");
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

    // Read board account to get current round
    let board_data = rpc.get_account_data(&board_pda)?;
    if board_data.len() < 32 {
        anyhow::bail!(
            "Invalid board data length: {} (expected 32)",
            board_data.len()
        );
    }

    // Board structure: 8-byte discriminator + 3 u64 fields (round_id, start_slot, end_slot)
    // Extract round ID from board (bytes 8-16)
    let round_id = u64::from_le_bytes(board_data[8..16].try_into()?);

    println!("   Board PDA: {}", board_pda);
    println!("   Current round: {}\n", round_id);

    // Select cells to deploy to (first 5 cells for testing)
    let mut squares = [false; 25];
    let cells_to_deploy: Vec<usize> = (0..5).collect();

    for &i in &cells_to_deploy {
        squares[i] = true;
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
                    println!("\n✅ Transaction CONFIRMED!");

                    // Check new balance
                    let new_balance = rpc.get_balance(&authority)?;
                    let cost = (balance - new_balance) as f64 / 1e9;
                    println!("\n💰 New balance: {} SOL", new_balance as f64 / 1e9);
                    println!("   Transaction cost: {} SOL", cost);

                    println!("\n🎉 DEPLOYMENT TEST SUCCESSFUL!");
                    println!("   The skip_preflight fix worked correctly");
                    println!("   Miner account created and Deploy executed successfully");
                }
                Err(e) => {
                    println!("\n❌ Transaction confirmation failed: {}", e);
                    println!("\n💡 The transaction may still succeed on-chain");
                    println!("   Check the signature on Solscan (link above)");
                }
            }
        }
        Err(e) => {
            println!("\n❌ Transaction submission failed!");
            println!("\n🔍 Error details:");
            println!("{}", e);

            // Check if it's the "Invalid account owner" error
            let error_str = format!("{}", e);
            if error_str.contains("Invalid account owner")
                || error_str.contains("simulation failed")
            {
                println!("\n⚠️  This looks like a simulation error!");
                println!("   The skip_preflight=true should have prevented this.");
                println!("   Possible causes:");
                println!("   1. skip_preflight not applied in bot code");
                println!("   2. Transaction structure issue");
                println!("   3. Account derivation mismatch");
            } else if error_str.contains("already deployed")
                || error_str.contains("already claimed")
            {
                println!("\n💡 Cells may already be deployed in this round");
                println!("   This is expected if testing multiple times");
                println!("   Wait for next round (~60s) to test fresh deployment");
            }

            return Err(e.into());
        }
    }

    Ok(())
}
