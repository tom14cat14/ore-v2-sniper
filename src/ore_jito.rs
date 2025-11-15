// ore_jito.rs — Simplified Jito bundle submission for Ore lottery
// Based on MEV_Bot's jito_bundle_client.rs but simplified for lottery betting

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    instruction::Instruction,
    transaction::VersionedTransaction,
    signature::{Keypair, Signer},
    pubkey::Pubkey,
    system_instruction,
    compute_budget::ComputeBudgetInstruction,
    message::{VersionedMessage, v0},
    hash::Hash,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::info;
use uuid::Uuid;

/// JITO Tip Floor API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TipFloorResponse {
    #[serde(rename = "landed_tips_99th_percentile")]
    pub landed_tips_99th: f64,
}

/// Cached tip floor data
#[derive(Debug, Clone)]
pub struct CachedTipFloor {
    pub data: TipFloorResponse,
    pub fetched_at: Instant,
}

impl CachedTipFloor {
    pub fn is_expired(&self) -> bool {
        // Refresh every 10 minutes
        self.fetched_at.elapsed() > Duration::from_secs(600)
    }
}

/// Bundle submission request (JITO API format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSubmissionRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Vec<Vec<String>>, // Array of arrays of base58 encoded transactions
}

/// Bundle submission response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSubmissionResponse {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<String>,
    pub error: Option<JitoError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JitoError {
    pub code: i32,
    pub message: String,
}

/// Simplified Jito client for Ore lottery betting
#[derive(Clone)]
pub struct OreJitoClient {
    client: Client,
    block_engine_url: String,
    tip_accounts: Vec<Pubkey>,
    cached_tip_floor: Arc<Mutex<Option<CachedTipFloor>>>,
    last_submit: Arc<Mutex<Instant>>,
}

