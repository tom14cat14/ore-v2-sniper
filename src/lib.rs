// Ore Board Sniper - Real Ore V2 Protocol

pub mod config;
pub mod ore_board_sniper;
pub mod ore_instructions;
pub mod ore_shredstream;
pub mod ore_rpc;
pub mod ore_jito;
pub mod dashboard;
pub mod jupiter_price;
pub mod ore_board_websocket;

// Re-exports for convenience
pub use config::{OreConfig, DailyStats};
pub use ore_board_sniper::{OreBoardSniper, OreBoard, Cell, update_board_from_log, mark_mempool_deploy};
pub use ore_instructions::{build_deploy_instruction, build_checkpoint_instruction, get_board_address, get_miner_address, get_round_address};
pub use ore_shredstream::{OreShredStreamProcessor, OreEvent, OreStreamEvent};
pub use ore_rpc::{OreRpcClient, BoardAccount, RoundAccount, TreasuryAccount};
pub use ore_jito::OreJitoClient;
