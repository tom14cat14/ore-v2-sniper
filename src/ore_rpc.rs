// ore_rpc.rs — RPC client for fetching Ore V2 board state
// Queries Board and Round accounts to get real-time cell costs and pot size

use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use tracing::{debug, info};

use crate::ore_instructions::{BOARD, ORE_PROGRAM_ID, ROUND};

/// Board account from Ore V2 program (24 bytes total after discriminator)
#[derive(Debug, Clone)]
pub struct BoardAccount {
    pub round_id: u64,   // Current round number
    pub start_slot: u64, // Round start slot
    pub end_slot: u64,   // Round end slot (when reset happens)
}

/// Round account from Ore V2 program (contains pot and cell costs)
#[derive(Debug, Clone)]
pub struct RoundAccount {
    pub id: u64,
    pub deployed: [u64; 25], // SOL deployed per square (cell cost = min to claim)
    pub count: [u64; 25],    // Number of miners per square
    pub total_deployed: u64, // Total pot size
    pub total_winnings: u64, // Total winnings for round
}

/// Treasury account from Ore V2 program (contains Motherlode)
#[derive(Debug, Clone)]
pub struct TreasuryAccount {
    pub motherlode: u64, // Current Motherlode in ORE tokens
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
        // Get Board PDA
        let ore_program = ORE_PROGRAM_ID.parse::<Pubkey>()?;
        let (board_pda, _bump) = Pubkey::find_program_address(&[BOARD], &ore_program);

        debug!("📡 Fetching Board PDA: {}", board_pda);

        // Get account data
        let account = self
            .rpc
            .get_account(&board_pda)
            .map_err(|e| anyhow!("Failed to fetch Board account: {}", e))?;

        // Parse Board account (8 byte discriminator + 24 bytes data)
        if account.data.len() < 32 {
            return Err(anyhow!(
                "Board account data too small: {} bytes",
                account.data.len()
            ));
        }

        // Skip 8-byte discriminator, then parse 3 u64 fields
        let round_id = u64::from_le_bytes(account.data[8..16].try_into()?);
        let start_slot = u64::from_le_bytes(account.data[16..24].try_into()?);
        let end_slot = u64::from_le_bytes(account.data[24..32].try_into()?);

        info!(
            "📊 Board: round {} | reset at slot {} (slots {}-{})",
            round_id, end_slot, start_slot, end_slot
        );

        Ok(BoardAccount {
            round_id,
            start_slot,
            end_slot,
        })
    }

    /// Fetch current round state from RPC
    pub async fn fetch_round(&self, round_id: u64) -> Result<RoundAccount> {
        // Get Round PDA
        let ore_program = ORE_PROGRAM_ID.parse::<Pubkey>()?;
        let round_id_bytes = round_id.to_le_bytes();
        let (round_pda, _bump) =
            Pubkey::find_program_address(&[ROUND, &round_id_bytes], &ore_program);

        debug!(
            "📡 Fetching Round PDA: {} (round_id={})",
            round_pda, round_id
        );

        // Get account data
        let account = self
            .rpc
            .get_account(&round_pda)
            .map_err(|e| anyhow!("Failed to fetch Round account: {}", e))?;

        // Parse Round account structure:
        // 8 bytes: discriminator
        // 8 bytes: id
        // 200 bytes: deployed[25] (25 * 8)
        // 32 bytes: slot_hash
        // 200 bytes: count[25] (25 * 8)
        // ... then motherlode, rent_payer, top_miner, rewards, totals

        if account.data.len() < 8 + 8 + 200 + 32 + 200 + 8 + 8 + 32 + 32 + 8 + 8 + 8 + 8 {
            return Err(anyhow!(
                "Round account data too small: {} bytes",
                account.data.len()
            ));
        }

        let mut offset = 8; // Skip discriminator

        // Parse id
        let id = u64::from_le_bytes(account.data[offset..offset + 8].try_into()?);
        offset += 8;

        // Parse deployed array (25 u64s)
        let mut deployed = [0u64; 25];
        for i in 0..25 {
            deployed[i] = u64::from_le_bytes(account.data[offset..offset + 8].try_into()?);
            offset += 8;
        }
        offset += 32; // Skip slot_hash

        // Parse count array (25 u64s)
        let mut count = [0u64; 25];
        for i in 0..25 {
            count[i] = u64::from_le_bytes(account.data[offset..offset + 8].try_into()?);
            offset += 8;
        }

        // Skip expires_at, motherlode, rent_payer, top_miner, top_miner_reward
        offset += 8 + 8 + 32 + 32 + 8;

        // Parse total_deployed
        let total_deployed = u64::from_le_bytes(account.data[offset..offset + 8].try_into()?);
        offset += 8;
        offset += 8; // Skip total_vaulted

        // Parse total_winnings
        let total_winnings = u64::from_le_bytes(account.data[offset..offset + 8].try_into()?);

        info!(
            "📊 Round {}: pot={:.6} SOL, deployed cells={}/25",
            id,
            total_deployed as f64 / 1e9,
            deployed.iter().filter(|&&x| x > 0).count()
        );

        Ok(RoundAccount {
            id,
            deployed,
            count,
            total_deployed,
            total_winnings,
        })
    }

    /// Fetch complete board state and update OreBoard with real data
    pub async fn update_board_state(&self, board: &mut crate::OreBoard) -> Result<()> {
        // Fetch board account
        let board_account = self.fetch_board().await?;

        // Fetch round account
        let round_account = self.fetch_round(board_account.round_id).await?;

        // Update board with RPC data
        board.reset_slot = board_account.end_slot;

        // Update cell costs from round deployed amounts
        for (i, cell) in board.cells.iter_mut().enumerate() {
            cell.id = i as u8;

            // Cell cost = amount already deployed (this is what you need to match/exceed)
            // If cell is unclaimed, use a min cost (typically 0.001-0.01 SOL)
            cell.cost_lamports = if round_account.deployed[i] > 0 {
                round_account.deployed[i]
            } else {
                1_000_000 // Default minimum: 0.001 SOL for unclaimed cells
            };

            cell.claimed = round_account.deployed[i] > 0;
            cell.difficulty = round_account.count[i]; // Number of miners on this cell
        }

        info!(
            "✅ Board updated: round {}, pot={:.6} SOL, {}/25 cells claimed",
            board_account.round_id,
            round_account.total_deployed as f64 / 1e9,
            round_account.deployed.iter().filter(|&&x| x > 0).count()
        );

        Ok(())
    }

    /// Get current slot from RPC
    pub async fn get_current_slot(&self) -> Result<u64> {
        self.rpc
            .get_slot()
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