impl OreJitoClient {
    /// Create new Jito client
    pub fn new(block_engine_url: String) -> Self {
        // Official Jito tip accounts (mainnet-beta)
        let tip_accounts = vec![
            "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".parse().unwrap(),
            "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe".parse().unwrap(),
            "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY".parse().unwrap(),
            "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49".parse().unwrap(),
            "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh".parse().unwrap(),
            "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt".parse().unwrap(),
            "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL".parse().unwrap(),
            "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT".parse().unwrap(),
        ];

        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            block_engine_url,
            tip_accounts,
            cached_tip_floor: Arc::new(Mutex::new(None)),
            last_submit: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(2))),
        }
    }

    /// Fetch JITO tip floor (99th percentile)
    pub async fn fetch_tip_floor(&self) -> Result<f64> {
        // Check cache first
        {
            let cache = self.cached_tip_floor.lock().unwrap();
            if let Some(cached) = cache.as_ref() {
                if !cached.is_expired() {
                    return Ok(cached.data.landed_tips_99th);
                }
            }
        }

        // Fetch from API
        let url = format!("{}/api/v1/bundles/tip_floor", self.block_engine_url);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch tip floor: {}", e))?;

        let tip_floor: Vec<TipFloorResponse> = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse tip floor response: {}", e))?;

        if tip_floor.is_empty() {
            return Err(anyhow!("Empty tip floor response"));
        }

        let percentile_99th = tip_floor[0].landed_tips_99th;

        // Update cache
        {
            let mut cache = self.cached_tip_floor.lock().unwrap();
            *cache = Some(CachedTipFloor {
                data: tip_floor[0].clone(),
                fetched_at: Instant::now(),
            });
        }

        info!("💰 JITO tip floor (99th): {:.6} SOL", percentile_99th);
        Ok(percentile_99th)
    }

    /// Calculate dynamic tip based on EV margin
    ///
    /// Strategy: Use 99th percentile as baseline, scale based on profit margin
    /// - High EV (>50%): 2.0x multiplier (very aggressive)
    /// - Medium EV (20-50%): 1.5x multiplier (aggressive)
    /// - Low EV (15-20%): 1.0x multiplier (baseline)
    pub async fn calculate_dynamic_tip(&self, ev_percentage: f64, bet_amount_sol: f64) -> Result<u64> {
        let base_tip = self.fetch_tip_floor().await?;

        // Calculate multiplier based on EV
        let multiplier = if ev_percentage > 0.5 {
            2.0  // Very high EV - be aggressive
        } else if ev_percentage > 0.2 {
            1.5  // Medium EV - moderately aggressive
        } else {
            1.0  // Low EV - use baseline
        };

        let tip_sol = base_tip * multiplier;

        // Cap at 1% of bet (don't pay more than 1% in tips)
        let max_tip_sol = bet_amount_sol * 0.01;
        let final_tip_sol = tip_sol.min(max_tip_sol);

        // Convert to lamports
        let tip_lamports = (final_tip_sol * 1_000_000_000.0) as u64;

        info!("💰 Dynamic tip: {:.6} SOL (EV: {:.1}%, multiplier: {:.1}x)",
              final_tip_sol, ev_percentage * 100.0, multiplier);

        Ok(tip_lamports)
    }

    /// Build Jito bundle with Deploy instruction + tip
    pub fn build_bundle(
        &self,
        deploy_ix: Instruction,
        tip_lamports: u64,
        wallet: &Keypair,
        recent_blockhash: Hash,
    ) -> Result<Vec<VersionedTransaction>> {
        // Select random tip account
        let tip_account = self.tip_accounts[0]; // Simplest: use first account

        // Build tip instruction
        let tip_ix = system_instruction::transfer(
            &wallet.pubkey(),
            &tip_account,
            tip_lamports,
        );

        // Add compute budget for priority
        let compute_units = 200_000;
        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(compute_units);
        let compute_price_ix = ComputeBudgetInstruction::set_compute_unit_price(1_000); // Micro-lamports

        // Build versioned transaction
        let message = v0::Message::try_compile(
            &wallet.pubkey(),
            &[
                compute_limit_ix,
                compute_price_ix,
                deploy_ix,
                tip_ix,
            ],
            &[],
            recent_blockhash,
        ).map_err(|e| anyhow!("Failed to compile message: {}", e))?;

        let versioned_message = VersionedMessage::V0(message);
        let tx = VersionedTransaction::try_new(versioned_message, &[wallet])
            .map_err(|e| anyhow!("Failed to sign transaction: {}", e))?;

        Ok(vec![tx])
    }

    /// Submit bundle to JITO with rate limiting
    pub async fn submit_bundle(
        &self,
        bundle: Vec<VersionedTransaction>,
    ) -> Result<String> {
        // Rate limiting: ensure 1.1s between submissions
        // Calculate sleep duration and drop lock BEFORE await (fixes !Send issue)
        let sleep_duration = {
            let mut last_submit = self.last_submit.lock().unwrap();
            let elapsed = last_submit.elapsed();
            let duration = if elapsed < Duration::from_millis(1100) {
                Some(Duration::from_millis(1100) - elapsed)
            } else {
                None
            };
            *last_submit = Instant::now();
            duration
        };

        // Sleep if needed (lock is dropped, future is now Send)
        if let Some(duration) = sleep_duration {
            tokio::time::sleep(duration).await;
        }

        // Encode transactions as base58
        let encoded_txs: Vec<String> = bundle
            .iter()
            .map(|tx| {
                let serialized = bincode::serialize(tx).unwrap();
                bs58::encode(serialized).into_string()
            })
            .collect();

        // Build JITO API request
        let request = BundleSubmissionRequest {
            jsonrpc: "2.0".to_string(),
            id: Uuid::new_v4().to_string(),
            method: "sendBundle".to_string(),
            params: vec![encoded_txs],
        };

        // Submit to JITO
        let url = format!("{}/api/v1/bundles", self.block_engine_url);

        info!("📤 Submitting bundle to JITO: {}", url);

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to submit bundle: {}", e))?;

        let result: BundleSubmissionResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse bundle response: {}", e))?;

        if let Some(error) = result.error {
            return Err(anyhow!("Bundle submission error: {} (code {})", error.message, error.code));
        }

        let bundle_id = result.result.ok_or_else(|| anyhow!("No bundle ID in response"))?;

        info!("✅ Bundle submitted: {}", bundle_id);
        Ok(bundle_id)
    }

    /// Build a tip instruction for JITO
    fn build_tip_instruction(&self, payer: Pubkey, tip_lamports: u64) -> Result<Instruction> {
        use solana_sdk::system_instruction;

        // Select random tip account (use first for simplicity)
        let tip_account = self.tip_accounts[0];

        // Build transfer instruction
        Ok(system_instruction::transfer(
            &payer,
            &tip_account,
            tip_lamports,
        ))
    }

    /// Submit a checkpoint (claim) bundle to JITO
    pub async fn submit_checkpoint_bundle(
        &self,
        checkpoint_ix: Instruction,
        tip_lamports: u64,
        wallet: &Keypair,
        recent_blockhash: Hash,
    ) -> Result<String> {
        use solana_sdk::transaction::VersionedTransaction;
        use solana_sdk::message::{Message, VersionedMessage};

        // Build tip instruction
        let tip_ix = self.build_tip_instruction(wallet.pubkey(), tip_lamports)?;

        // Build message with checkpoint + tip
        let message = Message::new_with_blockhash(
            &[checkpoint_ix, tip_ix],
            Some(&wallet.pubkey()),
            &recent_blockhash,
        );

        // Sign transaction
        let tx = VersionedTransaction::try_new(
            VersionedMessage::Legacy(message),
            &[wallet],
        )?;

        // Submit via existing submit_bundle method
        self.submit_bundle(vec![tx]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires network access
    async fn test_fetch_tip_floor() {
        let client = OreJitoClient::new(
            "https://mainnet.block-engine.jito.wtf".to_string()
        );

        match client.fetch_tip_floor().await {
            Ok(tip) => {
                println!("99th percentile tip: {}", tip);
                assert!(tip > 0.0);
            }
            Err(e) => {
                println!("Failed to fetch tip floor: {}", e);
            }
        }
    }
}
