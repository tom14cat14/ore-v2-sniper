// ore_board_websocket.rs — WebSocket subscribers for Ore V2 Board, Round, and Treasury accounts
// Real-time account state updates via WebSocket (replaces periodic RPC polling)

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use solana_client::{nonblocking::pubsub_client::PubsubClient, rpc_config::RpcAccountInfoConfig};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn}; // For stream.next()

use crate::ore_instructions::{BOARD, ORE_PROGRAM_ID, ROUND};
use crate::ore_rpc::{BoardAccount, RoundAccount, TreasuryAccount};

const TREASURY: &[u8] = b"treasury";

/// Board account update message sent via broadcast channel
#[derive(Debug, Clone)]
pub struct BoardUpdate {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub entropy_var: Pubkey,
}

impl From<BoardAccount> for BoardUpdate {
    fn from(board: BoardAccount) -> Self {
        Self {
            round_id: board.round_id,
            start_slot: board.start_slot,
            end_slot: board.end_slot,
            entropy_var: Pubkey::default(), // Not available in BoardAccount (RPC data)
        }
    }
}

/// Round account update message sent via broadcast channel
#[derive(Debug, Clone)]
pub struct RoundUpdate {
    pub id: u64,
    pub deployed: [u64; 25], // SOL deployed per cell
    pub count: [u64; 25],    // Number of deployers per cell
    pub total_deployed: u64, // Total pot size
    pub total_winnings: u64, // Total winnings for round
}

impl From<RoundAccount> for RoundUpdate {
    fn from(round: RoundAccount) -> Self {
        Self {
            id: round.id,
            deployed: round.deployed,
            count: round.count,
            total_deployed: round.total_deployed,
            total_winnings: round.total_winnings,
        }
    }
}

/// Treasury account update message sent via broadcast channel
#[derive(Debug, Clone)]
pub struct TreasuryUpdate {
    pub motherlode_balance: u64, // Accumulated ORE jackpot (÷1e11 for ORE)
    pub total_minted: u64,       // Total ORE ever minted
}

impl From<TreasuryAccount> for TreasuryUpdate {
    fn from(treasury: TreasuryAccount) -> Self {
        Self {
            motherlode_balance: treasury.motherlode,
            total_minted: 0, // Not available in TreasuryAccount
        }
    }
}

/// WebSocket subscriber for Board account
pub struct BoardWebSocketSubscriber {
    ws_url: String,
    board_pda: Pubkey,
}

impl BoardWebSocketSubscriber {
    pub fn new(ws_url: String) -> Result<Self> {
        let ore_program = ORE_PROGRAM_ID.parse::<Pubkey>()?;
        let (board_pda, _) = Pubkey::find_program_address(&[BOARD], &ore_program);

        info!("📡 Board WebSocket subscriber initialized");
        info!("   Board PDA: {}", board_pda);
        info!("   WebSocket URL: {}", ws_url);

        Ok(Self { ws_url, board_pda })
    }

