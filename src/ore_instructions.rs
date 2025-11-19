// ore_instructions.rs — Real Ore V2 instruction builders
// Based on official Ore SDK from https://github.com/HardhatChad/ore

use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

// Ore V2 Program ID (mainnet-beta)
pub const ORE_PROGRAM_ID: &str = "oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv";

// Entropy API Program ID (for randomness)
// NOTE: This is Ore's custom entropy program, NOT the mainnet Entropy program
pub const ENTROPY_PROGRAM_ID: &str = "3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X";

// PDA Seeds (from ore-api/src/consts.rs)
pub const BOARD: &[u8] = b"board";
pub const MINER: &[u8] = b"miner";
pub const ROUND: &[u8] = b"round";
pub const AUTOMATION: &[u8] = b"automation";

// Instruction discriminators (from ore-api/src/instruction.rs)
pub const DEPLOY_DISCRIMINATOR: u8 = 6;
pub const CHECKPOINT_DISCRIMINATOR: u8 = 2;

// Constants
pub const ONE_MINUTE_SLOTS: u64 = 150;
pub const CHECKPOINT_FEE: u64 = 10_000; // 0.00001 SOL

/// Deploy instruction data structure
#[derive(Debug, Clone)]
pub struct DeployData {
    pub amount: [u8; 8],  // u64 in little-endian
    pub squares: [u8; 4], // 32-bit mask in little-endian
}

impl DeployData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![DEPLOY_DISCRIMINATOR];
        data.extend_from_slice(&self.amount);
        data.extend_from_slice(&self.squares);
        data
    }
}

/// Checkpoint instruction data structure
#[derive(Debug, Clone)]
pub struct CheckpointData {}

impl CheckpointData {
    pub fn to_bytes(&self) -> Vec<u8> {
        vec![CHECKPOINT_DISCRIMINATOR]
    }
}

/// Build Deploy instruction to bet SOL on board squares
///
/// Based on ore-api/src/sdk.rs lines 97-139
///
/// # Arguments
/// * `signer` - Transaction signer (pays fees)
/// * `authority` - Miner authority (wallet that owns positions)
/// * `amount` - Amount of SOL to deploy per square (in lamports)
/// * `round_id` - Current round ID
/// * `squares` - 25 bool array (true = deploy to this square)
/// * `entropy_var` - Entropy VAR address (from Board account, not derived!)
///
/// # Returns
/// * Solana Instruction ready for bundle
pub fn build_deploy_instruction(
    signer: Pubkey,
    authority: Pubkey,
    amount: u64,
    round_id: u64,
    squares: [bool; 25],
    entropy_var: Pubkey,
) -> Result<Instruction> {
    // Convert 25 bool array to 32-bit mask
    let mut mask: u32 = 0;
    for (i, &square) in squares.iter().enumerate().take(25) {
        if square {
            mask |= 1 << i;
        }
    }

    // Derive PDAs
    let ore_program_id = ORE_PROGRAM_ID.parse::<Pubkey>()?;
    let entropy_program_id = ENTROPY_PROGRAM_ID.parse::<Pubkey>()?;

    let (automation_address, _) =
        Pubkey::find_program_address(&[AUTOMATION, &authority.to_bytes()], &ore_program_id);
    let (board_address, _) = Pubkey::find_program_address(&[BOARD], &ore_program_id);
    let (miner_address, _) =
        Pubkey::find_program_address(&[MINER, &authority.to_bytes()], &ore_program_id);
    let (round_address, _) =
        Pubkey::find_program_address(&[ROUND, &round_id.to_le_bytes()], &ore_program_id);

    // CRITICAL FIX: Use entropy_var from Board account instead of deriving it!
    // The Board account stores the current entropy VAR, which may rotate.
    // entropy_var parameter is passed from Board WebSocket/RPC data.

    // Build instruction data
    let data = DeployData {
        amount: amount.to_le_bytes(),
        squares: mask.to_le_bytes(),
    };

    // Build instruction
    Ok(Instruction {
        program_id: ore_program_id,
        accounts: vec![
            AccountMeta::new(signer, true),                       // Signer
            AccountMeta::new(authority, false),                   // Authority
            AccountMeta::new(automation_address, false),          // Automation (may be empty)
            AccountMeta::new(board_address, false),               // Board
            AccountMeta::new(miner_address, false),               // Miner
            AccountMeta::new(round_address, false),               // Round
            AccountMeta::new_readonly(system_program::ID, false), // System program
            AccountMeta::new(entropy_var, false),                 // Entropy VAR (from Board!)
            AccountMeta::new_readonly(entropy_program_id, false), // Entropy program
        ],
        data: data.to_bytes(),
    })
}

