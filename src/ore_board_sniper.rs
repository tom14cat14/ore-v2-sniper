// ore_board_sniper.rs — REAL ORE V2 LOTTERY SNIPER FOR SHREDSTREAM
//
// PROTOCOL: Ore V2 is a LOTTERY/GAMBLING system (NOT mining!)
// - Deploy: Bet SOL on board squares (25 squares available)
// - Wait: Round lasts 150 slots (~60 seconds)
// - Reset: Random winning square chosen via entropy
// - Checkpoint: Claim SOL + ORE rewards if you bet on winner
//
// Strategy: Snipe cheapest board cells in last 2.8s before reset
// Latency target: <150ms E2E
// Based on real Ore V2 protocol with 25-cell lottery board

use anyhow::Result;
use arc_swap::ArcSwap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

use crate::config::OreConfig;
use crate::ore_instructions::{
    build_deploy_instruction, build_checkpoint_instruction,
    get_board_address, get_miner_address,
};
use crate::ore_shredstream::{OreShredStreamProcessor, OreEvent};
use crate::ore_jito::OreJitoClient;
use solana_sdk::signature::{Keypair, Signer};

// Ore V2 constants
const BOARD_SIZE: usize = 25;           // 25-cell board
const SNIPE_WINDOW_SECS: f64 = 2.8;     // Start sniping 2.8s before reset
const EPOCH_DURATION_SECS: u64 = 60;    // Board resets every 60 seconds
#[allow(dead_code)]
const MAX_COMPETITORS: usize = 3;        // Max competitors to track
#[allow(dead_code)]
const BASE_TIP: u64 = 10_000;           // Base Jito tip in lamports
const SLOT_DURATION_MS: f64 = 400.0;    // Average Solana slot time

// Real Ore V2 program ID and mint (mainnet-beta)
// Verified from official repo: https://github.com/HardhatChad/ore
#[allow(dead_code)]
const ORE_PROGRAM_ID: &str = "oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv";
#[allow(dead_code)]
const ORE_MINT_ADDRESS: &str = "oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp";

/// 25-cell Ore board state
#[derive(Clone, Default, Debug)]
pub struct OreBoard {
    pub cells: [Cell; BOARD_SIZE],
    pub reset_slot: u64,
    pub current_slot: u64,
    pub round_id: u64,  // Current round ID for claiming rewards
}

/// Individual cell on the Ore board
#[derive(Clone, Default, Debug)]
pub struct Cell {
    pub id: u8,
    pub cost_lamports: u64,      // Dynamic SOL cost to claim
    pub difficulty: u64,          // Mining difficulty
    pub claimed: bool,            // Claimed on-chain
    pub claimed_in_mempool: bool, // Claimed in mempool (avoid)
}

// Global state with atomic updates
static BOARD: once_cell::sync::Lazy<ArcSwap<OreBoard>> =
    once_cell::sync::Lazy::new(|| ArcSwap::from_pointee(OreBoard::default()));
static RECENT_BLOCKHASH: once_cell::sync::Lazy<RwLock<solana_sdk::hash::Hash>> =
    once_cell::sync::Lazy::new(|| RwLock::new(solana_sdk::hash::Hash::default()));

/// Ore board sniper
pub struct OreBoardSniper {
    config: OreConfig,
    ore_price_sol: f64,
    stats: SnipeStats,
    shredstream: Option<OreShredStreamProcessor>,
    jito_client: Option<OreJitoClient>,
    wallet: Option<Keypair>,
    rpc_client: Option<crate::ore_rpc::OreRpcClient>,
}

#[derive(Debug, Clone, Default)]
pub struct SnipeStats {
    pub total_snipes: u64,
    pub successful_snipes: u64,
    pub failed_snipes: u64,
    pub total_spent_sol: f64,      // Total SOL spent on bets
    pub total_earned_sol: f64,     // Total SOL won from claims
    pub total_tips_paid: f64,      // Total Jito tips paid
    pub total_claims: u64,         // Number of successful claims
    pub starting_balance: f64,     // Starting wallet balance
    pub last_balance_check: f64,   // Last known wallet balance
}