    /// Subscribe to Board account updates and send to broadcast channel
    pub async fn subscribe(&self, tx: broadcast::Sender<BoardUpdate>) -> Result<()> {
        loop {
            match self.subscribe_inner(tx.clone()).await {
                Ok(_) => {
                    warn!("⚠️  Board WebSocket subscription ended normally, reconnecting...");
                }
                Err(e) => {
                    error!("❌ Board WebSocket error: {} - reconnecting in 5s", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn subscribe_inner(&self, tx: broadcast::Sender<BoardUpdate>) -> Result<()> {
        info!("🔌 Connecting to Board WebSocket: {}", self.ws_url);

        let pubsub = PubsubClient::new(&self.ws_url)
            .await
            .map_err(|e| anyhow!("Failed to connect to WebSocket: {}", e))?;

        info!("✅ Board WebSocket connected");

        // Subscribe to Board PDA account changes (with confirmed commitment)
        let config = RpcAccountInfoConfig {
            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            data_slice: None,
            min_context_slot: None,
        };

        let (mut stream, _unsub) = pubsub
            .account_subscribe(&self.board_pda, Some(config))
            .await
            .map_err(|e| anyhow!("Failed to subscribe to Board account: {}", e))?;

        info!("✅ Subscribed to Board PDA account updates");

        // Process incoming account updates
        while let Some(response) = stream.next().await {
            // Extract bytes from UiAccountData enum
            use base64::{engine::general_purpose, Engine as _};
            use solana_account_decoder::UiAccountData;

            let bytes: Vec<u8> = match &response.value.data {
                UiAccountData::Binary(encoded_data, _)
                | UiAccountData::LegacyBinary(encoded_data) => {
                    debug!(
                        "📦 Board data: len={}, preview={}",
                        encoded_data.len(),
                        &encoded_data[..encoded_data.len().min(100)]
                    );

                    // Decode base64 string to bytes
                    match general_purpose::STANDARD.decode(encoded_data) {
                        Ok(data) => {
                            debug!("✅ Board data decoded: {} bytes", data.len());
                            data
                        }
                        Err(e) => {
                            warn!(
                                "⚠️  Board WS: base64 decode FAILED: {} (len={}, preview={})",
                                e,
                                encoded_data.len(),
                                &encoded_data[..encoded_data.len().min(50)]
                            );
                            continue;
                        }
                    }
                }
                UiAccountData::Json(_) => {
                    warn!("⚠️  Received JSON account data, expected binary");
                    continue;
                }
            };

            match self.parse_board_update(&bytes) {
                Ok(update) => {
                    debug!(
                        "📊 Board update: round {}, reset_slot {}, entropy_var {}",
                        update.round_id, update.end_slot, update.entropy_var
                    );

                    // Send to broadcast channel (ignore send errors - no subscribers is OK)
                    let _ = tx.send(update);
                }
                Err(e) => {
                    warn!("⚠️  Failed to parse Board update: {}", e);
                }
            }
        }

        Ok(())
    }

    fn parse_board_update(&self, data: &[u8]) -> Result<BoardUpdate> {
        // DEBUG: Log raw bytes to investigate 33-byte structure
        debug!(
            "📊 Board account bytes ({}): {:02x?}",
            data.len(),
            &data[..data.len().min(40)]
        );

        // INVESTIGATION: 33 bytes suggests:
        // - 1 byte: discriminator OR
        // - 8 bytes: discriminator + 1 byte round_id OR
        // - Different structure entirely

        // Try 32-byte format: Just a Pubkey (no discriminator)
        if data.len() == 32 {
            let current_round_pda = Pubkey::try_from(data)?;
            debug!(
                "📊 Board (32-byte format): current_round_pda = {}",
                current_round_pda
            );

            // Return dummy values - Round account will provide actual data
            return Ok(BoardUpdate {
                round_id: 0,
                start_slot: 0,
                end_slot: 0,
                entropy_var: current_round_pda,
            });
        }

        // Try 33-byte format: 1 byte discriminator + 32 bytes Pubkey
        if data.len() == 33 {
            let current_round_pda = Pubkey::try_from(&data[1..33])?;
            debug!(
                "📊 Board (33-byte format): current_round_pda = {}",
                current_round_pda
            );

            return Ok(BoardUpdate {
                round_id: 0,
                start_slot: 0,
                end_slot: 0,
                entropy_var: current_round_pda,
            });
        }

        // Try original 64-byte format
        if data.len() < 64 {
            return Err(anyhow!(
                "Board account data unexpected size: {} bytes (expected 32, 33, or 64+)",
                data.len()
            ));
        }

        // Skip 8-byte discriminator, then parse fields:
        // [8..16]: round_id (u64)
        // [16..24]: start_slot (u64)
        // [24..32]: end_slot (u64)
        // [32..64]: entropy_var (Pubkey, 32 bytes)
        let round_id = u64::from_le_bytes(data[8..16].try_into()?);
        let start_slot = u64::from_le_bytes(data[16..24].try_into()?);
        let end_slot = u64::from_le_bytes(data[24..32].try_into()?);
        let entropy_var = Pubkey::try_from(&data[32..64])?;

        Ok(BoardUpdate {
            round_id,
            start_slot,
            end_slot,
            entropy_var,
        })
    }
}

/// Spawn Board WebSocket subscriber task
pub fn spawn_board_subscriber(ws_url: String) -> Result<broadcast::Receiver<BoardUpdate>> {
    let subscriber = BoardWebSocketSubscriber::new(ws_url)?;

    // Create broadcast channel (capacity 16 for buffering)
    let (tx, rx) = broadcast::channel(16);

    // Spawn background task
    tokio::spawn(async move {
        if let Err(e) = subscriber.subscribe(tx).await {
            error!("❌ Board WebSocket subscriber failed: {}", e);
        }
    });

    info!("🚀 Board WebSocket subscriber task spawned");

    Ok(rx)
}

/// WebSocket subscriber for Round account
pub struct RoundWebSocketSubscriber {
    ws_url: String,
    round_pda: Pubkey,
}

impl RoundWebSocketSubscriber {
    pub fn new(ws_url: String, round_id: u64) -> Result<Self> {
        let ore_program = ORE_PROGRAM_ID.parse::<Pubkey>()?;
        let round_id_bytes = round_id.to_le_bytes();
        let (round_pda, _) = Pubkey::find_program_address(&[ROUND, &round_id_bytes], &ore_program);

        info!("📡 Round WebSocket subscriber initialized");
        info!("   Round ID: {}", round_id);
        info!("   Round PDA: {}", round_pda);
        info!("   WebSocket URL: {}", ws_url);

        Ok(Self { ws_url, round_pda })
    }

    /// Subscribe to Round account updates and send to broadcast channel
    pub async fn subscribe(&self, tx: broadcast::Sender<RoundUpdate>) -> Result<()> {
        loop {
            match self.subscribe_inner(tx.clone()).await {
                Ok(_) => {
                    warn!("⚠️  Round WebSocket subscription ended normally, reconnecting...");
                }
                Err(e) => {
                    error!("❌ Round WebSocket error: {} - reconnecting in 5s", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn subscribe_inner(&self, tx: broadcast::Sender<RoundUpdate>) -> Result<()> {
        info!("🔌 Connecting to Round WebSocket: {}", self.ws_url);

        let pubsub = PubsubClient::new(&self.ws_url)
            .await
            .map_err(|e| anyhow!("Failed to connect to WebSocket: {}", e))?;

        info!("✅ Round WebSocket connected");

        let config = RpcAccountInfoConfig {
            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            data_slice: None,
            min_context_slot: None,
        };

        let (mut stream, _unsub) = pubsub
            .account_subscribe(&self.round_pda, Some(config))
            .await
            .map_err(|e| anyhow!("Failed to subscribe to Round account: {}", e))?;

        info!("✅ Subscribed to Round PDA account updates");

        while let Some(response) = stream.next().await {
            use base64::{engine::general_purpose, Engine as _};
            use solana_account_decoder::UiAccountData;

            let bytes: Vec<u8> = match &response.value.data {
                UiAccountData::Binary(encoded_data, _)
                | UiAccountData::LegacyBinary(encoded_data) => {
                    debug!(
                        "📦 Round data: len={}, preview={}",
                        encoded_data.len(),
                        &encoded_data[..encoded_data.len().min(100)]
                    );

                    match general_purpose::STANDARD.decode(encoded_data) {
                        Ok(data) => {
                            debug!("✅ Round data decoded: {} bytes", data.len());
                            data
                        }
                        Err(e) => {
                            warn!(
                                "⚠️  Round WS: base64 decode FAILED: {} (len={}, preview={})",
                                e,
                                encoded_data.len(),
                                &encoded_data[..encoded_data.len().min(50)]
                            );
                            continue;
                        }
                    }
                }
                UiAccountData::Json(_) => {
                    warn!("⚠️  Round WS: Received JSON, expected binary");
                    continue;
                }
            };

            match self.parse_round_update(&bytes) {
                Ok(update) => {
                    debug!(
                        "📊 Round update: id {}, pot={:.6} SOL, {}/25 cells claimed",
                        update.id,
                        update.total_deployed as f64 / 1e9,
                        update.deployed.iter().filter(|&&x| x > 0).count()
                    );
                    let _ = tx.send(update);
                }
                Err(e) => {
                    warn!("⚠️  Failed to parse Round update: {}", e);
                }
            }
        }

        Ok(())
    }

    fn parse_round_update(&self, data: &[u8]) -> Result<RoundUpdate> {
        // Parse Round account structure (from ore_rpc.rs lines 149-204):
        // 8 bytes: discriminator
        // 8 bytes: id
        // 200 bytes: deployed[25] (25 * 8)
        // 32 bytes: slot_hash
        // 200 bytes: count[25] (25 * 8)
        // ... then motherlode, rent_payer, top_miner, rewards, totals

        if data.len() < 8 + 8 + 200 + 32 + 200 + 8 + 8 + 32 + 32 + 8 + 8 + 8 + 8 {
            return Err(anyhow!(
                "Round account data too small: {} bytes",
                data.len()
            ));
        }

        let mut offset = 8; // Skip discriminator

        // Parse id
        let id = u64::from_le_bytes(data[offset..offset + 8].try_into()?);
        offset += 8;

        // Parse deployed array (25 u64s)
        let mut deployed = [0u64; 25];
        for i in 0..25 {
            deployed[i] = u64::from_le_bytes(data[offset..offset + 8].try_into()?);
            offset += 8;
        }
        offset += 32; // Skip slot_hash

        // Parse count array (25 u64s)
        let mut count = [0u64; 25];
        for i in 0..25 {
            count[i] = u64::from_le_bytes(data[offset..offset + 8].try_into()?);
            offset += 8;
        }

        // Skip expires_at, motherlode, rent_payer, top_miner, top_miner_reward
        offset += 8 + 8 + 32 + 32 + 8;

        // Parse total_deployed
        let total_deployed = u64::from_le_bytes(data[offset..offset + 8].try_into()?);
        offset += 8;
        offset += 8; // Skip total_vaulted

        // Parse total_winnings
        let total_winnings = u64::from_le_bytes(data[offset..offset + 8].try_into()?);

        Ok(RoundUpdate {
            id,
            deployed,
            count,
            total_deployed,
            total_winnings,
        })
    }
}

/// Spawn Round WebSocket subscriber task
pub fn spawn_round_subscriber(
    ws_url: String,
    round_id: u64,
) -> Result<broadcast::Receiver<RoundUpdate>> {
    let subscriber = RoundWebSocketSubscriber::new(ws_url, round_id)?;

    let (tx, rx) = broadcast::channel(16);

    tokio::spawn(async move {
        if let Err(e) = subscriber.subscribe(tx).await {
            error!("❌ Round WebSocket subscriber failed: {}", e);
        }
    });

    info!("🚀 Round WebSocket subscriber task spawned");

    Ok(rx)
}

/// WebSocket subscriber for Treasury account (Motherlode jackpot)
pub struct TreasuryWebSocketSubscriber {
    ws_url: String,
    treasury_pda: Pubkey,
}

impl TreasuryWebSocketSubscriber {
    pub fn new(ws_url: String) -> Result<Self> {
        let ore_program = ORE_PROGRAM_ID.parse::<Pubkey>()?;
        let (treasury_pda, _) = Pubkey::find_program_address(&[TREASURY], &ore_program);

        info!("📡 Treasury WebSocket subscriber initialized");
        info!("   Treasury PDA: {}", treasury_pda);
        info!("   WebSocket URL: {}", ws_url);

        Ok(Self {
            ws_url,
            treasury_pda,
        })
    }

    /// Subscribe to Treasury account updates and send to broadcast channel
    pub async fn subscribe(&self, tx: broadcast::Sender<TreasuryUpdate>) -> Result<()> {
        loop {
            match self.subscribe_inner(tx.clone()).await {
                Ok(_) => {
                    warn!("⚠️  Treasury WebSocket subscription ended normally, reconnecting...");
                }
                Err(e) => {
                    error!("❌ Treasury WebSocket error: {} - reconnecting in 5s", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn subscribe_inner(&self, tx: broadcast::Sender<TreasuryUpdate>) -> Result<()> {
        info!("🔌 Connecting to Treasury WebSocket: {}", self.ws_url);

        let pubsub = PubsubClient::new(&self.ws_url)
            .await
            .map_err(|e| anyhow!("Failed to connect to WebSocket: {}", e))?;

        info!("✅ Treasury WebSocket connected");

        let config = RpcAccountInfoConfig {
            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            data_slice: None,
            min_context_slot: None,
        };

        let (mut stream, _unsub) = pubsub
            .account_subscribe(&self.treasury_pda, Some(config))
            .await
            .map_err(|e| anyhow!("Failed to subscribe to Treasury account: {}", e))?;

        info!("✅ Subscribed to Treasury PDA account updates");

        while let Some(response) = stream.next().await {
            use base64::{engine::general_purpose, Engine as _};
            use solana_account_decoder::UiAccountData;

            let bytes: Vec<u8> = match &response.value.data {
                UiAccountData::Binary(encoded_data, _)
                | UiAccountData::LegacyBinary(encoded_data) => {
                    debug!(
                        "📦 Treasury data: len={}, preview={}",
                        encoded_data.len(),
                        &encoded_data[..encoded_data.len().min(100)]
                    );

                    match general_purpose::STANDARD.decode(encoded_data) {
                        Ok(data) => {
                            debug!("✅ Treasury data decoded: {} bytes", data.len());
                            data
                        }
                        Err(e) => {
                            warn!(
                                "⚠️  Treasury WS: base64 decode FAILED: {} (len={}, preview={})",
                                e,
                                encoded_data.len(),
                                &encoded_data[..encoded_data.len().min(50)]
                            );
                            continue;
                        }
                    }
                }
                UiAccountData::Json(_) => {
                    warn!("⚠️  Treasury WS: Received JSON, expected binary");
                    continue;
                }
            };

            match self.parse_treasury_update(&bytes) {
                Ok(update) => {
                    debug!(
                        "💎 Treasury update: Motherlode={:.2} ORE, Total minted={:.2} ORE",
                        update.motherlode_balance as f64 / 1e11,
                        update.total_minted as f64 / 1e11
                    );
                    let _ = tx.send(update);
                }
                Err(e) => {
                    warn!("⚠️  Failed to parse Treasury update: {}", e);
                }
            }
        }

        Ok(())
    }

    fn parse_treasury_update(&self, data: &[u8]) -> Result<TreasuryUpdate> {
        // Parse Treasury account (from ore_rpc.rs lines 283-303):
        // [0-8]: discriminator
        // [8-16]: unknown field
        // [16-24]: motherlode_balance (ORE has 11 decimals, not 9!)
        // [24-32]: total_minted

        if data.len() < 32 {
            return Err(anyhow!(
                "Treasury account data too small: {} bytes",
                data.len()
            ));
        }

        let motherlode_balance = u64::from_le_bytes(data[16..24].try_into()?);
        let total_minted = u64::from_le_bytes(data[24..32].try_into()?);

        Ok(TreasuryUpdate {
            motherlode_balance,
            total_minted,
        })
    }
}

/// Spawn Treasury WebSocket subscriber task
pub fn spawn_treasury_subscriber(ws_url: String) -> Result<broadcast::Receiver<TreasuryUpdate>> {
    let subscriber = TreasuryWebSocketSubscriber::new(ws_url)?;

    let (tx, rx) = broadcast::channel(16);

    tokio::spawn(async move {
        if let Err(e) = subscriber.subscribe(tx).await {
            error!("❌ Treasury WebSocket subscriber failed: {}", e);
        }
    });

    info!("🚀 Treasury WebSocket subscriber task spawned");

    Ok(rx)
}
