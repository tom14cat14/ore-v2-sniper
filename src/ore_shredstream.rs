// ore_shredstream.rs — ShredStream processor for Ore V2 lottery bot
// Monitors Ore program for Deploy and BoardReset events

use anyhow::Result;
use futures::StreamExt;
use solana_entry::entry::Entry;
use solana_stream_sdk::ShredstreamClient;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn, debug};

use crate::ore_instructions::ORE_PROGRAM_ID;

/// Ore program events parsed from logs
#[derive(Debug, Clone)]
pub enum OreEvent {
    /// Board reset event with new reset slot
    BoardReset { slot: u64 },

    /// Cell deployed to (claimed)
    CellDeployed { cell_id: u8, authority: String, amount_lamports: u64 },

    /// Current slot update
    SlotUpdate { slot: u64 },
}

/// ShredStream processor for Ore V2 lottery
pub struct OreShredStreamProcessor {
    pub endpoint: String,
    event_rx: Option<broadcast::Receiver<(u64, Vec<Entry>)>>,
    current_slot: Arc<RwLock<u64>>,
    initialized: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OreStreamEvent {
    pub events: Vec<OreEvent>,
    pub latency_us: f64,
    pub current_slot: u64,
}

impl OreShredStreamProcessor {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            event_rx: None,
            current_slot: Arc::new(RwLock::new(0)),
            initialized: false,
        }
    }

    /// Initialize persistent gRPC-over-HTTPS connection to ShredStream
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        info!("🔌 Initializing ShredStream for Ore V2 lottery monitoring");
        info!("📡 Target program: {}", ORE_PROGRAM_ID);

        // Connect to ShredStream via gRPC-over-HTTPS
        let mut client = ShredstreamClient::connect(&self.endpoint).await
            .map_err(|e| anyhow::anyhow!("ShredStream connection failed: {}", e))?;

        info!("✅ ShredStream connection established");

        info!("📡 Subscribing to ShredStream for all entries (filter Ore events locally)");

        // Subscribe to ALL entries (no filtering - per ShredStream Service working implementation)
        // Account-based filtering appears unreliable with ERPC ShredStream
        // We filter for Ore program transactions in parse_ore_transaction() instead
        let request = ShredstreamClient::create_empty_entries_request();

        let mut stream = client.subscribe_entries(request).await?;
        info!("📡 Subscribed to ShredStream (will filter Ore V2 events locally)");

        // Create broadcast channel for fan-out to multiple consumers (snipe + auto-claim)
        // Capacity 100 = buffer 100 slot updates before dropping old ones
        let (tx, rx) = broadcast::channel(100);
        self.event_rx = Some(rx);

        let current_slot = self.current_slot.clone();

        // CRITICAL: Spawn task to actively poll the stream
        // Streams are LAZY and require active consumption via tokio::spawn
        // This is the fix - don't wait for first message, just spawn and poll!
        tokio::spawn(async move {
            let mut entries_processed = 0u64;
            info!("🚀 Background Ore ShredStream processor started (actively polling)");

            loop {
                match stream.next().await {
                    Some(slot_entry_result) => {
                        match slot_entry_result {
                            Ok(slot_entry) => {
                                let slot = slot_entry.slot;

                                // Log first few slot updates to verify stream is working
                                if entries_processed < 5 {
                                    info!("📡 Received slot {} with {} bytes of entry data",
                                          slot, slot_entry.entries.len());
                                }

                                // Update current slot
                                {
                                    let mut current = current_slot.write().await;
                                    *current = slot;
                                }

                                // Deserialize entries from binary data
                                match bincode::deserialize::<Vec<Entry>>(&slot_entry.entries) {
                                    Ok(entries) => {
                                        let entry_count = entries.len();
                                        entries_processed += entry_count as u64;

                                        // Log entry counts
                                        if entry_count > 0 || entries_processed < 10 {
                                            info!("📦 Slot {}: {} entries ({} total processed)",
                                                  slot, entry_count, entries_processed);
                                        }

                                        // Broadcast to all receivers (ignore send errors - no receivers or channel full)
                                        let _ = tx.send((slot, entries));
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Failed to deserialize entries: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("⚠️ ShredStream error: {}", e);
                            }
                        }
                    }
                    None => {
                        warn!("🛑 ShredStream ended: stream returned None after {} entries", entries_processed);
                        break;
                    }
                }
            }
        });

        self.initialized = true;
        info!("✅ Ore ShredStream processor initialized with broadcast channel");
        Ok(())
    }

    /// Process ShredStream data and extract Ore events
    pub async fn process(&mut self) -> Result<OreStreamEvent> {
        let start = Instant::now();

        // Initialize if not already done
        if !self.initialized {
            self.initialize().await?;
        }

        // Get current slot
        let current_slot = *self.current_slot.read().await;

        // Try to receive from broadcast channel (non-blocking)
        if let Some(ref mut rx) = self.event_rx {
            match rx.try_recv() {
                Ok((slot, entries)) => {
                    let mut events = Vec::new();

                    // Add slot update event
                    events.push(OreEvent::SlotUpdate { slot });

                    // Parse entries for Ore program events
                    for entry in &entries {
                        for tx in &entry.transactions {
                            // Extract Ore events from transaction logs
                            if let Some(mut ore_events) = self.parse_ore_transaction(tx) {
                                // Fill in slot for BoardReset events
                                for event in &mut ore_events {
                                    if let OreEvent::BoardReset { slot: event_slot } = event {
                                        *event_slot = slot;
                                    }
                                }
                                events.extend(ore_events);
                            }
                        }
                    }

                    let latency_us = start.elapsed().as_micros() as f64;

                    if !events.is_empty() {
                        debug!("🎲 Ore events detected: {} events in slot {}", events.len(), slot);
                    }

                    Ok(OreStreamEvent {
                        events,
                        latency_us,
                        current_slot,
                    })
                }
                Err(broadcast::error::TryRecvError::Empty) => {
                    // No new data available (normal case)
                    Ok(OreStreamEvent {
                        events: vec![],
                        latency_us: start.elapsed().as_micros() as f64,
                        current_slot,
                    })
                }
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    warn!("⚠️ ShredStream lagged by {} messages - processing too slow!", n);
                    // Channel is lagging but still active, return empty and let next poll catch up
                    Ok(OreStreamEvent {
                        events: vec![],
                        latency_us: start.elapsed().as_micros() as f64,
                        current_slot,
                    })
                }
                Err(broadcast::error::TryRecvError::Closed) => {
                    Err(anyhow::anyhow!("ShredStream channel closed - stream disconnected"))
                }
            }
        } else {
            Err(anyhow::anyhow!("ShredStream not initialized"))
        }
    }

    /// Parse Ore program transaction for events
    fn parse_ore_transaction(&self, tx: &solana_sdk::transaction::VersionedTransaction) -> Option<Vec<OreEvent>> {
        use solana_sdk::message::VersionedMessage;

        let mut events = Vec::new();

        // Get account keys from the transaction
        let account_keys = match tx.message {
            VersionedMessage::Legacy(ref msg) => &msg.account_keys,
            VersionedMessage::V0(ref msg) => &msg.account_keys,
        };

        // Check if any account is the Ore program
        let ore_program_pubkey: solana_sdk::pubkey::Pubkey = ORE_PROGRAM_ID.parse().ok()?;
        let has_ore_program = account_keys.iter().any(|key| key == &ore_program_pubkey);

        if !has_ore_program {
            return None;
        }

        // Get instructions
        let instructions = match &tx.message {
            VersionedMessage::Legacy(ref msg) => &msg.instructions,
            VersionedMessage::V0(ref msg) => &msg.instructions,
        };

        // Parse each instruction
        for ix in instructions {
            let program_id_index = ix.program_id_index as usize;
            if program_id_index >= account_keys.len() {
                continue;
            }

            let program_id = &account_keys[program_id_index];
            if program_id != &ore_program_pubkey {
                continue;
            }

            // Parse instruction data
            if ix.data.is_empty() {
                continue;
            }

            // First byte is the instruction discriminator
            let discriminator = ix.data[0];

            // Ore V2 instruction discriminators (from regolith-labs/ore):
            // Automate = 0, Checkpoint = 2, ClaimSOL = 3, ClaimORE = 4,
            // Close = 5, Deploy = 6, Log = 8, Reset = 9

            match discriminator {
                // Deploy instruction - cell claim
                6 => {
                    // Deploy has: amount (8 bytes) + squares (4 bytes)
                    // squares contains the cell IDs as a 32-bit bitmask
                    if ix.data.len() >= 13 {
                        // Parse amount (bytes 1-8, little-endian u64)
                        let amount_lamports = u64::from_le_bytes([
                            ix.data[1], ix.data[2], ix.data[3], ix.data[4],
                            ix.data[5], ix.data[6], ix.data[7], ix.data[8]
                        ]);

                        // Parse squares bitmask (little-endian u32)
                        let squares = u32::from_le_bytes([
                            ix.data[9], ix.data[10], ix.data[11], ix.data[12]
                        ]);

                        // Get authority from accounts (usually first signer)
                        let authority = if !ix.accounts.is_empty() {
                            let auth_index = ix.accounts[0] as usize;
                            if auth_index < account_keys.len() {
                                account_keys[auth_index].to_string()
                            } else {
                                "unknown".to_string()
                            }
                        } else {
                            "unknown".to_string()
                        };

                        // Log all cells in the bitmask (proportional ownership - track amounts!)
                        for cell_id in 0..32 {
                            if (squares & (1 << cell_id)) != 0 {
                                debug!("🎲 Detected Deploy: cell_id={}, amount={:.6} SOL, authority={}",
                                       cell_id, amount_lamports as f64 / 1e9, &authority[..8]);
                                events.push(OreEvent::CellDeployed {
                                    cell_id: cell_id as u8,
                                    authority: authority.clone(),
                                    amount_lamports
                                });
                            }
                        }
                    }
                }
                // Reset instruction - BoardReset
                9 => {
                    debug!("🔄 Detected BoardReset event");
                    // Reset doesn't have additional data
                    // We'll use the current slot as the reset slot
                    events.push(OreEvent::BoardReset { slot: 0 }); // Slot will be filled by caller
                }
                // Checkpoint instruction - claim rewards (also important)
                2 => {
                    debug!("💰 Detected Checkpoint instruction (reward claim)");
                }
                _ => {
                    // Log other instructions for debugging
                    if discriminator <= 20 {
                        debug!("❓ Other Ore instruction: discriminator={}", discriminator);
                    }
                }
            }
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }

    /// Get current slot (real-time from ShredStream)
    pub async fn get_current_slot(&self) -> u64 {
        *self.current_slot.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_processor_creation() {
        let processor = OreShredStreamProcessor::new(
            "https://shredstream.rpcpool.com:443".to_string()
        );

        assert!(!processor.initialized);
        assert_eq!(processor.get_current_slot().await, 0);
    }
}