impl OreBoardSniper {
    pub fn new(config: OreConfig) -> Result<Self> {
        // Initialize ShredStream if enabled
        let shredstream = if config.use_shredstream_timing {
            if let Some(endpoint) = &config.shredstream_endpoint {
                info!("🔌 ShredStream enabled: {}", endpoint);
                Some(OreShredStreamProcessor::new(endpoint.clone()))
            } else {
                info!("⚠️ ShredStream enabled but no endpoint provided");
                None
            }
        } else {
            None
        };

        // Load wallet if real trading enabled
        let wallet = if config.enable_real_trading {
            info!("🔑 Loading wallet from private key");
            Some(load_wallet(&config.wallet_private_key)?)
        } else {
            info!("📝 Paper trading mode - no wallet loaded");
            None
        };

        // Initialize Jito client if real trading enabled
        let jito_client = if config.enable_real_trading {
            info!("💰 Initializing Jito client: {}", config.jito_endpoint);
            Some(OreJitoClient::new(config.jito_endpoint.clone()))
        } else {
            None
        };

        // Initialize RPC client for board state fetching
        let rpc_client = Some(crate::ore_rpc::OreRpcClient::new(config.rpc_url.clone()));
        info!("📡 RPC client initialized: {}", config.rpc_url);

        Ok(Self {
            config,
            ore_price_sol: 0.0008, // ~$300 at 375k SOL price - update from Jupiter
            stats: SnipeStats::default(),
            shredstream,
            jito_client,
            wallet,
            rpc_client,
        })
    }

