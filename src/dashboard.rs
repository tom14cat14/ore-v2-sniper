// dashboard.rs — Dashboard integration for sol-pulse.com
// Writes bot status and events to JSON files for web dashboard

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use tracing::{debug, warn};

use crate::ore_board_sniper::{OreBoard, SnipeStats};

const STATUS_FILE: &str = "/home/tom14cat14/ORE/data/status.json";
const EVENTS_FILE: &str = "/home/tom14cat14/ORE/data/events.json";
const MAX_EVENTS: usize = 100; // Keep last 100 events

/// Dashboard status structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStatus {
    pub bot_running: bool,
    pub paper_trading: bool,
    pub round_id: u64,
    pub pot_size: f64,       // in SOL
    pub motherlode_ore: f64, // in ORE (Motherlode accumulation)
    pub ore_price_sol: f64,  // Live ORE price in SOL from Jupiter
    pub ore_price_usd: f64,  // Live ORE/USD price from Jupiter
    pub reset_slot: u64,
    pub current_slot: u64,
    pub time_remaining_seconds: f64, // Time until reset (countdown from 60s)
    pub cells_claimed: usize,
    pub wallet_balance: f64,
    pub wallet_address: String,
    pub shredstream_latency_ms: Option<f64>,
    pub rpc_latency_ms: Option<f64>,
    pub entries_processed: u64,
    pub shredstream_connected: bool,
    // Round-level metrics
    pub rounds_played: u64,
    pub rounds_won: u64,
    pub rounds_lost: u64,
    pub round_win_rate: f64,
    // Pick-level metrics (can make multiple picks per round)
    pub picks_made: u64,
    pub picks_won: u64,
    pub pick_win_rate: f64,
    // Financial metrics
    pub total_spent: f64,
    pub total_earned: f64,
    pub board: BoardStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardStatus {
    pub pot_size: f64, // in SOL
    pub cells: Vec<CellStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellStatus {
    pub id: u8,
    pub cost_lamports: u64, // DEPRECATED - misleading, kept for backwards compatibility
    pub deployed_lamports: u64, // CRITICAL: Total SOL deployed to this cell (needed for EV calculations)
    pub claimed: bool,
    pub difficulty: u64,
}

/// Event for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub slot: Option<u64>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardEvents {
    pub events: Vec<DashboardEvent>,
    pub count: usize,
}

/// Dashboard writer
#[derive(Clone)]
pub struct DashboardWriter {
    events: Vec<DashboardEvent>,
}

impl Default for DashboardWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardWriter {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Write current status to file
    pub fn write_status(
        &self,
        board: &OreBoard,
        stats: &SnipeStats,
        paper_trading: bool,
        wallet_address: &str,
        shredstream_latency_ms: Option<f64>,
        rpc_latency_ms: Option<f64>,
        entries_processed: u64,
        shredstream_connected: bool,
        pot_size_lamports: u64,
        motherlode_ore: f64,
        ore_price_sol: f64,
        ore_price_usd: f64,
    ) {
        let cells_claimed = board.cells.iter().filter(|c| c.claimed).count();

        // Calculate time remaining until reset (countdown from ~60s)
        let slots_remaining = board.reset_slot.saturating_sub(board.current_slot) as f64;
        let time_remaining_seconds = (slots_remaining * 0.4).max(0.0); // 0.4s per slot

        let status = DashboardStatus {
            bot_running: true,
            paper_trading,
            round_id: board.round_id,
            pot_size: pot_size_lamports as f64 / 1e9,
            motherlode_ore,
            ore_price_sol,
            ore_price_usd,
            reset_slot: board.reset_slot,
            current_slot: board.current_slot,
            time_remaining_seconds,
            cells_claimed,
            wallet_balance: stats.last_balance_check,
            wallet_address: wallet_address.to_string(),
            shredstream_latency_ms,
            rpc_latency_ms,
            entries_processed,
            shredstream_connected,
            // Round-level metrics
            rounds_played: stats.rounds_played,
            rounds_won: stats.rounds_won,
            rounds_lost: stats.rounds_lost,
            round_win_rate: if stats.rounds_played > 0 {
                (stats.rounds_won as f64 / stats.rounds_played as f64) * 100.0
            } else {
                0.0
            },
            // Pick-level metrics
            picks_made: stats.picks_made,
            picks_won: stats.picks_won,
            pick_win_rate: if stats.picks_made > 0 {
                (stats.picks_won as f64 / stats.picks_made as f64) * 100.0
            } else {
                0.0
            },
            // Financial metrics
            total_spent: stats.total_spent_sol,
            total_earned: stats.total_earned_sol,
            board: BoardStatus {
                pot_size: pot_size_lamports as f64 / 1e9,
                cells: board
                    .cells
                    .iter()
                    .map(|cell| CellStatus {
                        id: cell.id,
                        cost_lamports: cell.cost_lamports, // Kept for backwards compatibility
                        deployed_lamports: cell.deployed_lamports, // CRITICAL: Total SOL on cell for EV calculations
                        claimed: cell.claimed,
                        difficulty: cell.difficulty,
                    })
                    .collect(),
            },
        };

        match File::create(STATUS_FILE) {
            Ok(file) => {
                if let Err(e) = serde_json::to_writer_pretty(file, &status) {
                    warn!("Failed to write dashboard status: {}", e);
                } else {
                    debug!("📊 Dashboard status updated");
                }
            }
            Err(e) => {
                warn!("Failed to create dashboard status file: {}", e);
            }
        }
    }

    /// Add an event
    pub fn add_event(&mut self, event: DashboardEvent) {
        self.events.insert(0, event); // Insert at beginning (newest first)

        // Keep only last MAX_EVENTS
        if self.events.len() > MAX_EVENTS {
            self.events.truncate(MAX_EVENTS);
        }

        // Write events to file
        self.write_events();
    }

    /// Write events to file
    fn write_events(&self) {
        let events_data = DashboardEvents {
            count: self.events.len(),
            events: self.events.clone(),
        };

        match File::create(EVENTS_FILE) {
            Ok(file) => {
                if let Err(e) = serde_json::to_writer_pretty(file, &events_data) {
                    warn!("Failed to write dashboard events: {}", e);
                } else {
                    debug!("📋 Dashboard events updated ({} events)", self.events.len());
                }
            }
            Err(e) => {
                warn!("Failed to create dashboard events file: {}", e);
            }
        }
    }

    /// Load existing events from file (on startup)
    pub fn load_events(&mut self) {
        let events_path = Path::new(EVENTS_FILE);
        if events_path.exists() {
            match File::open(events_path) {
                Ok(file) => match serde_json::from_reader::<_, DashboardEvents>(file) {
                    Ok(events_data) => {
                        self.events = events_data.events;
                        debug!(
                            "📋 Loaded {} existing events from dashboard",
                            self.events.len()
                        );
                    }
                    Err(e) => {
                        warn!("Failed to parse dashboard events: {}", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to open dashboard events file: {}", e);
                }
            }
        }
    }
}

/// Helper to get current timestamp in ISO 8601 format
pub fn get_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}
