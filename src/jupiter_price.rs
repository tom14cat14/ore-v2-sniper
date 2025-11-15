// jupiter_price.rs — Jupiter Price API client for ORE/SOL price
// Fetches real-time ORE price in SOL from Jupiter aggregator

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

const ORE_MINT: &str = "oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp";
const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const JUPITER_PRICE_API: &str = "https://lite-api.jup.ag/price/v3";

// Jupiter v3 returns flat dict mapping token address to price data
type JupiterPriceResponse = std::collections::HashMap<String, PriceData>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PriceData {
    usd_price: f64,        // Price in USD
    block_id: u64,         // Block ID
    decimals: u8,          // Token decimals
    price_change_24h: f64, // 24h price change %
}

/// Fetch ORE price in SOL from Jupiter Price API v3
/// Returns (ore_per_sol, ore_usd)
pub async fn fetch_ore_price() -> Result<(f64, f64)> {
    // Fetch both ORE and SOL prices to calculate ORE/SOL ratio
    let url = format!("{}?ids={},{}", JUPITER_PRICE_API, ORE_MINT, SOL_MINT);

    debug!("📡 Fetching ORE and SOL prices from Jupiter v3: {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch prices: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!("Jupiter API returned error: {}", response.status()));
    }

    let price_data: JupiterPriceResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Jupiter response: {}", e))?;

    // Get ORE price in USD
    let ore_usd = price_data
        .get(ORE_MINT)
        .ok_or_else(|| anyhow!("ORE price not found in response"))?
        .usd_price;

    // Get SOL price in USD
    let sol_usd = price_data
        .get(SOL_MINT)
        .ok_or_else(|| anyhow!("SOL price not found in response"))?
        .usd_price;

    // Calculate ORE price in SOL
    let ore_per_sol = ore_usd / sol_usd;

    info!(
        "💰 ORE Price: {:.8} SOL (ORE=${:.4}, SOL=${:.2})",
        ore_per_sol, ore_usd, sol_usd
    );

    Ok((ore_per_sol, ore_usd))
}

/// Cached ORE price fetcher (polls Jupiter every 30s)
pub struct OrePriceFetcher {
    cached_price: f64,
    cached_price_usd: f64,
    last_update: std::time::Instant,
    cache_duration: std::time::Duration,
}

impl OrePriceFetcher {
    pub fn new() -> Self {
        Self {
            cached_price: 0.0,
            cached_price_usd: 0.0,
            last_update: std::time::Instant::now() - std::time::Duration::from_secs(3600),
            cache_duration: std::time::Duration::from_secs(30), // 30s cache
        }
    }

    /// Get ORE price in SOL (uses cache if fresh, fetches if stale)
    pub async fn get_price(&mut self) -> Result<f64> {
        if self.last_update.elapsed() > self.cache_duration || self.cached_price == 0.0 {
            match fetch_ore_price().await {
                Ok((price_sol, price_usd)) => {
                    self.cached_price = price_sol;
                    self.cached_price_usd = price_usd;
                    self.last_update = std::time::Instant::now();
                    Ok(price_sol)
                }
                Err(e) => {
                    warn!("Failed to fetch ORE price, using cached: {}", e);
                    if self.cached_price > 0.0 {
                        Ok(self.cached_price) // Use stale cache on error
                    } else {
                        Err(e) // No cache available
                    }
                }
            }
        } else {
            Ok(self.cached_price)
        }
    }

    /// Get ORE price in USD (uses cache if fresh, fetches if stale)
    pub async fn get_price_usd(&mut self) -> Result<f64> {
        // Trigger fetch if needed
        self.get_price().await?;
        Ok(self.cached_price_usd)
    }

    /// Force refresh price (ignore cache)
    pub async fn refresh(&mut self) -> Result<f64> {
        self.last_update =
            std::time::Instant::now() - self.cache_duration - std::time::Duration::from_secs(1);
        self.get_price().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_fetch_ore_price() {
        match fetch_ore_price().await {
            Ok((price_sol, price_usd)) => {
                println!("ORE Price: {:.6} SOL, ${:.4} USD", price_sol, price_usd);
                assert!(price_sol > 0.0);
                assert!(price_usd > 0.0);
            }
            Err(e) => {
                println!("Failed to fetch ORE price: {}", e);
            }
        }
    }
}
