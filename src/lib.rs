// Ore Board Sniper - Real Ore V2 Protocol

pub mod config;
pub mod ore_board_sniper;
pub mod ore_instructions;
pub mod ore_rpc;
pub mod ore_shredstream;
// pub mod ore_jito;  // Unused - bot uses RPC submission (2.8s window is sufficient)
pub mod dashboard;
pub mod jupiter_price;
pub mod ore_board_websocket;

// Re-exports for convenience
pub use config::{DailyStats, OreConfig};
pub use ore_board_sniper::{
    mark_mempool_deploy, update_board_from_log, Cell, OreBoard, OreBoardSniper,
};
pub use ore_instructions::{
    build_checkpoint_instruction, build_deploy_instruction, get_board_address, get_miner_address,
    get_round_address,
};
pub use ore_rpc::{BoardAccount, OreRpcClient, RoundAccount, TreasuryAccount};
pub use ore_shredstream::{OreEvent, OreShredStreamProcessor, OreStreamEvent};
// pub use ore_jito::OreJitoClient;  // Unused - bot uses RPC submission
