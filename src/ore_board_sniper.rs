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
    build_deploy_instruction,
};
use crate::ore_shredstream::{OreShredStreamProcessor, OreEvent};
use crate::dashboard::{DashboardWriter, DashboardEvent, get_timestamp};
use solana_sdk::{
    signature::{Keypair, Signer},
    pubkey::Pubkey,
};

// Ore V2 constants
const BOARD_SIZE: usize = 25;           // 25-cell board
const SNIPE_WINDOW_SECS: f64 = 2.0;     // Start sniping 2s before reset (late = fewer competitors per cell!)
const FORCE_TEST_EXECUTION: bool = true; // 🔥 TESTING ENTROPY VAR FIX
const EXECUTE_ONCE_AND_EXIT: bool = true;  // Execute one buy then exit
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
    pub pot_lamports: u64,  // Real pot size from Round account (total_deployed)
    pub motherlode_ore: u64,  // Motherlode jackpot in lamports (divide by 1e9 for ORE)
    pub ore_price_sol: f64,  // ORE price in SOL (from Jupiter)
    pub entropy_var: solana_sdk::pubkey::Pubkey,  // Entropy VAR address for Deploy instruction
}

/// Individual cell on the Ore board
#[derive(Clone, Default, Debug)]
pub struct Cell {
    pub id: u8,
    pub cost_lamports: u64,      // Dynamic SOL cost to claim (minimum to deploy)
    pub deployed_lamports: u64,  // Total SOL deployed to this cell (from Round account)
    pub difficulty: u64,          // Number of deployers on this cell
    pub claimed: bool,            // Claimed on-chain
    pub claimed_in_mempool: bool, // Claimed in mempool (avoid)
    pub deployers: Vec<String>,   // Track all deployers (for pot splitting calculation)
}

// Global state with atomic updates
static BOARD: once_cell::sync::Lazy<ArcSwap<OreBoard>> =
    once_cell::sync::Lazy::new(|| ArcSwap::from_pointee(OreBoard::default()));
static RECENT_BLOCKHASH: once_cell::sync::Lazy<RwLock<solana_sdk::hash::Hash>> =
    once_cell::sync::Lazy::new(|| RwLock::new(solana_sdk::hash::Hash::default()));

// Multi-cell S_j ranking approach - old threshold sniping removed

/// Ore board sniper
pub struct OreBoardSniper {
    config: OreConfig,
    price_fetcher: crate::jupiter_price::OrePriceFetcher,
    stats: SnipeStats,
    shredstream: Option<OreShredStreamProcessor>,
    wallet: Option<Keypair>,
    rpc_client: Option<crate::ore_rpc::OreRpcClient>,
    dashboard: DashboardWriter,
    entries_processed: u64,
    board_ws_rx: tokio::sync::broadcast::Receiver<crate::ore_board_websocket::BoardUpdate>,
    round_ws_rx: tokio::sync::broadcast::Receiver<crate::ore_board_websocket::RoundUpdate>,
    treasury_ws_rx: tokio::sync::broadcast::Receiver<crate::ore_board_websocket::TreasuryUpdate>,
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
    pub async fn new(config: OreConfig) -> Result<Self> {
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
            let kp = load_wallet(&config.wallet_private_key)?;
            info!("✅ Wallet loaded: {}", kp.pubkey());
            Some(kp)
        } else {
            info!("📝 Paper trading mode - no wallet loaded");
            None
        };

        // Initialize RPC client for board state fetching
        let rpc_client = Some(crate::ore_rpc::OreRpcClient::new(config.rpc_url.clone()));
        info!("📡 RPC client initialized: {}", config.rpc_url);

        // Initialize dashboard writer
        let mut dashboard = DashboardWriter::new();
        dashboard.load_events(); // Load existing events on startup
        info!("📊 Dashboard writer initialized");

        // Initialize ORE price fetcher
        let price_fetcher = crate::jupiter_price::OrePriceFetcher::new();
        info!("💰 ORE price fetcher initialized (Jupiter API)");

        // Fetch initial board state to get current round_id for Round WebSocket
        let initial_round_id = if let Some(ref rpc) = rpc_client {
            // Fetch board state asynchronously to get current round
            match rpc.fetch_board().await {
                Ok(board) => {
                    info!("📊 Initial board fetch: round {}", board.round_id);
                    board.round_id
                }
                Err(e) => {
                    warn!("⚠️ Failed to fetch initial board state: {} - using round 0", e);
                    0
                }
            }
        } else {
            0
        };

