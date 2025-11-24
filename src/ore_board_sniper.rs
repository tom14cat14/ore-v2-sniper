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
use tracing::{debug, info, warn};

use crate::config::OreConfig;
use crate::dashboard::{get_timestamp, DashboardEvent, DashboardWriter};
use crate::ore_instructions::build_deploy_instruction;
use crate::ore_shredstream::{OreEvent, OreShredStreamProcessor};
use solana_sdk::signature::{Keypair, Signer};

// Ore V2 constants
const BOARD_SIZE: usize = 25; // 25-cell board
                              // SNIPE_WINDOW now configured via config.snipe_window_seconds (from SNIPE_WINDOW_SECONDS env var)
const EPOCH_DURATION_SECS: u64 = 60; // Board resets every 60 seconds
#[allow(dead_code)]
const MAX_COMPETITORS: usize = 3; // Max competitors to track
#[allow(dead_code)]
const BASE_TIP: u64 = 10_000; // Base Jito tip in lamports
const SLOT_DURATION_MS: f64 = 400.0; // Average Solana slot time

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
    pub round_id: u64,       // Current round ID for claiming rewards
    pub pot_lamports: u64,   // Real pot size from Round account (total_deployed)
    pub motherlode_ore: u64, // Motherlode jackpot in raw units (divide by 1e11 for ORE - ORE has 11 decimals!)
    pub ore_price_sol: f64,  // ORE price in SOL (from Jupiter)
    pub entropy_var: solana_sdk::pubkey::Pubkey, // Entropy VAR address for Deploy instruction
}

