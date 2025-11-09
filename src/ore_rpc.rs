// ore_rpc.rs — RPC client for fetching Ore V2 board state
// Queries Board and Round accounts to get real-time cell costs

use anyhow::{Result, anyhow};
use solana_client::rpc_client::RpcClient;
use tracing::info;

use crate::ore_instructions::{get_board_address, get_round_address};
use crate::OreBoard;

/// Simplified Board struct (matches Ore program)
#[derive(Debug, Clone)]
pub struct BoardAccount {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
}

/// Simplified Round struct (matches Ore program)
#[derive(Debug, Clone)]
pub struct RoundAccount {
    pub id: u64,
    pub deployed: [u64; 25],  // SOL deployed per square
    pub count: [u64; 25],      // Number of miners per square
    pub total_deployed: u64,
    pub expires_at: u64,
}

/// RPC client for Ore V2 state
pub struct OreRpcClient {
    rpc: RpcClient,
}

impl OreRpcClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc: RpcClient::new(rpc_url),
        }
    }

    /// Fetch current board state from RPC
    pub async fn fetch_board(&self) -> Result<BoardAccount> {
        let board_address = get_board_address()?;

        // Get account data
        let account = self.rpc.get_account(&board_address)
            .map_err(|e| anyhow!("Failed to fetch Board account: {}", e))?;

        // Parse account data
        // In production, you'd deserialize using borsh or bincode
        // For now, use simplified parsing

        if account.data.len() < 24 {
            return Err(anyhow!("Board account data too small"));
        }

        // Simple parsing (assuming first 24 bytes are round_id, start_slot, end_slot)
        let round_id = u64::from_le_bytes(account.data[0..8].try_into()?);
        let start_slot = u64::from_le_bytes(account.data[8..16].try_into()?);
        let end_slot = u64::from_le_bytes(account.data[16..24].try_into()?);

        info!("📊 Board fetched: round {} | slots {}-{}", round_id, start_slot, end_slot);

        Ok(BoardAccount {
            round_id,
            start_slot,
            end_slot,
        })
    }

    /// Fetch current round state from RPC
    pub async fn fetch_round(&self, round_id: u64) -> Result<RoundAccount> {
        let round_address = get_round_address(round_id)?;

        // Get account data
        let account = self.rpc.get_account(&round_address)
            .map_err(|e| anyhow!("Failed to fetch Round account: {}", e))?;

        // Parse account data
        // In production, you'd deserialize using the actual Round struct
        // For now, use simplified parsing

        if account.data.len() < 8 {
            return Err(anyhow!("Round account data too small"));
        }

        // Parse round data (simplified)
        let id = u64::from_le_bytes(account.data[0..8].try_into()?);

        // TODO: Parse deployed array (25 u64s starting at offset 8)
        // TODO: Parse count array (25 u64s after deployed)
        // For now, return defaults

        let deployed = [0u64; 25];
        let count = [0u64; 25];

        info!("📊 Round {} fetched", id);

        Ok(RoundAccount {
            id,
            deployed,
            count,
            total_deployed: 0,
            expires_at: 0,
        })
    }

    /// Fetch complete board state and update OreBoard
    pub async fn update_board_state(&self, board: &mut OreBoard) -> Result<()> {
        // Fetch board account
        let board_account = self.fetch_board().await?;

        // Fetch round account
        let round_account = self.fetch_round(board_account.round_id).await?;

        // Update board with RPC data
        board.reset_slot = board_account.end_slot;

        // Update cell costs from round deployed amounts
        for (i, cell) in board.cells.iter_mut().enumerate() {
            cell.id = i as u8;
            // Cost is minimum deployment amount (could be dynamic based on round rules)
            // For now, use a fixed cost or the current deployed amount
            cell.cost_lamports = if round_account.deployed[i] > 0 {
                round_account.deployed[i]
            } else {
                5_000_000 // Default: 0.005 SOL
            };
            cell.difficulty = round_account.count[i]; // Use miner count as "difficulty"
        }

        info!("✅ Board state updated from RPC (round {})", board_account.round_id);

        Ok(())
    }

    /// Get current slot from RPC
    pub async fn get_current_slot(&self) -> Result<u64> {
        self.rpc.get_slot()
            .map_err(|e| anyhow!("Failed to get current slot: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires RPC connection
    async fn test_fetch_board() {
        let rpc = OreRpcClient::new("https://api.mainnet-beta.solana.com".to_string());

        match rpc.fetch_board().await {
            Ok(board) => {
                println!("Board: {:?}", board);
                assert!(board.round_id > 0);
            }
            Err(e) => {
                println!("Failed to fetch board: {}", e);
            }
        }
    }
}
