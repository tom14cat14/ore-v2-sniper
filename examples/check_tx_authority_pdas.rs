// Check PDA derivations for successful transaction authority
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

const ORE_PROGRAM_ID: &str = "oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv";
const BOARD: &[u8] = b"board";
const MINER: &[u8] = b"miner";
const AUTOMATION: &[u8] = b"automation";
const ROUND: &[u8] = b"round";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ore_program = Pubkey::from_str(ORE_PROGRAM_ID)?;
    let authority = Pubkey::from_str("7vL6NaGtf636nw3yzLRMuXxR4FZ2P9EUxDBm7cEBSLRS")?; // From successful tx

    println!("=== Successful TX Authority PDAs ===\n");
    println!("Authority: {}", authority);

    // Miner PDA
    let (miner_pda, _) =
        Pubkey::find_program_address(&[MINER, &authority.to_bytes()], &ore_program);
    println!("Miner PDA: {}", miner_pda);

    // Automation PDA
    let (auto_pda, _) =
        Pubkey::find_program_address(&[AUTOMATION, &authority.to_bytes()], &ore_program);
    println!("Automation PDA: {}", auto_pda);

    // Check matches in transaction
    let tx_accounts = vec![
        ("0", "7vL6NaGtf636nw3yzLRMuXxR4FZ2P9EUxDBm7cEBSLRS"),
        ("1", "BrcSxdp1nXFzou1YyDnQJcPNBNHgoypZmTsyKBSLLXzi"),
        ("2", "PgzDnw8wsJyQwzGBzTorQf5D52ytUHjJ9QQsT32N6GE"),
        ("3", "88vW691KnvcLLtByaR1X2o7rL9wWYD1rpFYVNPT3QKQv"),
        ("4", "45db2FSR4mcXdSVVZbKbwojU6uYDpMyhpEi7cC8nHaWG"),
        ("5", "4YbbUm3seC8Zi3QCfbCn6JgJsg4cAy9qJB9H79tyKPMo"),
        ("6", "Gg8ABv6pAXCMX77PUEVjuxDiG2Ce5EXwzMzJiya4fUb1"),
        ("7", "BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E"),
    ];

    println!("\n=== Account Matches ===");
    for (idx, addr) in &tx_accounts {
        let pubkey = Pubkey::from_str(addr)?;
        if pubkey == miner_pda {
            println!("Account {}: Miner PDA ✅", idx);
        } else if pubkey == auto_pda {
            println!("Account {}: Automation PDA ✅", idx);
        }
    }

    Ok(())
}