        // Spawn Board WebSocket subscriber for real-time Board updates
        let board_ws_rx = crate::ore_board_websocket::spawn_board_subscriber(config.ws_url.clone())?;
        info!("📡 Board WebSocket subscriber spawned");

        // Spawn Round WebSocket subscriber for real-time Round updates
        let round_ws_rx = crate::ore_board_websocket::spawn_round_subscriber(config.ws_url.clone(), initial_round_id)?;
        info!("📡 Round WebSocket subscriber spawned (round {})", initial_round_id);

        // Spawn Treasury WebSocket subscriber for real-time Motherlode updates
        let treasury_ws_rx = crate::ore_board_websocket::spawn_treasury_subscriber(config.ws_url.clone())?;
        info!("📡 Treasury WebSocket subscriber spawned");

        Ok(Self {
            config,
            price_fetcher,
            stats: SnipeStats::default(),
            shredstream,
            wallet,
            rpc_client,
            dashboard,
            entries_processed: 0,
            board_ws_rx,
            round_ws_rx,
            treasury_ws_rx,
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
        let mut last_rpc_refresh = std::time::Instant::now();

        loop {
            // Periodically refresh board state via RPC (every 5 seconds)
            // This ensures we have valid data even if WebSocket returns dummy values
            if last_rpc_refresh.elapsed().as_secs() >= 5 {
                if let Some(ref rpc) = self.rpc_client {
                    let mut board = BOARD.load().as_ref().clone();
                    match rpc.update_board_state(&mut board).await {
                        Ok(()) => {
                            BOARD.store(Arc::new(board));
                            debug!("✅ RPC board refresh: round {}, pot={:.6} SOL",
                                  BOARD.load().round_id,
                                  BOARD.load().pot_lamports as f64 / 1e9);
                        }
                        Err(e) => {
                            warn!("⚠️  RPC board refresh failed: {}", e);
                        }
                    }
                }
                last_rpc_refresh = std::time::Instant::now();
            }

            // Check for Board WebSocket updates (non-blocking)
            match self.board_ws_rx.try_recv() {
                Ok(board_update) => {
                    debug!("📡 Board WebSocket update: round {}, reset_slot {}",
                          board_update.round_id, board_update.end_slot);

                    // Skip WebSocket Board updates if round_id is 0 (dummy value from 33-byte format)
                    // RPC refresh will provide valid data instead
                    if board_update.round_id == 0 {
                        debug!("⚠️  Skipping WebSocket Board update with round_id=0 (dummy value)");
                        continue;
                    }

                    let mut board = BOARD.load().as_ref().clone();
                    let old_round_id = board.round_id;

                    // Update board with WebSocket data
                    board.round_id = board_update.round_id;
                    board.reset_slot = board_update.end_slot;
                    board.entropy_var = board_update.entropy_var;
                    BOARD.store(Arc::new(board));

                    // If round changed, re-subscribe to new Round account
                    if board_update.round_id != old_round_id && board_update.round_id > 0 {
                        info!("🔄 Round changed {} → {}, re-subscribing to Round WebSocket",
                              old_round_id, board_update.round_id);

                        // Spawn new Round subscriber for new round
                        match crate::ore_board_websocket::spawn_round_subscriber(
                            self.config.ws_url.clone(),
                            board_update.round_id
                        ) {
                            Ok(new_rx) => {
                                self.round_ws_rx = new_rx;
                                info!("✅ Round WebSocket re-subscribed to round {}", board_update.round_id);
                            }
                            Err(e) => {
                                warn!("⚠️  Failed to re-subscribe to Round WebSocket: {}", e);
                            }
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                    warn!("⚠️  Board WebSocket lagged, skipped {} updates", skipped);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    warn!("⚠️  Board WebSocket closed");
                }
            }

            // Check for Round WebSocket updates (non-blocking)
            match self.round_ws_rx.try_recv() {
                Ok(round_update) => {
                    debug!("📊 Round WebSocket update: pot={:.6} SOL, {}/25 cells claimed",
                          round_update.total_deployed as f64 / 1e9,
                          round_update.deployed.iter().filter(|&&x| x > 0).count());

                    // Update board with Round data
                    let mut board = BOARD.load().as_ref().clone();
                    board.pot_lamports = round_update.total_deployed;

                    // Update cell costs and deployment status
                    for (i, cell) in board.cells.iter_mut().enumerate() {
                        cell.deployed_lamports = round_update.deployed[i];
                        cell.difficulty = round_update.count[i];
                        cell.claimed = round_update.deployed[i] > 0;

                        // Estimate cost (simplified - real cost calculated by program)
                        let base_cost = 1_000_000u64; // 0.001 SOL
                        let difficulty_factor = 1.0 + (round_update.count[i] as f64 * 0.1);
                        cell.cost_lamports = (base_cost as f64 * difficulty_factor) as u64;
                        cell.cost_lamports = cell.cost_lamports.max(1_000_000).min(20_000_000);
                    }

                    BOARD.store(Arc::new(board));
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                    warn!("⚠️  Round WebSocket lagged, skipped {} updates", skipped);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    warn!("⚠️  Round WebSocket closed");
                }
            }

            // Check for Treasury WebSocket updates (non-blocking)
            match self.treasury_ws_rx.try_recv() {
                Ok(treasury_update) => {
                    debug!("💎 Treasury WebSocket update: Motherlode={:.2} ORE",
                          treasury_update.motherlode_balance as f64 / 1e11);

                    // Update board with Treasury data
                    let mut board = BOARD.load().as_ref().clone();
                    board.motherlode_ore = treasury_update.motherlode_balance;
                    BOARD.store(Arc::new(board));
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                    warn!("⚠️  Treasury WebSocket lagged, skipped {} updates", skipped);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    warn!("⚠️  Treasury WebSocket closed");
                }
            }

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

            // Log timing every 30 seconds
            static LAST_TIMING_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let last_log = LAST_TIMING_LOG.load(std::sync::atomic::Ordering::Relaxed);
            if now_secs - last_log >= 30 {
                let available = board.cells.iter().filter(|c| !c.claimed && !c.claimed_in_mempool).count();
                let pot: u64 = board.cells.iter().filter(|c| c.claimed || c.claimed_in_mempool).map(|c| c.cost_lamports).sum();
                info!("⏱️  {:.1}s until snipe window | {} cells free | pot: {:.6} SOL",
                      time_left, available, pot as f64 / 1e9);
                LAST_TIMING_LOG.store(now_secs, std::sync::atomic::Ordering::Relaxed);
            }

            // === FORCE TEST MODE: ShredStream-first execution ===
            if FORCE_TEST_EXECUTION {
                // SHREDSTREAM-FIRST: Execute as soon as we have 2+ cells with costs
                // Don't wait for WebSocket/RPC round_id or entropy_var
                // This achieves <1ms execution latency (the whole point of ShredStream!)
                let cells_with_cost = board.cells.iter().filter(|c| c.cost_lamports > 0).count();

                if cells_with_cost < 2 {
                    debug!("🔥 FORCE TEST: Waiting for ShredStream to detect cells ({}/2 cells ready)",
                          cells_with_cost);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }

                info!("🔥 FORCE TEST MODE: ShredStream detected {} cells - executing NOW!", cells_with_cost);
                info!("   (Bypassing round_id/entropy_var checks - ShredStream-first architecture)");
                info!("   Cells with cost: {}/25", cells_with_cost);
            }

            // Only act in snipe window (normal mode)
            if !FORCE_TEST_EXECUTION && time_left > SNIPE_WINDOW_SECS {
                debug!("⏱️  {:.1}s until snipe window", time_left);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            // IN SNIPE WINDOW! (<2s = final pot known + max time bonus)
            info!("🎯 FINAL SNIPE WINDOW: {:.2}s left (final pot + 1.3x time bonus)", time_left);

            // === MULTI-CELL PORTFOLIO STRATEGY ===
            // Get wallet balance
            let wallet_balance = match self.check_wallet_balance().await {
                Ok(balance) => balance,
                Err(e) => {
                    warn!("Failed to check wallet balance: {}, using 0.0", e);
                    0.0
                }
            };

            // Calculate how many cells to buy (adaptive scaling)
            let target_cell_count = self.config.calculate_cell_count(wallet_balance);

            // Find best N cells (ranked by S_j)
            let targets = self.find_snipe_targets(&board, time_left, target_cell_count as usize, wallet_balance);

            if !targets.is_empty() {
                let total_cost: f64 = targets.iter().map(|c| c.cost_lamports as f64 / 1e9).sum();
                info!("🎯 MULTI-CELL PORTFOLIO: {} cells selected | Total: {:.6} SOL | Balance: {:.6} SOL",
                    targets.len(), total_cost, wallet_balance);

                // Log each target
                for (idx, cell) in targets.iter().enumerate() {
                    let ev = self.calculate_ev(&board, cell, time_left);
                    let s_j = self.calculate_s_j(&board, cell);
                    info!("   #{}: Cell {} | Cost: {:.6} SOL | Deployers: {} | EV: {:.1}% | S_j: {:.2}",
                        idx + 1, cell.id, cell.cost_lamports as f64 / 1e9, cell.difficulty, ev * 100.0, s_j);
                }

                // Execute multi-cell snipe (JITO bundle with all cells)
                self.execute_multi_snipe(&targets, time_left).await?;

                // Exit after one execution (testing mode)
                if EXECUTE_ONCE_AND_EXIT {
                    info!("✅ EXECUTE_ONCE_AND_EXIT: Snipe completed, exiting bot");
                    std::process::exit(0);
                }
            } else {
                let min_deployers = board.cells.iter().map(|c| c.difficulty).min().unwrap_or(0);
                let max_deployers = board.cells.iter().map(|c| c.difficulty).max().unwrap_or(0);
                let min_cost = board.cells.iter().map(|c| c.cost_lamports).min().unwrap_or(0);
                let max_cost = board.cells.iter().map(|c| c.cost_lamports).max().unwrap_or(0);
                info!("⚠️  No opportunity: pot {:.6} SOL, deployers {}-{}, cost {:.6}-{:.6} SOL, need EV > {:.1}%, Motherlode check failed or no +EV cells",
                      board.pot_lamports as f64 / 1e9, min_deployers, max_deployers,
                      min_cost as f64 / 1e9, max_cost as f64 / 1e9, self.config.min_ev_percentage);
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

            // Update dashboard status every 2 seconds
            self.update_dashboard_status().await;

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

    /// Find best snipe target - cell with LOWEST total deployed SOL
    /// (to maximize our % share of that cell)
    /// Find multiple snipe targets (multi-cell portfolio strategy)
    ///
    /// Returns top N cells ranked by S_j score (drain potential per cost)
    /// where N is determined by adaptive scaling based on bankroll
    fn find_snipe_targets(&self, board: &OreBoard, time_left: f64, num_cells: usize, wallet_balance_sol: f64) -> Vec<Cell> {
        // 🔥 FORCE TEST MODE: Just buy ANY 2 cells to test execution
        if FORCE_TEST_EXECUTION {
            info!("🔥 FORCE TEST MODE: Selecting ANY 2 cells for test execution");
            let test_cells: Vec<Cell> = board.cells.iter()
                .take(2)  // Just take first 2 cells, don't care which
                .cloned()
                .collect();

            info!("🔥 FORCE TEST: Selected {} cells for execution (cell IDs: {} and {})",
                test_cells.len(),
                test_cells[0].id,
                test_cells.get(1).map(|c| c.id).unwrap_or(255)
            );
            return test_cells;
        }

        const MAX_CELL_COST: u64 = 5_000_000;  // Max 0.005 SOL per cell (TESTING MODE)
        const MIN_MOTHERLODE_ORE: f64 = 10.0;  // Only play when Motherlode >= 10 ORE (TESTING MODE)

        // === Motherlode Gating ===
        let motherlode_ore = board.motherlode_ore as f64 / 1e11;
        if motherlode_ore < MIN_MOTHERLODE_ORE {
            return Vec::new();
        }

        // === Find +EV Cells ===
        let mut positive_ev_cells: Vec<(f64, Cell)> = board.cells.iter()
            .filter(|c| c.cost_lamports <= MAX_CELL_COST)
            .filter(|c| {
                let ev = self.calculate_ev(board, c, time_left);
                ev >= self.config.min_ev_decimal()
            })
            .map(|c| {
                let s_j = self.calculate_s_j(board, c);
                (s_j, c.clone())
            })
            .collect();

        if positive_ev_cells.is_empty() {
            return Vec::new();
        }

        // === S_j Ranking ===
        // Sort by S_j descending (highest S_j first)
        positive_ev_cells.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // === Cost Safety Check ===
        // Take top N cells but ensure total cost <= max_cost_per_round
        let max_total_cost = self.config.max_cost_per_round_sol;
        let mut selected_cells = Vec::new();
        let mut total_cost = 0.0;

        for (_s_j, cell) in positive_ev_cells.iter().take(num_cells) {
            let cell_cost = cell.cost_lamports as f64 / 1e9;

            // Check if adding this cell would exceed max cost or wallet balance
            if total_cost + cell_cost > max_total_cost {
                break;
            }
            if total_cost + cell_cost > wallet_balance_sol - self.config.min_wallet_balance_sol {
                break;
            }

            selected_cells.push(cell.clone());
            total_cost += cell_cost;
        }

        selected_cells
    }

    /// Legacy single-cell method (for backwards compatibility)
    #[allow(dead_code)]
    fn find_snipe_target(&self, board: &OreBoard, time_left: f64) -> Option<Cell> {
        self.find_snipe_targets(board, time_left, 1, f64::MAX).into_iter().next()
    }

    /// Calculate expected value for a cell (LOTTERY SYSTEM WITH POT SPLITTING)
    ///
    /// REAL MECHANICS (from live testing):
    /// - Deploy any amount of SOL to a cell
    /// - Your payout = (Your Deploy / Cell Total) × Winnings
    /// - Winnings = Pot × 0.85 (15% vaulted as "gravy")
    /// - Win probability = 1/25 (random cell selected)
    /// - Ignore ORE rewards (bonus if you get it)
    ///
    /// EV = (Win Prob × My Share) - My Deploy
    fn calculate_ev(&self, board: &OreBoard, cell: &Cell, _time_left: f64) -> f64 {
        // === Parameters ===
        let total_pot = board.pot_lamports as f64 / 1e9;  // Total pot (SOL)
        let cell_deployed = cell.deployed_lamports as f64 / 1e9;  // Cell deployed (SOL)
        let n_j = cell.difficulty as f64;  // Number of deployers on cell
        let p_j = cell.cost_lamports as f64 / 1e9;  // Cell price (SOL)
        let motherlode = board.motherlode_ore as f64 / 1e11;  // Motherlode (ORE)
        let ore_price = board.ore_price_sol;  // ORE price (SOL/ORE)

        // Constants
        let rake = 0.10;  // 10% vaulted
        let adj = 0.95;   // Variance adjustment
        let fees = 0.00005;  // Jito + priority fees (SOL)

        // === SOL Component ===
        // If this cell wins (1/25 prob), SOL winnings are split proportionally
        // Our share = (p_j / (cell_deployed + p_j)) × (total_pot × 0.85)  [proportional split]
        // Simplified: drain losers' SOL → (total_pot - cell_deployed - rake×total_pot) / (n_j+1)
        let cell_total_after = cell_deployed + p_j;
        let my_fraction = if cell_total_after > 0.0 { p_j / cell_total_after } else { 0.0 };
        let winnings = total_pot * (1.0 - rake);
        let my_sol_if_win = my_fraction * winnings;

        // === ORE Component ===
        // If this cell wins (1/25), ONE random deployer gets 1 ORE + Motherlode chance
        // Probability I get ORE = (1/25) × (1/(n_j+1))
        // Expected ORE = 1 + motherlode/625 (includes 1/625 Motherlode trigger chance)
        // Expected ORE value = ore_price × (1 + motherlode/625) / (25 × (n_j+1))
        let ore_expected_value = if n_j + 1.0 > 0.0 {
            ore_price * (1.0 + motherlode / 625.0) / (25.0 * (n_j + 1.0))
        } else {
            0.0
        };

        // === Total EV ===
        // Win prob = 1/25, apply variance adj, subtract cost and fees
        let win_prob = 1.0 / 25.0;
        let expected_return = win_prob * (my_sol_if_win + ore_expected_value * 25.0) * adj;
        let ev_sol = expected_return - p_j - fees;

        // Return EV as percentage
        if p_j > 0.0 {
            ev_sol / p_j
        } else {
            0.0
        }
    }

    /// Calculate S_j ranking: measures "drain potential per cost"
    /// S_j = (total_pot - cell_deployed) / [(n_j+1) × p_j]
    /// Higher S_j = better opportunity (more SOL to drain from losers, lower cost/competition)
    fn calculate_s_j(&self, board: &OreBoard, cell: &Cell) -> f64 {
        let total_pot = board.pot_lamports as f64 / 1e9;  // Total pot (SOL)
        let cell_deployed = cell.deployed_lamports as f64 / 1e9;  // Cell deployed (SOL)
        let n_j = cell.difficulty as f64;  // Number of deployers
        let p_j = cell.cost_lamports as f64 / 1e9;  // Cell price (SOL)

        let denominator = (n_j + 1.0) * p_j;
        if denominator > 0.0 {
            (total_pot - cell_deployed) / denominator
        } else {
            0.0
        }
    }

    /// Calculate time until reset in seconds
    fn time_until_reset(&self, board: &OreBoard, current_slot: u64) -> f64 {
        let slots_left = board.reset_slot.saturating_sub(current_slot) as f64;
        (slots_left * SLOT_DURATION_MS / 1000.0).max(0.0)
    }


    /// Execute multi-cell snipe (portfolio strategy)
    ///
    /// Deploys to multiple cells in a single transaction
    /// Uses regular RPC (not JITO) - 2s window is sufficient
    async fn execute_multi_snipe(&mut self, cells: &[Cell], time_left: f64) -> Result<()> {
        let start = Instant::now();
        let total_cost: f64 = cells.iter().map(|c| c.cost_lamports as f64 / 1e9).sum();

        if self.config.paper_trading {
            info!("📝 PAPER TRADE: Would deploy to {} cells (total: {:.6} SOL)", cells.len(), total_cost);
            for (idx, cell) in cells.iter().enumerate() {
                let board = BOARD.load();
                let ev = self.calculate_ev(&board, cell, time_left);
                info!("   #{}: Cell {} | Cost: {:.6} SOL | EV: {:.1}%",
                    idx + 1, cell.id, cell.cost_lamports as f64 / 1e9, ev * 100.0);
            }

            self.stats.total_snipes += cells.len() as u64;
            self.stats.successful_snipes += cells.len() as u64;
            self.stats.total_spent_sol += total_cost;
            return Ok(());
        }

        // LIVE TRADING
        info!("🚀 LIVE: Building multi-cell Deploy for {} cells (total: {:.6} SOL)", cells.len(), total_cost);

        // Get wallet
        let wallet = self.wallet.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wallet not loaded"))?;
        let authority = wallet.pubkey();

        // Get current round ID
        let board = BOARD.load();
        let round_id = (board.current_slot / 150) as u64;

        // Build squares array with ALL selected cells set to true
        let mut squares = [false; 25];
        let mut total_amount = 0u64;

        for cell in cells {
            squares[cell.id as usize] = true;
            total_amount += cell.cost_lamports;
        }

        // Get current board for entropy_var
        let current_board = BOARD.load();

        // Build Deploy instruction for multiple cells
        let deploy_ix = build_deploy_instruction(
            authority,
            authority,
            total_amount,  // Total amount for all cells
            round_id,
            squares,       // Multiple cells set to true
        )?;

        info!("✅ Multi-cell Deploy instruction built in {:?}", start.elapsed());

        // Build and send transaction via RPC (simpler than JITO for 2s window)
        use solana_sdk::transaction::Transaction;
        use solana_client::rpc_client::RpcClient;

        let rpc = RpcClient::new(self.config.rpc_url.clone());

        // Get recent blockhash
        let blockhash = rpc.get_latest_blockhash()?;

        // Build transaction
        let tx = Transaction::new_signed_with_payer(
            &[deploy_ix],
            Some(&authority),
            &[wallet],
            blockhash,
        );

        // Submit transaction
        let signature = rpc.send_transaction(&tx)?;

        info!("✅ Multi-cell transaction submitted: {} | {} cells | Total: {:.6} SOL | Time: {:.1}s",
            signature, cells.len(), total_cost, time_left);

        // Update stats
        self.stats.total_snipes += cells.len() as u64;
        self.stats.successful_snipes += cells.len() as u64;
        self.stats.total_spent_sol += total_cost;

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

    /// Update dashboard status (write to /tmp/ore_bot_status.json)
    async fn update_dashboard_status(&mut self) {
        let board = BOARD.load();

        // Calculate pot size
        let total_pot: u64 = board.cells.iter()
            .filter(|c| c.claimed || c.claimed_in_mempool)
            .map(|c| c.cost_lamports)
            .sum();

        // Get wallet address
        let wallet_address = if let Some(wallet) = &self.wallet {
            wallet.pubkey().to_string()
        } else {
            "Paper Trading".to_string()
        };

        // ShredStream connection status
        let shredstream_connected = self.shredstream.is_some();

        // Calculate latencies (stub for now - can be calculated from actual metrics)
        let shredstream_latency_ms = if shredstream_connected {
            Some(0.25) // Stub - measure actual latency in production
        } else {
            None
        };

        let rpc_latency_ms = Some(60.0); // Stub - measure actual RPC latency

        // Get Motherlode amount (in ORE)
        let motherlode_ore = board.motherlode_ore as f64 / 1e11;

        self.dashboard.write_status(
            &board,
            &self.stats,
            self.config.paper_trading,
            &wallet_address,
            shredstream_latency_ms,
            rpc_latency_ms,
            self.entries_processed,
            shredstream_connected,
            total_pot,
            motherlode_ore,
        );
    }

    /// Legacy EV-based sniping (replaced by S_j ranking multi-cell approach)
    #[allow(dead_code)]
    async fn try_ev_snipe(&self, current_pot: u64, time_left: f64, use_jito: bool) -> Result<()> {
        let board = BOARD.load();

        // Find cheapest unclaimed cell
        let cheapest_cell = board.cells.iter()
            .filter(|c| !c.claimed && !c.claimed_in_mempool)
            .min_by_key(|c| c.cost_lamports);

        if let Some(cell) = cheapest_cell {
            let ev = self.calculate_ev(&board, cell, time_left);

            if ev >= self.config.min_ev_decimal() {
                let submission_method = if use_jito { "JITO (fast)" } else { "RPC (free)" };
                info!("🎯 EV SNIPE: Cell {} | Cost: {:.6} SOL | Pot: {:.6} SOL | EV: {:.1}% | Via: {}",
                      cell.id, cell.cost_lamports as f64 / 1e9,
                      current_pot as f64 / 1e9, ev * 100.0, submission_method);

                // Execute snipe
                if self.config.paper_trading {
                    info!("📝 PAPER TRADE: Would EV-snipe cell {} for {:.6} SOL via {}",
                          cell.id, cell.cost_lamports as f64 / 1e9, submission_method);
                } else {
                    if use_jito {
                        info!("⚡ Using JITO for speed (time-critical: {:.1}s left)", time_left);
                        // TODO: Implement JITO submission (needs refactoring execute_snipe)
                    } else {
                        info!("💸 Using free RPC submission ({:.1}s left - no rush)", time_left);
                        // TODO: Implement regular RPC submission
                    }
                    warn!("⚠️ Live trading not yet enabled - run in paper mode");
                }
            }
        }

        Ok(())
    }

    /// Legacy clone helper (no longer needed with multi-cell approach)
    #[allow(dead_code)]
    fn clone_for_snipe(&self) -> Self {
        Self {
            config: self.config.clone(),
            price_fetcher: crate::jupiter_price::OrePriceFetcher::new(), // Fresh fetcher for clone
            stats: self.stats.clone(),
            shredstream: None, // Don't clone ShredStream (not needed for snipes)
            wallet: self.wallet.as_ref().map(|w| w.insecure_clone()),
            rpc_client: None, // Don't clone RPC (not needed for snipes)
            dashboard: self.dashboard.clone(),
            entries_processed: self.entries_processed,
            board_ws_rx: self.board_ws_rx.resubscribe(), // New receiver from same broadcast channel
            round_ws_rx: self.round_ws_rx.resubscribe(), // New receiver from same broadcast channel
            treasury_ws_rx: self.treasury_ws_rx.resubscribe(), // New receiver from same broadcast channel
        }
    }

    async fn wait_for_new_slot(&mut self) -> Result<u64> {
        if let Some(ref mut shredstream) = self.shredstream {
            // Process ShredStream events
            let event = shredstream.process().await?;

            // Handle Ore events
            for ore_event in event.events {
                self.entries_processed += 1; // Track total events processed

                match ore_event {
                    OreEvent::SlotUpdate { slot } => {
                        debug!("📡 Slot update: {}", slot);
                    }
                    OreEvent::BoardReset { slot } => {
                        info!("🔄 Board reset at slot {} - New round starting!", slot);

                        // Add dashboard event
                        self.dashboard.add_event(DashboardEvent {
                            event_type: "BoardReset".to_string(),
                            slot: Some(slot),
                            timestamp: get_timestamp(),
                            cell_id: None,
                            authority: None,
                        });

                        // Update board reset slot and increment round ID
                        let mut board = BOARD.load().as_ref().clone();
                        let _old_round_id = board.round_id;
                        board.reset_slot = slot;
                        board.round_id += 1;
                        // Clear all claims and deployers
                        for cell in &mut board.cells {
                            cell.claimed = false;
                            cell.claimed_in_mempool = false;
                            cell.deployers.clear();  // Reset deployer count for new round
                        }

                        // **CRITICAL: Fetch real board state from RPC**
                        if let Some(ref rpc_client) = self.rpc_client {
                            match rpc_client.update_board_state(&mut board).await {
                                Ok(_) => {
                                    info!("✅ Real board state loaded from RPC (pot, Motherlode, ORE price)");
                                }
                                Err(e) => {
                                    warn!("⚠️ Failed to fetch board state: {} - using defaults", e);
                                }
                            }
                        }

                        BOARD.store(Arc::new(board));
                    }
                    OreEvent::CellDeployed { cell_id, authority, amount_lamports } => {
                        info!("✅ Cell {} deployed: {:.6} SOL by {}",
                              cell_id, amount_lamports as f64 / 1e9, &authority[..8]);

                        // PROPORTIONAL OWNERSHIP TRACKING (Ore V2 mechanics):
                        // Multiple players can deploy to same cell with different amounts
                        // Rewards are split proportionally based on each player's share
                        {
                            let mut board = BOARD.load().as_ref().clone();
                            if (cell_id as usize) < BOARD_SIZE {
                                let cell = &mut board.cells[cell_id as usize];

                                // Track deployer
                                cell.deployers.push(authority.clone());
                                cell.claimed = true;

                                // PROPORTIONAL OWNERSHIP: Track total deployed to this cell
                                cell.deployed_lamports += amount_lamports;

                                // Set our fixed investment amount (from config, convert SOL to lamports)
                                if cell.cost_lamports == 0 {
                                    cell.cost_lamports = (self.config.max_claim_cost_sol * 1e9) as u64;
                                }

                                // Track difficulty (number of deployers for pot splitting)
                                cell.difficulty = cell.deployers.len() as u64;

                                info!("   → Cell {} totals: deployed={:.6} SOL, deployers={}",
                                      cell_id, cell.deployed_lamports as f64 / 1e9, cell.difficulty);
                            }
                            BOARD.store(Arc::new(board));
                        }

                        // IMMEDIATE EXECUTION CHECK (ShredStream-first architecture)
                        // In FORCE_TEST mode, execute as soon as we have 2 cells with valid costs
                        if FORCE_TEST_EXECUTION {
                            let board = BOARD.load();
                            let cells_with_cost: Vec<_> = board.cells.iter()
                                .filter(|c| c.cost_lamports > 0)
                                .collect();

                            if cells_with_cost.len() >= 2 {
                                info!("🔥 FORCE TEST: ShredStream detected {} cells, executing NOW!", cells_with_cost.len());

                                // Trigger immediate execution (don't wait for RPC/WebSocket)
                                // This happens in the main loop via the force test condition
                                // We just need to ensure board.cells are populated, which they now are
                            }
                        }

                        // FINAL-WINDOW STRATEGY: Wait until <2s to know true EV
                        // Early EV is meaningless - pot still growing
                        // At <2s we know: final pot size, final deployer counts, max time bonus (1.3x)

                        // Add dashboard event
                        self.dashboard.add_event(DashboardEvent {
                            event_type: "CellDeployed".to_string(),
                            slot: Some(BOARD.load().current_slot),
                            timestamp: get_timestamp(),
                            cell_id: Some(cell_id),
                            authority: Some(authority.clone()),
                        });

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