/// Build Checkpoint instruction to claim rewards after round ends
///
/// Based on ore-api/src/sdk.rs lines 256-273
///
/// # Arguments
/// * `signer` - Transaction signer (can be anyone if round expired >12h)
/// * `board_address` - Board PDA
/// * `miner_address` - Miner PDA (for the authority)
/// * `round_id` - Round ID to checkpoint
///
/// # Returns
/// * Solana Instruction ready for bundle
pub fn build_checkpoint_instruction(
    signer: Pubkey,
    board_address: Pubkey,
    miner_address: Pubkey,
    round_id: u64,
) -> Result<Instruction> {
    let ore_program_id = ORE_PROGRAM_ID.parse::<Pubkey>()?;

    // Derive round PDA
    let (round_address, _) =
        Pubkey::find_program_address(&[ROUND, &round_id.to_le_bytes()], &ore_program_id);

    // Derive treasury PDA
    let (treasury_address, _) = Pubkey::find_program_address(&[b"treasury"], &ore_program_id);

    // Build instruction data
    let data = CheckpointData {};

    // Build instruction
    Ok(Instruction {
        program_id: ore_program_id,
        accounts: vec![
            AccountMeta::new(signer, true),                       // Signer
            AccountMeta::new(board_address, false),               // Board
            AccountMeta::new(miner_address, false),               // Miner
            AccountMeta::new(round_address, false),               // Round
            AccountMeta::new(treasury_address, false),            // Treasury
            AccountMeta::new_readonly(system_program::ID, false), // System program
        ],
        data: data.to_bytes(),
    })
}

/// Derive board PDA
pub fn get_board_address() -> Result<Pubkey> {
    let ore_program_id = ORE_PROGRAM_ID.parse::<Pubkey>()?;
    let (board, _) = Pubkey::find_program_address(&[BOARD], &ore_program_id);
    Ok(board)
}

/// Derive miner PDA for a given authority
pub fn get_miner_address(authority: Pubkey) -> Result<Pubkey> {
    let ore_program_id = ORE_PROGRAM_ID.parse::<Pubkey>()?;
    let (miner, _) = Pubkey::find_program_address(&[MINER, &authority.to_bytes()], &ore_program_id);
    Ok(miner)
}

/// Derive round PDA for a given round ID
pub fn get_round_address(round_id: u64) -> Result<Pubkey> {
    let ore_program_id = ORE_PROGRAM_ID.parse::<Pubkey>()?;
    let (round, _) =
        Pubkey::find_program_address(&[ROUND, &round_id.to_le_bytes()], &ore_program_id);
    Ok(round)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_conversion() {
        // Test 25 bool array to 32-bit mask conversion
        let mut squares = [false; 25];
        squares[0] = true; // Cell 0
        squares[5] = true; // Cell 5
        squares[24] = true; // Cell 24

        let mut mask: u32 = 0;
        for (i, &square) in squares.iter().enumerate().take(25) {
            if square {
                mask |= 1 << i;
            }
        }

        // Expected mask: bit 0, bit 5, bit 24 set
        assert_eq!(mask, 0b1_0000_0000_0000_0000_0010_0001);
        assert_eq!(mask, 16777249);
    }

    #[test]
    fn test_deploy_data_encoding() {
        let data = DeployData {
            amount: 5_000_000u64.to_le_bytes(), // 0.005 SOL
            squares: 1u32.to_le_bytes(),        // Only cell 0
        };

        let bytes = data.to_bytes();
        assert_eq!(bytes[0], DEPLOY_DISCRIMINATOR);
        assert_eq!(bytes.len(), 1 + 8 + 4); // discriminator + amount + squares
    }

    #[test]
    fn test_checkpoint_data_encoding() {
        let data = CheckpointData {};
        let bytes = data.to_bytes();
        assert_eq!(bytes[0], CHECKPOINT_DISCRIMINATOR);
        assert_eq!(bytes.len(), 1); // Just discriminator
    }
}