/// Individual cell on the Ore board
#[derive(Clone, Default, Debug)]
pub struct Cell {
    pub id: u8,
    pub cost_lamports: u64, // DEPRECATED - You SET deployment amount via config, NOT read from board
    pub deployed_lamports: u64, // Total SOL deployed to this cell (from Round account)
    pub difficulty: u64,    // Number of deployers on this cell
    pub claimed: bool,      // Claimed on-chain
    pub claimed_in_mempool: bool, // Claimed in mempool (avoid)
    pub deployers: Vec<String>, // Track all deployers (for pot splitting calculation)
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
    // Track deployments for win/loss tracking
    last_round_deployed: Option<u64>,
    last_round_cells: Vec<u8>,
    last_round_amount: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SnipeStats {
    // Round-level metrics
    pub rounds_played: u64,       // Total rounds participated in
    pub rounds_won: u64,          // Rounds where we won more than we spent
    pub rounds_lost: u64,         // Rounds where we lost or broke even

    // Pick-level metrics (can make multiple picks per round)
    pub picks_made: u64,          // Total cells deployed to across all rounds
    pub picks_won: u64,           // Total cells that were winning cells

    // Financial metrics
    pub total_spent_sol: f64,     // Total SOL spent on deployments
    pub total_earned_sol: f64,    // Total SOL won from claims
    pub total_tips_paid: f64,     // Total Jito tips paid
    pub total_claims: u64,        // Number of successful claims
    pub starting_balance: f64,    // Starting wallet balance
    pub last_balance_check: f64,  // Last known wallet balance
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
                    warn!(
                        "⚠️ Failed to fetch initial board state: {} - using round 0",
                        e
                    );
                    0
                }
            }
        } else {
            0
        };

        // Spawn Board WebSocket subscriber for real-time Board updates
        let board_ws_rx =
            crate::ore_board_websocket::spawn_board_subscriber(config.ws_url.clone())?;
        info!("📡 Board WebSocket subscriber spawned");

        // Spawn Round WebSocket subscriber for real-time Round updates
        let round_ws_rx = crate::ore_board_websocket::spawn_round_subscriber(
            config.ws_url.clone(),
            initial_round_id,
        )?;
        info!(
            "📡 Round WebSocket subscriber spawned (round {})",
            initial_round_id
        );

        // Spawn Treasury WebSocket subscriber for real-time Motherlode updates
        let treasury_ws_rx =
            crate::ore_board_websocket::spawn_treasury_subscriber(config.ws_url.clone())?;
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
            last_round_deployed: None,
            last_round_cells: Vec::new(),
            last_round_amount: 0.0,
        })
    }

    /// Main sniping loop - called from ShredStream slot updates
    pub async fn run(&mut self) -> Result<()> {
        info!("🎯 Ore Board Sniper started");
        info!(
            "⚙️  Mode: {}",
            if self.config.paper_trading {
                "📝 PAPER TRADING"
            } else {
                "💰 LIVE TRADING"
            }
        );
        info!("💎 Min EV: {:.1}%", self.config.min_ev_percentage);
        info!("📊 Board: 25 cells, resets every {}s", EPOCH_DURATION_SECS);

        // Check starting wallet balance
        if let Ok(balance) = self.check_wallet_balance().await {
            self.stats.starting_balance = balance;
            self.stats.last_balance_check = balance;
            info!("💰 Starting wallet balance: {:.6} SOL", balance);
        }

        // Fetch initial Treasury state (Motherlode)
        if let Some(ref rpc) = self.rpc_client {
            match rpc.fetch_treasury().await {
                Ok(treasury) => {
                    let mut board = BOARD.load().as_ref().clone();
                    board.motherlode_ore = treasury.motherlode;
                    BOARD.store(Arc::new(board));
                    info!(
                        "💎 Initial Motherlode: {:.2} ORE",
                        treasury.motherlode as f64 / 1e11
                    );
                }
                Err(e) => {
                    warn!("⚠️  Failed to fetch initial Treasury state: {}", e);
                }
            }
        }

        // Start blockhash refresh task
        self.start_blockhash_refresh();

        let mut last_slot = 0u64;
        let mut last_pnl_log = std::time::Instant::now();
        let mut last_rpc_refresh = std::time::Instant::now();
        let mut last_executed_round: Option<u64> = None; // Track which round we executed

        loop {
            // Periodically refresh board state via RPC (every 5 seconds)
            // This ensures we have valid data even if WebSocket returns dummy values
            if last_rpc_refresh.elapsed().as_secs() >= 5 {
                if let Some(ref rpc) = self.rpc_client {
                    let mut board = BOARD.load().as_ref().clone();
                    match rpc.update_board_state(&mut board).await {
                        Ok(()) => {
                            BOARD.store(Arc::new(board));
                            debug!(
                                "✅ RPC board refresh: round {}, pot={:.6} SOL",
                                BOARD.load().round_id,
                                BOARD.load().pot_lamports as f64 / 1e9
                            );
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
                    debug!(
                        "📡 Board WebSocket update: round {}, reset_slot {}",
                        board_update.round_id, board_update.end_slot
                    );

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
                        info!("🔄 ROUND CHANGE DETECTED! {} → {}, re-subscribing to Round WebSocket (reset_slot: {})",
                              old_round_id, board_update.round_id, board_update.end_slot);

                        // Spawn new Round subscriber for new round
                        match crate::ore_board_websocket::spawn_round_subscriber(
                            self.config.ws_url.clone(),
                            board_update.round_id,
                        ) {
                            Ok(new_rx) => {
                                self.round_ws_rx = new_rx;
                                info!(
                                    "✅ Round WebSocket re-subscribed to round {}",
                                    board_update.round_id
                                );
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
                    debug!(
                        "📊 Round WebSocket update: pot={:.6} SOL, {}/25 cells claimed",
                        round_update.total_deployed as f64 / 1e9,
                        round_update.deployed.iter().filter(|&&x| x > 0).count()
                    );

                    // Update board with Round data
                    let mut board = BOARD.load().as_ref().clone();
                    board.pot_lamports = round_update.total_deployed;

                    // Update cell deployment status
                    for (i, cell) in board.cells.iter_mut().enumerate() {
                        cell.deployed_lamports = round_update.deployed[i];
                        cell.difficulty = round_update.count[i];
                        cell.claimed = round_update.deployed[i] > 0;

                        // Set our fixed investment amount (from config)
                        // This is what WE will deploy, not the cost to claim
                        if cell.cost_lamports == 0 {
                            cell.cost_lamports = (self.config.max_claim_cost_sol * 1e9) as u64;
                        }
                    }

                    BOARD.store(Arc::new(board));

                    // Update dashboard instantly when Round changes (pot size, cell deployments)
                    self.update_dashboard_status().await;
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
                    debug!(
                        "💎 Treasury WebSocket update: Motherlode={:.2} ORE",
                        treasury_update.motherlode_balance as f64 / 1e11
                    );

                    // Update board with Treasury data
                    let mut board = BOARD.load().as_ref().clone();
                    board.motherlode_ore = treasury_update.motherlode_balance;
                    BOARD.store(Arc::new(board));

                    // Update dashboard instantly when Motherlode changes
                    self.update_dashboard_status().await;
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
            static LAST_TIMING_LOG: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let last_log = LAST_TIMING_LOG.load(std::sync::atomic::Ordering::Relaxed);
            if now_secs - last_log >= 30 {
                let available = board
                    .cells
                    .iter()
                    .filter(|c| !c.claimed && !c.claimed_in_mempool)
                    .count();
                info!(
                    "⏱️  {:.1}s until snipe window | {} cells free | pot: {:.6} SOL",
                    time_left,
                    available,
                    board.pot_lamports as f64 / 1e9
                );
                LAST_TIMING_LOG.store(now_secs, std::sync::atomic::Ordering::Relaxed);
            }

            // === FORCE TEST MODE: ShredStream-first execution ===
            if self.config.force_test_mode {
                // SHREDSTREAM-FIRST: Execute as soon as we have 2+ cells with costs
                // Don't wait for WebSocket/RPC round_id or entropy_var
                // This achieves <1ms execution latency (the whole point of ShredStream!)
                let cells_with_cost = board.cells.iter().filter(|c| c.cost_lamports > 0).count();

                if cells_with_cost < 2 {
                    debug!(
                        "🔥 FORCE TEST: Waiting for ShredStream to detect cells ({}/2 cells ready)",
                        cells_with_cost
                    );
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }

                info!(
                    "🔥 FORCE TEST MODE: ShredStream detected {} cells - executing NOW!",
                    cells_with_cost
                );
                info!(
                    "   (Bypassing round_id/entropy_var checks - ShredStream-first architecture)"
                );
                info!("   Cells with cost: {}/25", cells_with_cost);
            }

            // Only act in snipe window (normal mode)
            let snipe_window_secs = self.config.snipe_window_seconds as f64;
            if !self.config.force_test_mode && time_left > snipe_window_secs {
                // Event-driven: just continue to next ShredStream event, no polling sleep
                continue;
            }

            // Check if already executed this round
            if last_executed_round == Some(board.round_id) {
                // Already executed this round, wait for next round
                continue;
            }

            // IN SNIPE WINDOW! (using configured snipe_window_seconds)
            info!(
                "🎯 FINAL SNIPE WINDOW: {:.2}s left (configured: {:.1}s)",
                time_left, snipe_window_secs
            );

            // === CRITICAL: Refresh board state via RPC to get FRESH data ===
            // WebSocket updates are lagged - need RPC call to see real deployer counts (~400/cell at end)
            if let Some(ref rpc) = self.rpc_client {
                let mut board = BOARD.load().as_ref().clone();
                match rpc.update_board_state(&mut board).await {
                    Ok(()) => {
                        BOARD.store(Arc::new(board));
                        info!("✅ Fresh RPC data loaded before execution");
                    }
                    Err(e) => {
                        warn!(
                            "⚠️  Failed to refresh board via RPC: {}, using stale data",
                            e
                        );
                    }
                }
            }

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
            let targets = self.find_snipe_targets(
                &board,
                time_left,
                target_cell_count as usize,
                wallet_balance,
            );

            if !targets.is_empty() {
                let total_cost: f64 = targets.len() as f64 * self.config.deployment_per_cell_sol; // OUR deployment - NEW!
                info!("🎯 MULTI-CELL PORTFOLIO: {} cells selected | Total: {:.6} SOL | Balance: {:.6} SOL",
                    targets.len(), total_cost, wallet_balance);

                // Log each target
                for (idx, cell) in targets.iter().enumerate() {
                    let ev = self.calculate_ev(&board, cell, time_left);
                    let s_j = self.calculate_s_j(&board, cell);
                    info!("   #{}: Cell {} | Deployed: {:.6} SOL | Deployers: {} | EV: {:.1}% | S_j: {:.2}",
                        idx + 1, cell.id, cell.deployed_lamports as f64 / 1e9, cell.difficulty, ev * 100.0, s_j);
                }

                // Execute multi-cell snipe (JITO bundle with all cells)
                self.execute_multi_snipe(&targets, time_left).await?;

                // Mark this round as executed to prevent re-execution
                last_executed_round = Some(board.round_id);
                info!(
                    "✅ Round {} executed, waiting for next round",
                    board.round_id
                );

                // Exit after one execution (testing mode)
                if self.config.execute_once_and_exit {
                    info!("✅ EXECUTE_ONCE_AND_EXIT: Snipe completed, exiting bot");
                    std::process::exit(0);
                }
            } else {
                let min_deployers = board.cells.iter().map(|c| c.difficulty).min().unwrap_or(0);
                let max_deployers = board.cells.iter().map(|c| c.difficulty).max().unwrap_or(0);
                let min_deployed = board
                    .cells
                    .iter()
                    .map(|c| c.deployed_lamports)
                    .min()
                    .unwrap_or(0);
                let max_deployed = board
                    .cells
                    .iter()
                    .map(|c| c.deployed_lamports)
                    .max()
                    .unwrap_or(0);
                info!("⚠️  No opportunity: pot {:.6} SOL, cells have {}-{} deployers with {:.6}-{:.6} SOL already deployed, we deploy {:.6} SOL, need EV > {:.1}%",
                      board.pot_lamports as f64 / 1e9, min_deployers, max_deployers,
                      min_deployed as f64 / 1e9, max_deployed as f64 / 1e9,
                      self.config.deployment_per_cell_sol, self.config.min_ev_percentage);
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

            // CRITICAL: If round has ended (time_left <= 0), wait for next round
            // instead of continuously re-evaluating the expired round
            if time_left <= 0.0 {
                let old_round_id = BOARD.load().round_id;
                let old_reset_slot = BOARD.load().reset_slot;
                let old_pot = BOARD.load().pot_lamports;
                info!("⏳ Round ended, waiting for next round... (current: round_id={}, reset_slot={}, pot={:.6} SOL)",
                      old_round_id, old_reset_slot, old_pot as f64 / 1e9);

                // Wait for reset_slot to change (indicating new round started)
                let mut attempts = 0;
                while BOARD.load().reset_slot == old_reset_slot && attempts < 300 {
                    // Max 30 seconds
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    attempts += 1;

                    // Log progress every 5 seconds (50 attempts)
                    if attempts % 50 == 0 {
                        let current_pot = BOARD.load().pot_lamports;
                        info!(
                            "   ⏳ Still waiting... {}s elapsed (pot now: {:.6} SOL)",
                            attempts / 10,
                            current_pot as f64 / 1e9
                        );
                    }
                }

                let new_board = BOARD.load();
                if new_board.reset_slot != old_reset_slot {
                    info!("✅ NEW ROUND STARTED! round_id: {} → {}, reset_slot: {} → {}, pot: {:.6} → {:.6} SOL",
                          old_round_id, new_board.round_id,
                          old_reset_slot, new_board.reset_slot,
                          old_pot as f64 / 1e9, new_board.pot_lamports as f64 / 1e9);

                    // Resolve previous round outcome (win/loss tracking)
                    self.resolve_previous_round(old_round_id, &new_board);
                } else {
                    warn!("⚠️  Timeout waiting for new round (30s), continuing anyway (pot: {:.6} SOL)",
                          new_board.pot_lamports as f64 / 1e9);
                }
                continue;
            }

            // Minimal sleep to prevent tight loop (1ms for speed optimization)
            tokio::time::sleep(Duration::from_millis(1)).await;
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
    fn find_snipe_targets(
        &self,
        board: &OreBoard,
        time_left: f64,
        num_cells: usize,
        wallet_balance_sol: f64,
    ) -> Vec<Cell> {
        info!(
            "🔍 find_snipe_targets called: num_cells={}, wallet={:.6} SOL, time_left={:.2}s",
            num_cells, wallet_balance_sol, time_left
        );

        // 🔥 FORCE TEST MODE: Just buy ANY 2 cells to test execution
        if self.config.force_test_mode {
            info!("🔥 FORCE TEST MODE: Selecting ANY 2 cells for test execution");
            let test_cells: Vec<Cell> = board
                .cells
                .iter()
                .take(2) // Just take first 2 cells, don't care which
                .cloned()
                .collect();

            info!(
                "🔥 FORCE TEST: Selected {} cells for execution (cell IDs: {} and {})",
                test_cells.len(),
                test_cells[0].id,
                test_cells.get(1).map(|c| c.id).unwrap_or(255)
            );
            return test_cells;
        }

        const MIN_MOTHERLODE_ORE: f64 = 0.0; // No minimum - cover all cells

        // === Motherlode Gating ===
        let motherlode_ore = board.motherlode_ore as f64 / 1e11; // ORE has 11 decimals!
        info!(
            "💎 Motherlode check: {:.2} ORE (need >= {:.1} ORE)",
            motherlode_ore, MIN_MOTHERLODE_ORE
        );
        if motherlode_ore < MIN_MOTHERLODE_ORE {
            info!(
                "⚠️  Motherlode too low: {:.2} ORE < {:.1} ORE minimum",
                motherlode_ore, MIN_MOTHERLODE_ORE
            );
            return Vec::new();
        }

        // === Find +EV Cells ===
        // NOTE: We evaluate ALL cells, not filtering by existing deployments!
        // We deploy OUR configured amount (deployment_per_cell_sol) into cells
        let mut positive_ev_cells: Vec<(f64, Cell)> = board
            .cells
            .iter()
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
        positive_ev_cells
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // === Cost Safety Check ===
        // Take top N cells but ensure total cost <= max_cost_per_round
        let max_total_cost = self.config.max_cost_per_round_sol;
        let mut selected_cells = Vec::new();
        let mut total_cost = 0.0;

        for (_s_j, cell) in positive_ev_cells.iter().take(num_cells) {
            let cell_cost = self.config.deployment_per_cell_sol; // OUR deployment amount - NEW!

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
        self.find_snipe_targets(board, time_left, 1, f64::MAX)
            .into_iter()
            .next()
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
        let total_pot = board.pot_lamports as f64 / 1e9; // Total pot (SOL)
        let cell_deployed = cell.deployed_lamports as f64 / 1e9; // Cell already deployed (SOL)
        let p_j = self.config.deployment_per_cell_sol; // OUR investment amount (SOL) - NEW!
        let motherlode = board.motherlode_ore as f64 / 1e11; // Motherlode (ORE) - 11 decimals!
        let ore_price = board.ore_price_sol; // ORE price (SOL/ORE)

        // Constants
        let rake = 0.10; // 10% vaulted
        let adj = 0.95; // Variance adjustment (conservative)
        let fees = 0.00005; // Transaction fees (SOL)

        // === Step 1: Calculate my proportional share if this cell wins ===
        // my_fraction = my_investment / (total_on_cell + my_investment)
        let cell_total_after = cell_deployed + p_j;
        let my_fraction = if cell_total_after > 0.0 {
            p_j / cell_total_after
        } else {
            0.0
        };

        // === Step 2: Calculate rewards if this cell wins (1/25 probability) ===
        let win_prob = 1.0 / 25.0;

        // SOL winnings: CORRECT MECHANICS
        // 1. ONE cell wins (1/25 chance)
        // 2. ALL pot goes to that winning cell's bettors (NOT split 25 ways!)
        // 3. Within winning cell, you get: (your_bet / cell_total) × total_pot
        // 4. 10% rake is taken when you CLAIM from YOUR winnings only
        //
        // Example: 100 SOL pot, Cell 1 wins with 10 SOL total (you bet 1 SOL = 10%)
        // Your winnings = 10% × 100 SOL = 10 SOL
        // After rake = 10 × 0.9 = 9 SOL
        let my_sol_before_rake = my_fraction * total_pot; // Your % of ENTIRE pot
        let my_sol_if_win = my_sol_before_rake * (1.0 - rake); // 10% rake on YOUR winnings

        // ORE winnings: Your proportional share of ORE rewards
        // 1. Regular ORE (1 ORE): Goes to ONE random deployer (uniform lottery, NOT proportional)
        //    - If cell has N deployers, you have 1/N chance of getting full 1 ORE
        // 2. Motherlode: your_% × motherlode (when triggered, 1/625 chance)
        // CRITICAL: 10% rake is applied to ALL winnings (SOL + ORE) when you claim
        let n_deployers_after = (cell.difficulty + 1) as f64; // Existing deployers + us
        let regular_ore_chance = 1.0 / n_deployers_after; // Uniform lottery
        let regular_ore_before_rake = regular_ore_chance * 1.0 * ore_price; // 1/N chance of 1 ORE
        let regular_ore_value = regular_ore_before_rake * (1.0 - rake); // 10% rake

        let motherlode_trigger_prob = 1.0 / 625.0; // Motherlode trigger chance
        let my_motherlode_before_rake = my_fraction * motherlode * ore_price; // Your % × motherlode value
        let motherlode_value = my_motherlode_before_rake * motherlode_trigger_prob * (1.0 - rake); // 10% rake

        let ore_value_if_win = regular_ore_value + motherlode_value;

        // === Step 3: Calculate expected value ===
        // EV = (win_probability × rewards) - cost - fees
        let expected_return = win_prob * (my_sol_if_win + ore_value_if_win) * adj;
        let ev_sol = expected_return - p_j - fees;

        // DEBUG: Log ALL cells to see actual EV values (using info! instead of debug! for release builds)
        if cell.id < 25 {
            info!("🔍 Cell {} EV: pot={:.6}, deployed={:.6}, deployers={}, p_j={:.6}, my_frac={:.2}%, sol_win={:.6}, ore_val={:.6}, exp_ret={:.6}, ev_sol={:.6}, ev%={:.1}%",
                cell.id, total_pot, cell_deployed, cell.difficulty, p_j, my_fraction * 100.0, my_sol_if_win, ore_value_if_win, expected_return, ev_sol, if p_j > 0.0 { (ev_sol / p_j) * 100.0 } else { 0.0 });
        }

        // Return EV as decimal (0.15 = 15%)
        if p_j > 0.0 {
            ev_sol / p_j
        } else {
            0.0
        }
    }

    /// Calculate S_j ranking: measures "drain potential per SOL on cell"
    /// S_j = (total_pot - cell_deployed) / (cell_deployed + p_j)
    /// Higher S_j = better opportunity (more pot to drain, less SOL on cell)
    /// Focus on TOTAL SOL DEPLOYED, not number of miners
    fn calculate_s_j(&self, board: &OreBoard, cell: &Cell) -> f64 {
        let total_pot = board.pot_lamports as f64 / 1e9; // Total pot (SOL)
        let cell_deployed = cell.deployed_lamports as f64 / 1e9; // Cell deployed (SOL)
        let p_j = self.config.deployment_per_cell_sol; // OUR investment (SOL)

        // Denominator = total SOL on cell after our deployment
        // Lower cell_deployed = higher S_j (easier to get big % share)
        let denominator = cell_deployed + p_j;
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
        let total_cost: f64 = cells.len() as f64 * self.config.deployment_per_cell_sol; // OUR deployment - NEW!

        if self.config.paper_trading {
            info!(
                "📝 PAPER TRADE: Would deploy to {} cells (total: {:.6} SOL)",
                cells.len(),
                total_cost
            );
            for (idx, cell) in cells.iter().enumerate() {
                let board = BOARD.load();
                let ev = self.calculate_ev(&board, cell, time_left);
                info!(
                    "   #{}: Cell {} | Deployed: {:.6} SOL | Our Deploy: {:.6} SOL | EV: {:.1}%",
                    idx + 1,
                    cell.id,
                    cell.deployed_lamports as f64 / 1e9,
                    self.config.deployment_per_cell_sol,
                    ev * 100.0
                );
            }

            // Track this round for win/loss calculation
            let board = BOARD.load();
            self.last_round_deployed = Some(board.round_id);
            self.last_round_cells = cells.iter().map(|c| c.id).collect();
            self.last_round_amount = total_cost;

            self.stats.rounds_played += 1;
            self.stats.picks_made += cells.len() as u64;
            self.stats.total_spent_sol += total_cost;
            return Ok(());
        }

        // LIVE TRADING
        info!(
            "🚀 LIVE: Building multi-cell Deploy for {} cells (total: {:.6} SOL)",
            cells.len(),
            total_cost
        );

        // Get wallet
        let wallet = self
            .wallet
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wallet not loaded"))?;
        let authority = wallet.pubkey();

        // CRITICAL SAFETY CHECK: Verify sufficient wallet balance
        let wallet_balance = self.check_wallet_balance().await?;
        let total_needed = total_cost + 0.01; // Add 0.01 SOL for transaction fees
        if wallet_balance < total_needed {
            return Err(anyhow::anyhow!(
                "Insufficient wallet balance: need {:.6} SOL (cost: {:.6} + fees: 0.01), have {:.6} SOL",
                total_needed, total_cost, wallet_balance
            ));
        }
        info!("✅ Balance check passed: {:.6} SOL available", wallet_balance);

        // Get current round ID from Board account (NOT calculated from slot!)
        // CRITICAL FIX: round_id must match the Board account, not be calculated
        let board = BOARD.load();
        let round_id = board.round_id;

        // Validate entropy_var is set (not default address)
        if board.entropy_var == solana_sdk::pubkey::Pubkey::default() {
            return Err(anyhow::anyhow!(
                "Entropy VAR not initialized - Board state may not be synced yet. Wait for WebSocket/RPC updates."
            ));
        }

        // Validate round_id is set
        if round_id == 0 {
            return Err(anyhow::anyhow!(
                "Round ID is 0 - Board state may not be synced yet. Wait for WebSocket/RPC updates."
            ));
        }

        info!("✅ Board state validated: round_id={}, entropy_var={}", round_id, board.entropy_var);

        // Build squares array with ALL selected cells set to true
        let mut squares = [false; 25];
        let deployment_lamports = (self.config.deployment_per_cell_sol * 1e9) as u64;
        let mut total_amount = 0u64;

        for cell in cells {
            squares[cell.id as usize] = true;
            total_amount += deployment_lamports; // OUR deployment amount per cell - NEW!
        }

        // Build Deploy instruction for multiple cells
        let deploy_ix = build_deploy_instruction(
            authority,
            authority,
            total_amount, // Total amount for all cells
            round_id,
            squares,            // Multiple cells set to true
            board.entropy_var,  // CRITICAL: Use entropy_var from Board account!
        )?;

        info!(
            "✅ Multi-cell Deploy instruction built in {:?}",
            start.elapsed()
        );

        // Build and send transaction via RPC (simpler than JITO for 2s window)
        use solana_client::rpc_client::RpcClient;
        use solana_client::rpc_config::RpcSendTransactionConfig;
        use solana_sdk::transaction::Transaction;

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

        // Submit transaction - SKIP SIMULATION for first-time wallet
        // The miner account doesn't exist yet, so simulation fails
        // But the Deploy instruction creates it on-chain
        let config = RpcSendTransactionConfig {
            skip_preflight: true, // Skip simulation - account will be created on-chain
            ..Default::default()
        };

        info!("⚠️  Skipping preflight simulation (first-time wallet - account will be created)");
        let signature = rpc.send_transaction_with_config(&tx, config)?;

        info!(
            "✅ Multi-cell transaction submitted: {} | {} cells | Total: {:.6} SOL | Time: {:.1}s",
            signature,
            cells.len(),
            total_cost,
            time_left
        );

        // Update stats
        // Track this round for win/loss calculation
        let board = BOARD.load();
        self.last_round_deployed = Some(board.round_id);
        self.last_round_cells = cells.iter().map(|c| c.id).collect();
        self.last_round_amount = total_cost;

        self.stats.rounds_played += 1;
        self.stats.picks_made += cells.len() as u64;
        self.stats.total_spent_sol += total_cost;

        Ok(())
    }

    /// Calculate dynamic Jito tip based on competition
    #[allow(dead_code)]
    fn calculate_dynamic_tip(&self, board: &OreBoard) -> u64 {
        let competitors = board.cells.iter().filter(|c| c.claimed_in_mempool).count();

        let multiplier = competitors.min(MAX_COMPETITORS) as u64;
        BASE_TIP + (multiplier * 15_000)
    }

    /// Check current wallet balance via ERPC RPC
    async fn check_wallet_balance(&self) -> Result<f64> {
        use solana_client::rpc_client::RpcClient;

        // In paper trading mode, return simulated balance
        if self.config.paper_trading {
            return Ok(self.config.paper_trading_balance);
        }

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
        let net_pnl =
            self.stats.total_earned_sol - (self.stats.total_spent_sol + self.stats.total_tips_paid);
        let win_rate = if self.stats.rounds_played > 0 {
            (self.stats.rounds_won as f64 / self.stats.rounds_played as f64) * 100.0
        } else {
            0.0
        };

        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("💰 PROFIT & LOSS SUMMARY");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!(
            "📊 Starting Balance: {:.6} SOL",
            self.stats.starting_balance
        );
        info!(
            "💼 Current Balance:  {:.6} SOL",
            self.stats.last_balance_check
        );
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!(
            "📤 Total Bets Placed: {} (Win Rate: {:.1}%)",
            self.stats.rounds_played, win_rate
        );
        info!("📥 Total Claims Won:  {}", self.stats.total_claims);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!(
            "💸 Total Spent (Bets): {:.6} SOL",
            self.stats.total_spent_sol
        );
        info!(
            "💳 Total Tips Paid:    {:.6} SOL",
            self.stats.total_tips_paid
        );
        info!(
            "💰 Total Earned:       {:.6} SOL",
            self.stats.total_earned_sol
        );
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!(
            "📈 Net P&L:            {:.6} SOL ({:+.2}%)",
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

        // Use RPC pot value (updated by WebSocket and RPC)
        let total_pot = board.pot_lamports;

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
        let motherlode_ore = board.motherlode_ore as f64 / 1e11; // ORE has 11 decimals!

        // Get live ORE prices (SOL and USD)
        let ore_price_sol = self.price_fetcher.get_price().await.unwrap_or(0.0);
        let ore_price_usd = self.price_fetcher.get_price_usd().await.unwrap_or(0.0);

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
            ore_price_sol,
            ore_price_usd,
        );
    }

    /// Legacy EV-based sniping (replaced by S_j ranking multi-cell approach)
    #[allow(dead_code)]
    async fn try_ev_snipe(&self, current_pot: u64, time_left: f64, use_jito: bool) -> Result<()> {
        let board = BOARD.load();

        // Find cheapest unclaimed cell
        let cheapest_cell = board
            .cells
            .iter()
            .filter(|c| !c.claimed && !c.claimed_in_mempool)
            .min_by_key(|c| c.cost_lamports);

        if let Some(cell) = cheapest_cell {
            let ev = self.calculate_ev(&board, cell, time_left);

            if ev >= self.config.min_ev_decimal() {
                let submission_method = if use_jito {
                    "JITO (fast)"
                } else {
                    "RPC (free)"
                };
                info!("🎯 EV SNIPE: Cell {} | Cost: {:.6} SOL | Pot: {:.6} SOL | EV: {:.1}% | Via: {}",
                      cell.id, cell.cost_lamports as f64 / 1e9,
                      current_pot as f64 / 1e9, ev * 100.0, submission_method);

                // Execute snipe
                if self.config.paper_trading {
                    info!(
                        "📝 PAPER TRADE: Would EV-snipe cell {} for {:.6} SOL via {}",
                        cell.id,
                        cell.cost_lamports as f64 / 1e9,
                        submission_method
                    );
                } else {
                    if use_jito {
                        info!(
                            "⚡ Using JITO for speed (time-critical: {:.1}s left)",
                            time_left
                        );
                        // TODO: Implement JITO submission (needs refactoring execute_snipe)
                    } else {
                        info!(
                            "💸 Using free RPC submission ({:.1}s left - no rush)",
                            time_left
                        );
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
            last_round_deployed: self.last_round_deployed,
            last_round_cells: self.last_round_cells.clone(),
            last_round_amount: self.last_round_amount,
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
                            cell.deployers.clear(); // Reset deployer count for new round
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
                    OreEvent::CellDeployed {
                        cell_id,
                        authority,
                        amount_lamports,
                    } => {
                        info!(
                            "✅ Cell {} deployed: {:.6} SOL by {}",
                            cell_id,
                            amount_lamports as f64 / 1e9,
                            &authority[..8]
                        );

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
                                    cell.cost_lamports =
                                        (self.config.max_claim_cost_sol * 1e9) as u64;
                                }

                                // Track difficulty (number of deployers for pot splitting)
                                cell.difficulty = cell.deployers.len() as u64;

                                info!(
                                    "   → Cell {} totals: deployed={:.6} SOL, deployers={}",
                                    cell_id,
                                    cell.deployed_lamports as f64 / 1e9,
                                    cell.difficulty
                                );
                            }
                            BOARD.store(Arc::new(board));
                        }

                        // IMMEDIATE EXECUTION CHECK (ShredStream-first architecture)
                        // In FORCE_TEST mode, execute as soon as we have 2 cells with valid costs
                        if self.config.force_test_mode {
                            let board = BOARD.load();
                            let cells_with_cost: Vec<_> =
                                board.cells.iter().filter(|c| c.cost_lamports > 0).collect();

                            if cells_with_cost.len() >= 2 {
                                info!(
                                    "🔥 FORCE TEST: ShredStream detected {} cells, executing NOW!",
                                    cells_with_cost.len()
                                );

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

impl OreBoardSniper {
    /// Check and resolve previous round outcome (win/loss tracking)
    /// Called when a new round starts to determine if previous round was profitable
    fn resolve_previous_round(&mut self, old_round_id: u64, board: &OreBoard) {
        // Only process if we deployed to the previous round
        if let Some(deployed_round) = self.last_round_deployed {
            if deployed_round != old_round_id {
                return; // Not the round we deployed to
            }

            // Check if any of our deployed cells are now claimed (indicating they won)
            let mut winning_picks = 0;
            let mut total_payout = 0.0;

            for &cell_id in &self.last_round_cells {
                if let Some(cell) = board.cells.get(cell_id as usize) {
                    if cell.claimed {
                        winning_picks += 1;

                        // Calculate our share of this cell's payout
                        // Our share = (our deployment / total deployed on cell) * (pot / num_winning_cells)
                        // Note: In ORE V2, typically only ONE cell wins, but we account for edge cases
                        if cell.deployed_lamports > 0 {
                            let our_deployment_lamports = (self.config.deployment_per_cell_sol * 1e9) as u64;
                            let our_share = our_deployment_lamports as f64 / cell.deployed_lamports as f64;

                            // Estimate payout (pot split among winning cell deployers)
                            // This is simplified - actual payout depends on ORE protocol specifics
                            let cell_payout = board.pot_lamports as f64 / 1e9;
                            total_payout += cell_payout * our_share;
                        }
                    }
                }
            }

            // Update pick-level stats
            self.stats.picks_won += winning_picks;

            // Determine if round was profitable
            if total_payout > self.last_round_amount {
                self.stats.rounds_won += 1;
                info!(
                    "✅ Round {} WON! Picks won: {}/{}, Spent: {:.6} SOL, Won: {:.6} SOL, Profit: {:.6} SOL",
                    old_round_id,
                    winning_picks,
                    self.last_round_cells.len(),
                    self.last_round_amount,
                    total_payout,
                    total_payout - self.last_round_amount
                );
                self.stats.total_earned_sol += total_payout;
            } else {
                self.stats.rounds_lost += 1;
                if winning_picks > 0 {
                    info!(
                        "⚠️  Round {} LOSS (partial win): Picks won: {}/{}, Spent: {:.6} SOL, Won: {:.6} SOL, Loss: {:.6} SOL",
                        old_round_id,
                        winning_picks,
                        self.last_round_cells.len(),
                        self.last_round_amount,
                        total_payout,
                        self.last_round_amount - total_payout
                    );
                    self.stats.total_earned_sol += total_payout;
                } else {
                    info!(
                        "❌ Round {} LOSS: Picks won: 0/{}, Spent: {:.6} SOL",
                        old_round_id,
                        self.last_round_cells.len(),
                        self.last_round_amount
                    );
                }
            }

            // Clear tracking for next round
            self.last_round_deployed = None;
            self.last_round_cells.clear();
            self.last_round_amount = 0.0;
        }
    }
}

// === HELPER FUNCTIONS ===

/// Fetch blockhash from RPC
/// Fixed: Was returning random hash (always failed). Now fetches real blockhash.
async fn fetch_blockhash_from_shredstream() -> Result<solana_sdk::hash::Hash> {
    use solana_client::rpc_client::RpcClient;
    use std::env;

    let rpc_url = env::var("RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());

    let rpc = RpcClient::new(rpc_url);
    rpc.get_latest_blockhash()
        .map_err(|e| anyhow::anyhow!("Failed to fetch blockhash from RPC: {}", e))
}

/// Load wallet from base58 encoded private key
fn load_wallet(private_key: &str) -> Result<Keypair> {
    let decoded = bs58::decode(private_key)
        .into_vec()
        .map_err(|e| anyhow::anyhow!("Failed to decode private key: {}", e))?;

    Keypair::try_from(&decoded[..]).map_err(|e| anyhow::anyhow!("Failed to load keypair: {}", e))
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

    // Test removed - requires async runtime and full config
    // Run integration tests with `cargo test --test integration_tests` instead
}
