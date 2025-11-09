use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;

/// Configuration for Ore Grid Sniper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OreConfig {
    // Ore API settings
    pub ore_api_url: String,
    pub ore_program_id: String,

    // Strategy parameters
    pub min_ev_percentage: f64,           // Minimum expected value (default: 15%)
    pub snipe_window_seconds: u64,        // Window before reset to snipe (default: 3s)
    pub reset_interval_seconds: u64,      // Ore grid reset interval (default: 60s)
    pub ore_price_sol: f64,               // Fallback Ore price in SOL

    // Safety limits
    pub max_claim_cost_sol: f64,          // Maximum cost per claim (default: 0.05 SOL)
    pub max_daily_claims: u32,            // Daily claim limit (default: 100)
    pub max_daily_loss_sol: f64,          // Daily loss limit (default: 0.5 SOL)
    pub min_wallet_balance_sol: f64,      // Minimum wallet balance to maintain (default: 0.1 SOL)

    // Jito settings
    pub jito_endpoint: String,
    pub jito_tip_lamports: u64,           // Base tip (default: 50,000 lamports)

    // RPC settings
    pub rpc_url: String,
    pub ws_url: String,

    // Wallet
    pub wallet_private_key: String,

    // Paper trading
    pub paper_trading: bool,
    pub enable_real_trading: bool,

    // ShredStream (optional - for timing precision)
    pub shredstream_endpoint: Option<String>,
    pub use_shredstream_timing: bool,

    // Performance
    pub polling_interval_ms: u64,         // Grid polling frequency (default: 100ms)
    pub max_retries: u32,                 // Max retries per bundle (default: 3)
}

impl OreConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok(); // Load .env if exists

        Ok(Self {
            // Ore API settings
            ore_api_url: env::var("ORE_API_URL")
                .unwrap_or_else(|_| "https://ore.supply/v1/grid".to_string()),
            ore_program_id: env::var("ORE_PROGRAM_ID")
                .unwrap_or_else(|_| "oreoN2tQbHXVaZcohgZJ4H2qQvY8kU7B5b6t3Yc3V3Yc".to_string()),

            // Strategy parameters
            min_ev_percentage: env::var("MIN_EV_PERCENTAGE")
                .unwrap_or_else(|_| "15.0".to_string())
                .parse()?,
            snipe_window_seconds: env::var("SNIPE_WINDOW_SECONDS")
                .unwrap_or_else(|_| "3".to_string())
                .parse()?,
            reset_interval_seconds: env::var("RESET_INTERVAL_SECONDS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()?,
            ore_price_sol: env::var("ORE_PRICE_SOL")
                .unwrap_or_else(|_| "0.00072".to_string())
                .parse()?,

            // Safety limits
            max_claim_cost_sol: env::var("MAX_CLAIM_COST_SOL")
                .unwrap_or_else(|_| "0.05".to_string())
                .parse()?,
            max_daily_claims: env::var("MAX_DAILY_CLAIMS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()?,
            max_daily_loss_sol: env::var("MAX_DAILY_LOSS_SOL")
                .unwrap_or_else(|_| "0.5".to_string())
                .parse()?,
            min_wallet_balance_sol: env::var("MIN_WALLET_BALANCE_SOL")
                .unwrap_or_else(|_| "0.1".to_string())
                .parse()?,

            // Jito settings
            jito_endpoint: env::var("JITO_ENDPOINT")
                .unwrap_or_else(|_| "https://ny.mainnet.block-engine.jito.wtf".to_string()),
            jito_tip_lamports: env::var("JITO_TIP_LAMPORTS")
                .unwrap_or_else(|_| "50000".to_string())
                .parse()?,

            // RPC settings
            rpc_url: env::var("RPC_URL")
                .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string()),
            ws_url: env::var("WS_URL")
                .unwrap_or_else(|_| "wss://api.mainnet-beta.solana.com".to_string()),

            // Wallet
            wallet_private_key: env::var("WALLET_PRIVATE_KEY")
                .expect("WALLET_PRIVATE_KEY must be set"),

            // Paper trading
            paper_trading: env::var("PAPER_TRADING")
                .unwrap_or_else(|_| "true".to_string()) == "true",
            enable_real_trading: env::var("ENABLE_REAL_TRADING")
                .unwrap_or_else(|_| "false".to_string()) == "true",

            // ShredStream (optional)
            shredstream_endpoint: env::var("SHREDSTREAM_ENDPOINT").ok(),
            use_shredstream_timing: env::var("USE_SHREDSTREAM_TIMING")
                .unwrap_or_else(|_| "false".to_string()) == "true",

            // Performance
            polling_interval_ms: env::var("POLLING_INTERVAL_MS")
                .unwrap_or_else(|_| "80".to_string())
                .parse()?,
            max_retries: env::var("MAX_RETRIES")
                .unwrap_or_else(|_| "3".to_string())
                .parse()?,
        })
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.enable_real_trading && self.paper_trading {
            anyhow::bail!("Cannot enable both ENABLE_REAL_TRADING and PAPER_TRADING");
        }

        if !self.enable_real_trading && !self.paper_trading {
            anyhow::bail!("Must enable either ENABLE_REAL_TRADING or PAPER_TRADING");
        }

        if self.min_ev_percentage <= 0.0 {
            anyhow::bail!("MIN_EV_PERCENTAGE must be positive");
        }

        if self.max_claim_cost_sol <= 0.0 {
            anyhow::bail!("MAX_CLAIM_COST_SOL must be positive");
        }

        if self.min_wallet_balance_sol <= 0.0 {
            anyhow::bail!("MIN_WALLET_BALANCE_SOL must be positive");
        }

        Ok(())
    }

    /// Get EV threshold as decimal (15% -> 0.15)
    pub fn min_ev_decimal(&self) -> f64 {
        self.min_ev_percentage / 100.0
    }
}

/// Daily statistics tracker
#[derive(Debug, Clone, Default)]
pub struct DailyStats {
    pub claims_today: u32,
    pub total_spent_sol: f64,
    pub total_earned_sol: f64,
    pub successful_claims: u32,
    pub failed_claims: u32,
    pub reset_date: String, // YYYY-MM-DD
}

impl DailyStats {
    pub fn new() -> Self {
        Self {
            reset_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            ..Default::default()
        }
    }

    pub fn net_profit_sol(&self) -> f64 {
        self.total_earned_sol - self.total_spent_sol
    }

    pub fn should_reset(&self) -> bool {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        today != self.reset_date
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}