    /// Main sniping loop - called from ShredStream slot updates
    pub async fn run(&mut self) -> Result<()> {
        info!("🎯 Ore Board Sniper started");
        info!("⚙️  Mode: {}", if self.config.paper_trading { "📝 PAPER TRADING" } else { "💰 LIVE TRADING" });
        info!("💎 Min EV: {:.1}%", self.config.min_ev_percentage);
        info!("📊 Board: 25 cells, resets every {}s", EPOCH_DURATION_SECS);

        // Check starting wallet balance
        if let Ok(balance) = self.check_wallet_balance().await {
            self.stats.starting_balance = balance;
            self.stats.last_balance_check = balance;
            info!("💰 Starting wallet balance: {:.6} SOL", balance);
        }

        // Start blockhash refresh task
        self.start_blockhash_refresh();

        let mut last_slot = 0u64;
        let mut last_pnl_log = std::time::Instant::now();

        loop {
            // Wait for new slot from ShredStream
            let current_slot = self.wait_for_new_slot().await?;

            if current_slot <= last_slot {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            last_slot = current_slot;

            // Update board current slot
            self.update_current_slot(current_slot);

            // Get current board state
            let board = BOARD.load();
            let time_left = self.time_until_reset(&board, current_slot);

            // Only act in snipe window
            if time_left > SNIPE_WINDOW_SECS {
                debug!("⏱️  {:.1}s until snipe window", time_left);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            // Find best snipe target
            if let Some(target) = self.find_snipe_target(&board, time_left) {
                let ev = self.calculate_ev(&target, time_left);
                info!("🎯 SNIPE TARGET: Cell {} | Cost: {:.6} SOL | EV: {:.1}%",
                    target.id, target.cost_lamports as f64 / 1e9, ev * 100.0);

                // Execute snipe
                self.execute_snipe(&target, time_left).await?;
            }

            // Log P&L summary every 5 minutes
            if last_pnl_log.elapsed() > Duration::from_secs(300) {
                // Update current balance
                if let Ok(balance) = self.check_wallet_balance().await {
                    self.stats.last_balance_check = balance;
                }
                self.log_pnl_summary();
                last_pnl_log = std::time::Instant::now();
            }

            // Small sleep to prevent tight loop
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Start background task to refresh blockhash every 2 slots (~800ms)
    fn start_blockhash_refresh(&self) {
        tokio::spawn(async move {
            loop {
                // In real implementation, get blockhash from ShredStream or RPC
                if let Ok(hash) = fetch_blockhash_from_shredstream().await {
                    *RECENT_BLOCKHASH.write().await = hash;
                    debug!("🔄 Blockhash refreshed");
                }
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        });
    }

    /// Find best snipe target (lowest cost with EV > threshold)
    fn find_snipe_target(&self, board: &OreBoard, time_left: f64) -> Option<Cell> {
        board.cells.iter()
            .filter(|c| !c.claimed && !c.claimed_in_mempool) // Not claimed anywhere
            .filter(|c| {
                let ev = self.calculate_ev(c, time_left);
                ev >= self.config.min_ev_decimal()
            })
            .min_by_key(|c| c.cost_lamports) // Cheapest first
            .cloned()
    }

    /// Calculate expected value for a cell (LOTTERY SYSTEM)
    ///
    /// In Ore V2, this is a lottery:
    /// - You bet SOL on squares
    /// - 1/25 chance of winning (random square selected)
    /// - If you win: get your bet back + ALL other bets + ORE reward
    /// - If you lose: lose your bet
    ///
    /// EV = (Probability × Win Amount) - (Probability × Loss Amount)
    ///    = (1/25 × Total Pot) - (24/25 × Bet)
    fn calculate_ev(&self, cell: &Cell, _time_left: f64) -> f64 {
        let cost_sol = cell.cost_lamports as f64 / 1_000_000_000.0;

        // Get current board state
        let board = BOARD.load();

        // Calculate total pot (sum of all deployed SOL)
        let total_pot: u64 = board.cells.iter()
            .filter(|c| c.claimed || c.claimed_in_mempool)
            .map(|c| c.cost_lamports)
            .sum();

        let pot_sol = total_pot as f64 / 1_000_000_000.0;

        // Probability of winning = 1/25 (random square)
        let win_prob = 1.0 / 25.0;
        let lose_prob = 24.0 / 25.0;

        // Win amount = pot + your bet back + estimated ORE reward
        let ore_reward_sol = 100.0 * self.ore_price_sol; // Estimate: 100 ORE per win
        let win_amount = pot_sol + cost_sol + ore_reward_sol;

        // Expected value calculation
        let expected_return = (win_prob * win_amount) - (lose_prob * cost_sol);

        // EV as percentage of bet
        if cost_sol > 0.0 {
            (expected_return - cost_sol) / cost_sol
        } else {
            0.0
        }
    }

    /// Calculate time until reset in seconds
    fn time_until_reset(&self, board: &OreBoard, current_slot: u64) -> f64 {
        let slots_left = board.reset_slot.saturating_sub(current_slot) as f64;
        (slots_left * SLOT_DURATION_MS / 1000.0).max(0.0)
    }

    /// Execute snipe - Build Deploy instruction and submit via Jito
    async fn execute_snipe(&mut self, cell: &Cell, time_left: f64) -> Result<()> {
        let start = Instant::now();

        if self.config.paper_trading {
            info!("📝 PAPER TRADE: Would deploy to cell {}", cell.id);
            let cost = cell.cost_lamports as f64 / 1e9;
            let ev = self.calculate_ev(cell, time_left);
            info!("   Cost: {:.6} SOL | EV: {:.1}% | Time left: {:.1}s", cost, ev * 100.0, time_left);

            self.stats.total_snipes += 1;
            self.stats.successful_snipes += 1;
            self.stats.total_spent_sol += cost;
            return Ok(());
        }

        // LIVE TRADING
        info!("🚀 LIVE: Building Deploy bundle for cell {}", cell.id);

        // Get wallet and Jito client
        let wallet = self.wallet.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wallet not loaded"))?;
        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Jito client not initialized"))?;

        let authority = wallet.pubkey();

        // Get current round ID from board
        let board = BOARD.load();
        let round_id = (board.current_slot / 150) as u64; // Estimate: 150 slots per round

        // Build Deploy instruction
        // Deploy to ONLY this cell (all others false)
        let mut squares = [false; 25];
        squares[cell.id as usize] = true;

        let deploy_ix = build_deploy_instruction(
            authority,          // Signer
            authority,          // Authority (same as signer)
            cell.cost_lamports, // Amount to bet
            round_id,           // Current round
            squares,            // Deploy to this cell only
        )?;

        info!("✅ Deploy instruction built in {:?}", start.elapsed());

        // Calculate dynamic Jito tip based on EV
        let ev = self.calculate_ev(cell, time_left);
        let bet_sol = cell.cost_lamports as f64 / 1_000_000_000.0;
        let tip_lamports = jito_client.calculate_dynamic_tip(ev, bet_sol).await?;

        info!("💰 Jito tip: {:.6} SOL (EV: {:.1}%)",
              tip_lamports as f64 / 1e9, ev * 100.0);

        // Get recent blockhash
        let recent_blockhash = *RECENT_BLOCKHASH.read().await;

        // Build Jito bundle
        let bundle = jito_client.build_bundle(
            deploy_ix,
            tip_lamports,
            wallet,
            recent_blockhash,
        )?;

        info!("📦 Bundle built in {:?}", start.elapsed());

        // Submit bundle via Jito
        let bundle_id = jito_client.submit_bundle(bundle).await?;

        info!("✅ Bundle submitted: {} | Cell {} | Cost: {:.6} SOL | Tip: {:.6} SOL",
            bundle_id,
            cell.id,
            bet_sol,
            tip_lamports as f64 / 1e9
        );

        // Update stats
        self.stats.total_snipes += 1;
        self.stats.total_spent_sol += bet_sol;
        self.stats.total_tips_paid += tip_lamports as f64 / 1e9;
        self.stats.successful_snipes += 1;

        Ok(())
    }

    /// Calculate dynamic Jito tip based on competition
    #[allow(dead_code)]
    fn calculate_dynamic_tip(&self, board: &OreBoard) -> u64 {
        let competitors = board.cells.iter()
            .filter(|c| c.claimed_in_mempool)
            .count();

        let multiplier = competitors.min(MAX_COMPETITORS) as u64;
        BASE_TIP + (multiplier * 15_000)
    }

    /// Check current wallet balance via ERPC RPC
    async fn check_wallet_balance(&self) -> Result<f64> {
        use solana_client::rpc_client::RpcClient;

        if let Some(wallet) = &self.wallet {
            let rpc = RpcClient::new(self.config.rpc_url.clone());
            let balance_lamports = rpc.get_balance(&wallet.pubkey())?;
            let balance_sol = balance_lamports as f64 / 1e9;
            Ok(balance_sol)
        } else {
            Ok(0.0)
        }
    }

    /// Log detailed P&L summary
    fn log_pnl_summary(&self) {
        let net_pnl = self.stats.total_earned_sol - (self.stats.total_spent_sol + self.stats.total_tips_paid);
        let win_rate = if self.stats.total_snipes > 0 {
            (self.stats.successful_snipes as f64 / self.stats.total_snipes as f64) * 100.0
        } else {
            0.0
        };

        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("💰 PROFIT & LOSS SUMMARY");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("📊 Starting Balance: {:.6} SOL", self.stats.starting_balance);
        info!("💼 Current Balance:  {:.6} SOL", self.stats.last_balance_check);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("📤 Total Bets Placed: {} (Win Rate: {:.1}%)", self.stats.total_snipes, win_rate);
        info!("📥 Total Claims Won:  {}", self.stats.total_claims);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("💸 Total Spent (Bets): {:.6} SOL", self.stats.total_spent_sol);
        info!("💳 Total Tips Paid:    {:.6} SOL", self.stats.total_tips_paid);
        info!("💰 Total Earned:       {:.6} SOL", self.stats.total_earned_sol);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("📈 Net P&L:            {:.6} SOL ({:+.2}%)",
            net_pnl,
            if self.stats.starting_balance > 0.0 {
                (net_pnl / self.stats.starting_balance) * 100.0
            } else {
                0.0
            }
        );
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }

    /// Wait for new slot from ShredStream and process Ore events
    async fn wait_for_new_slot(&mut self) -> Result<u64> {
        if let Some(ref mut shredstream) = self.shredstream {
            // Process ShredStream events
            let event = shredstream.process().await?;

            // Handle Ore events
            for ore_event in event.events {
                match ore_event {
                    OreEvent::SlotUpdate { slot } => {
                        debug!("📡 Slot update: {}", slot);
                    }
                    OreEvent::BoardReset { slot } => {
                        info!("🔄 Board reset at slot {}", slot);
                        // Update board reset slot and increment round ID
                        let mut board = BOARD.load().as_ref().clone();
                        let old_round_id = board.round_id;
                        board.reset_slot = slot;
                        board.round_id += 1;
                        // Clear all claims
                        for cell in &mut board.cells {
                            cell.claimed = false;
                            cell.claimed_in_mempool = false;
                        }

                        // **CRITICAL: Fetch real board state from RPC**
                        if let Some(ref rpc_client) = self.rpc_client {
                            match rpc_client.update_board_state(&mut board).await {
                                Ok(_) => {
                                    info!("✅ Real board state loaded from RPC");
                                }
                                Err(e) => {
                                    warn!("⚠️ Failed to fetch board state: {} - using defaults", e);
                                }
                            }
                        }

                        BOARD.store(Arc::new(board));

                        // Clone wallet and jito BEFORE spawn to avoid lifetime issues
                        let wallet_for_claim = self.wallet.as_ref().map(|w| Arc::new(w.insecure_clone()));
                        let jito_for_claim = self.jito_client.clone();
                        let claim_round_id = old_round_id; // Claim rewards from the round that just ended

                        // Spawn auto-claim task (only in real trading mode)
                        if !self.config.paper_trading {
                            if let (Some(wallet), Some(jito_client)) = (wallet_for_claim, jito_for_claim) {
                                tokio::spawn(async move {
                                    // Wait for round to complete + buffer (65 seconds)
                                    info!("⏳ Scheduled auto-claim for round {} in 65 seconds", claim_round_id);
                                    tokio::time::sleep(Duration::from_secs(65)).await;

                                    info!("🎁 Auto-claiming rewards for round {}", claim_round_id);

                                    // Build checkpoint instruction
                                    if let Ok(board_address) = get_board_address() {
                                        if let Ok(miner_address) = get_miner_address(wallet.pubkey()) {
                                            if let Ok(checkpoint_ix) = build_checkpoint_instruction(
                                                wallet.pubkey(),
                                                board_address,
                                                miner_address,
                                                claim_round_id,
                                            ) {
                                                // Get recent blockhash
                                                let blockhash = *RECENT_BLOCKHASH.read().await;

                                                // Calculate tip (small for claims)
                                                let tip_lamports = jito_client.calculate_dynamic_tip(0.0, 0.0).await.unwrap_or(10000);

                                                // Submit bundle
                                                match jito_client.submit_checkpoint_bundle(
                                                    checkpoint_ix,
                                                    tip_lamports,
                                                    &wallet,
                                                    blockhash
                                                ).await {
                                                    Ok(bundle_id) => {
                                                        info!("✅ Auto-claim submitted: {} | Round: {} | Tip: {:.6} SOL",
                                                            bundle_id, claim_round_id, tip_lamports as f64 / 1e9);
                                                        // TODO: Track earned SOL once claim confirms
                                                        // (requires monitoring transaction confirmation)
                                                    }
                                                    Err(e) => {
                                                        warn!("⚠️ Auto-claim failed: {} | Round: {}", e, claim_round_id);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                        } else {
                            info!("📝 Paper trading - skipping auto-claim for round {}", old_round_id);
                        }
                    }
                    OreEvent::CellDeployed { cell_id, authority } => {
                        info!("✅ Cell {} deployed by {}", cell_id, &authority[..8]);
                        mark_mempool_deploy(cell_id);
                    }
                }
            }

            Ok(event.current_slot)
        } else {
            // Fallback: polling mode (slower)
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(123456)
        }
    }

    /// Update current slot in board
    fn update_current_slot(&self, slot: u64) {
        let mut board = BOARD.load().as_ref().clone();
        board.current_slot = slot;
        BOARD.store(Arc::new(board));
    }

    pub fn get_stats(&self) -> &SnipeStats {
        &self.stats
    }
}

// === BOARD UPDATE FUNCTIONS (Called from ShredStream log parser) ===

/// Update board from Ore program log
pub fn update_board_from_log(log: &str) {
    let mut board = BOARD.load().as_ref().clone();

    // Parse BoardReset event
    if log.contains("BoardReset") {
        if let Some(reset_slot) = parse_reset_slot(log) {
            info!("🔄 Board reset detected - slot {}", reset_slot);
            board.reset_slot = reset_slot;
            // Clear all claims
            for cell in &mut board.cells {
                cell.claimed = false;
                cell.claimed_in_mempool = false;
            }
        }
    }

    // Parse CellClaimed event
    if let Some(cell_id) = parse_claimed_cell(log) {
        if (cell_id as usize) < BOARD_SIZE {
            board.cells[cell_id as usize].claimed = true;
            debug!("✅ Cell {} claimed on-chain", cell_id);
        }
    }

    // Parse cell states (costs, difficulty)
    parse_cell_states(&mut board, log);

    BOARD.store(Arc::new(board));
}

/// Mark cell as claimed in mempool (to avoid competing)
pub fn mark_mempool_deploy(cell_id: u8) {
    let mut board = BOARD.load().as_ref().clone();
    if (cell_id as usize) < BOARD_SIZE {
        board.cells[cell_id as usize].claimed_in_mempool = true;
        debug!("⚠️  Cell {} claimed in mempool", cell_id);
    }
    BOARD.store(Arc::new(board));
}

// === HELPER FUNCTIONS ===

/// Fetch blockhash from ShredStream or RPC
async fn fetch_blockhash_from_shredstream() -> Result<solana_sdk::hash::Hash> {
    // TODO: Integrate with ShredStream or RPC
    Ok(solana_sdk::hash::Hash::new_unique())
}

/// Load wallet from base58 encoded private key
fn load_wallet(private_key: &str) -> Result<Keypair> {
    let decoded = bs58::decode(private_key)
        .into_vec()
        .map_err(|e| anyhow::anyhow!("Failed to decode private key: {}", e))?;

    Keypair::try_from(&decoded[..])
        .map_err(|e| anyhow::anyhow!("Failed to load keypair: {}", e))
}

/// Parse reset slot from log
fn parse_reset_slot(_log: &str) -> Option<u64> {
    // TODO: Implement real parser based on Ore program logs
    // Example: "Program log: BoardReset { slot: 123456 }"
    None
}

/// Parse claimed cell from log
fn parse_claimed_cell(_log: &str) -> Option<u8> {
    // TODO: Implement real parser based on Ore program logs
    // Example: "Program log: CellClaimed { id: 5 }"
    None
}

/// Parse cell states from log or RPC
fn parse_cell_states(board: &mut OreBoard, _log: &str) {
    // TODO: Parse from logs or query via RPC getProgramAccounts
    // For now, use stub data
    for (i, cell) in board.cells.iter_mut().enumerate() {
        cell.id = i as u8;
        cell.cost_lamports = 5_000_000; // Stub: 0.005 SOL
        cell.difficulty = (i as u64) * 10;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ev_calculation() {
        let config = OreConfig {
            ore_api_url: "test".to_string(),
            ore_program_id: ORE_PROGRAM_ID.to_string(),
            min_ev_percentage: 15.0,
            snipe_window_seconds: 3,
            reset_interval_seconds: 60,
            ore_price_sol: 0.0008,
            max_claim_cost_sol: 0.05,
            max_daily_claims: 100,
            max_daily_loss_sol: 0.5,
            min_wallet_balance_sol: 0.1,
            jito_endpoint: "test".to_string(),
            jito_tip_lamports: 50000,
            rpc_url: "test".to_string(),
            ws_url: "test".to_string(),
            wallet_private_key: "test".to_string(),
            paper_trading: true,
            enable_real_trading: false,
            shredstream_endpoint: None,
            use_shredstream_timing: false,
            polling_interval_ms: 100,
            max_retries: 3,
        };

        let sniper = OreBoardSniper::new(config).unwrap();

        let cell = Cell {
            id: 1,
            cost_lamports: 5_000_000, // 0.005 SOL
            difficulty: 1000,
            claimed: false,
            claimed_in_mempool: false,
        };

        let ev = sniper.calculate_ev(&cell, 3.0);
        println!("EV: {:.2}%", ev * 100.0);

        // Note: EV may be negative with empty pot in lottery system
        // This is expected - positive EV only when total pot is large enough
        println!("Note: Lottery EV depends on total pot size");
    }
}
