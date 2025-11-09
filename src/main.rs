use anyhow::Result;
use ore_sniper::{OreConfig, OreBoardSniper};
use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
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

    // Create board sniper
    let mut sniper = OreBoardSniper::new(config)?;

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🚀 Starting Ore Board Sniper...");
    info!("📊 Strategy: Snipe cheapest cells <2.8s before reset");
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
            info!("   Total snipes: {}", stats.total_snipes);
            info!("   Successful: {}", stats.successful_snipes);
            info!("   Failed: {}", stats.failed_snipes);
            info!("   Total spent: {:.6} SOL", stats.total_spent_sol);
            info!("   Total earned: {:.6} SOL", stats.total_earned_sol);
            info!("   Net profit: {:.6} SOL", stats.total_earned_sol - stats.total_spent_sol);

            Err(e)
        }
    }
}

fn print_config_summary(config: &OreConfig) {
    info!("⚙️  Configuration Summary:");
    info!("   Mode: {}", if config.paper_trading { "📝 PAPER TRADING" } else { "💰 LIVE TRADING" });
    info!("   Min EV: {:.1}%", config.min_ev_percentage);
    info!("   Snipe window: {:.1}s before reset", 2.8);
    info!("   Max claim cost: {:.4} SOL", config.max_claim_cost_sol);
    info!("   Daily limits: {} claims, {:.2} SOL max loss",
        config.max_daily_claims, config.max_daily_loss_sol);
    info!("   Jito endpoint: {}", config.jito_endpoint);
    info!("   ShredStream: ✅ Native integration");
}
