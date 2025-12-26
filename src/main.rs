use anyhow::Result;
use ore_sniper::{OreBoardSniper, OreConfig};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("🎯 Ore Board Sniper v0.3.0 - Real Ore V2 Protocol");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Load configuration
    let config = match OreConfig::from_env() {
        Ok(cfg) => {
            info!("✅ Configuration loaded from environment");
            cfg
        }
        Err(e) => {
            error!("❌ Failed to load configuration: {}", e);
            error!("💡 Tip: Create a .env file with required settings");
            return Err(e);
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        error!("❌ Configuration validation failed: {}", e);
        return Err(e);
    }

    // Print configuration summary
    print_config_summary(&config);

    // Perform startup health checks
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🔍 Performing startup health checks...");

    if let Err(e) = perform_health_checks(&config).await {
        error!("❌ Health check failed: {}", e);
        error!("💡 Tip: Check your RPC endpoint and network connection");
        return Err(e);
    }

    info!("✅ All health checks passed");

    // Create board sniper
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🔧 Initializing Ore Board Sniper...");
    let mut sniper = OreBoardSniper::new(config).await?;

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🚀 Starting Ore Board Sniper...");
    info!("📊 Strategy: Snipe cheapest cells in final window");
    info!("⚡ Target latency: <150ms E2E");

    match sniper.run().await {
        Ok(_) => {
            info!("✅ Ore Board Sniper completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("❌ Ore Board Sniper error: {}", e);

            // Print final stats
            let stats = sniper.get_stats();
            info!("📊 Final Statistics:");
            info!("   Round-level:");
            info!("     Rounds played: {}", stats.rounds_played);
            info!("     Rounds won: {}", stats.rounds_won);
            info!("     Rounds lost: {}", stats.rounds_lost);
            info!("     Round win rate: {:.1}%", if stats.rounds_played > 0 {
                (stats.rounds_won as f64 / stats.rounds_played as f64) * 100.0
            } else { 0.0 });
            info!("   Pick-level:");
            info!("     Picks made: {}", stats.picks_made);
            info!("     Picks won: {}", stats.picks_won);
            info!("     Pick win rate: {:.1}%", if stats.picks_made > 0 {
                (stats.picks_won as f64 / stats.picks_made as f64) * 100.0
            } else { 0.0 });
            info!("   Financial:");
            info!("     Total spent: {:.6} SOL", stats.total_spent_sol);
            info!("     Total earned: {:.6} SOL", stats.total_earned_sol);
            info!("     Net profit: {:.6} SOL", stats.total_earned_sol - stats.total_spent_sol);

            Err(e)
        }
    }
}

fn print_config_summary(config: &OreConfig) {
    info!("⚙️  Configuration Summary:");
    info!(
        "   Mode: {}",
        if config.paper_trading {
            "📝 PAPER TRADING (SAFE - No real SOL spent)"
        } else if config.enable_real_trading {
            "💰 LIVE TRADING (DANGER - Real money!)"
        } else {
            "⚠️  Invalid config - check PAPER_TRADING and ENABLE_REAL_TRADING"
        }
    );
    info!("   Min EV: {:.1}%", config.min_ev_percentage);
    info!(
        "   Snipe window: {}s before reset",
        config.snipe_window_seconds
    );
    info!(
        "   Deployment per cell: {:.4} SOL",
        config.deployment_per_cell_sol
    );
    info!(
        "   Max cost per round: {:.4} SOL",
        config.max_cost_per_round_sol
    );
    info!(
        "   Daily limits: {} claims, {:.2} SOL max loss",
        config.max_daily_claims, config.max_daily_loss_sol
    );
    info!("   RPC: {}", config.rpc_url);
    info!(
        "   ShredStream: {}",
        if config.use_shredstream_timing {
            "✅ Enabled"
        } else {
            "❌ Disabled"
        }
    );
}

async fn perform_health_checks(config: &OreConfig) -> Result<()> {
    use solana_client::rpc_client::RpcClient;

    info!("   Checking RPC connection...");
    let rpc = RpcClient::new(config.rpc_url.clone());

    // Test RPC connection
    match rpc.get_health() {
        Ok(_) => info!("   ✅ RPC connection healthy"),
        Err(e) => {
            return Err(anyhow::anyhow!("RPC health check failed: {}", e));
        }
    }

    // Get current slot to verify RPC is responsive
    match rpc.get_slot() {
        Ok(slot) => info!("   ✅ RPC responsive (current slot: {})", slot),
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to get current slot: {}", e));
        }
    }

    // Validate wallet key format if real trading
    if config.enable_real_trading {
        info!("   Checking wallet configuration...");
        if config.wallet_private_key == "REPLACE_WITH_YOUR_BASE58_PRIVATE_KEY"
            || config.wallet_private_key.len() < 32
        {
            return Err(anyhow::anyhow!(
                "Invalid WALLET_PRIVATE_KEY - please set your actual wallet key in .env"
            ));
        }
        info!("   ✅ Wallet configuration looks valid");
    }

    Ok(())
}
