// Check PDA derivations for Ore lottery
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

const ORE_PROGRAM_ID: &str = "oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv";
const BOARD: &[u8] = b"board";
const MINER: &[u8] = b"miner";
const AUTOMATION: &[u8] = b"automation";
const ROUND: &[u8] = b"round";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ore_program = Pubkey::from_str(ORE_PROGRAM_ID)?;
    let wallet = Pubkey::from_str("8MBg94RS4WTPbggpkAUbsxauqq5HfL5DEvRn8rGcQB7u")?;

    println!("=== Ore V2 PDA Addresses ===\n");

    // Board PDA (global)
    let (board_pda, board_bump) = Pubkey::find_program_address(&[BOARD], &ore_program);
    println!("Board PDA: {}", board_pda);
    println!("  Bump: {}", board_bump);
    println!();

    // Miner PDA (wallet-specific)
    let (miner_pda, miner_bump) =
        Pubkey::find_program_address(&[MINER, &wallet.to_bytes()], &ore_program);
    println!("Miner PDA: {}", miner_pda);
    println!("  Bump: {}", miner_bump);
    println!("  Seeds: [MINER, wallet]");
    println!();

    // Automation PDA (wallet-specific)
    let (auto_pda, auto_bump) =
        Pubkey::find_program_address(&[AUTOMATION, &wallet.to_bytes()], &ore_program);
    println!("Automation PDA: {}", auto_pda);
    println!("  Bump: {}", auto_bump);
    println!("  Seeds: [AUTOMATION, wallet]");
    println!();

    // Current round (estimate from current slot)
    // Slot ~379224825 / 150 slots per round = round 2528165
    let round_id = 2528165u64;
    let (round_pda, round_bump) =
        Pubkey::find_program_address(&[ROUND, &round_id.to_le_bytes()], &ore_program);
    println!("Round {} PDA: {}", round_id, round_pda);
    println!("  Bump: {}", round_bump);
    println!("  Seeds: [ROUND, round_id_le_bytes]");
    println!();

    Ok(())
}
