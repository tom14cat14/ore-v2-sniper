// ore_shredstream.rs — ShredStream processor for Ore V2 lottery bot
// Monitors Ore program for Deploy and BoardReset events
//
// ARCHITECTURE: Direct stream processing (like MEV bot)
// - NO tokio::spawn (prevents 30s idle timeout issue)
// - NO broadcast channel (process stream directly)
// - process() polls stream synchronously and returns events

use anyhow::Result;
use futures::StreamExt;
use solana_entry::entry::Entry;
use solana_stream_sdk::ShredstreamClient;
use std::pin::Pin;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::ore_instructions::ORE_PROGRAM_ID;

/// Ore program events parsed from logs
#[derive(Debug, Clone)]
pub enum OreEvent {
    /// Board reset event with new reset slot
    BoardReset { slot: u64 },

    /// Cell deployed to (claimed)
    CellDeployed {
        cell_id: u8,
        authority: String,
        amount_lamports: u64,
    },

    /// Current slot update
    SlotUpdate { slot: u64 },
}

/// ShredStream processor for Ore V2 lottery
pub struct OreShredStreamProcessor {
    pub endpoint: String,
    client: Option<ShredstreamClient>,
    stream: Option<
        Pin<
            Box<
                dyn futures::Stream<
                        Item = Result<solana_stream_sdk::shredstream_proto::Entry, tonic::Status>,
                    > + Send,
            >,
        >,
    >,
    current_slot: u64,
    initialized: bool,
    entries_processed: u64,
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
            client: None,
            stream: None,
            current_slot: 0,
            initialized: false,
            entries_processed: 0,
        }
    }

    /// Initialize connection to ShredStream (called once)
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        info!("🔌 Initializing ShredStream for Ore V2 lottery monitoring");
        info!("📡 Target program: {}", ORE_PROGRAM_ID);
        info!("📡 Endpoint: {}", self.endpoint);

        // Connect to ShredStream
        info!("🔌 Connecting to ShredStream...");
        let mut client = ShredstreamClient::connect(&self.endpoint)
            .await
            .map_err(|e| anyhow::anyhow!("ShredStream connection failed: {}", e))?;
        info!("✅ ShredStream connection established");

        // Subscribe to ALL entries (no filtering - filter locally for Ore program)
        info!("📡 Subscribing to ShredStream entries...");
        let request = ShredstreamClient::create_empty_entries_request();
        let stream = client
            .subscribe_entries(request)
            .await
            .map_err(|e| anyhow::anyhow!("ShredStream subscribe failed: {}", e))?;
        info!("✅ Subscribed to ShredStream (will filter Ore V2 events locally)");

        // Store client and stream (Pin+Box for trait object)
        self.client = Some(client);
        self.stream = Some(Box::pin(stream));
        self.initialized = true;

        info!("✅ Ore ShredStream processor initialized (direct processing mode)");
        Ok(())
    }

    /// Process ShredStream data and extract Ore events
    /// NOTE: This polls the stream DIRECTLY (no spawn, no channel)
    /// Returns immediately with events from the next stream item
    pub async fn process(&mut self) -> Result<OreStreamEvent> {
        let start = Instant::now();

        // Initialize if not already done
        if !self.initialized {
            self.initialize().await?;
        }

        // Get stream reference
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("ShredStream not initialized"))?;

        // Poll stream for next entry (BLOCKING - waits for data)
        // Auto-reconnect on None (30s idle timeout from server)
        match tokio::time::timeout(std::time::Duration::from_secs(35), stream.next()).await {
            Ok(Some(slot_entry_result)) => {
                match slot_entry_result {
                    Ok(slot_entry) => {
                        let slot = slot_entry.slot;
                        self.current_slot = slot;

                        // Log first few entries to verify streaming
                        if self.entries_processed < 5 {
                            info!(
                                "📡 Received slot {} with {} bytes of entry data",
                                slot,
                                slot_entry.entries.len()
                            );
                        }

                        // Deserialize entries from binary data
                        match bincode::deserialize::<Vec<Entry>>(&slot_entry.entries) {
                            Ok(entries) => {
                                let entry_count = entries.len();
                                self.entries_processed += entry_count as u64;

                                // Log entry counts for first few or when non-empty
                                if entry_count > 0 || self.entries_processed < 10 {
                                    debug!(
                                        "📦 Slot {}: {} entries ({} total processed)",
                                        slot, entry_count, self.entries_processed
                                    );
                                }

                                // Parse entries for Ore events
                                let mut events = vec![OreEvent::SlotUpdate { slot }];

                                for entry in &entries {
                                    for tx in &entry.transactions {
                                        if let Some(mut ore_events) = self.parse_ore_transaction(tx)
                                        {
                                            // Fill in slot for BoardReset events
                                            for event in &mut ore_events {
                                                if let OreEvent::BoardReset { slot: event_slot } =
                                                    event
                                                {
                                                    *event_slot = slot;
                                                }
                                            }
                                            events.extend(ore_events);
                                        }
                                    }
                                }

                                let latency_us = start.elapsed().as_micros() as f64;

                                if events.len() > 1 {
                                    // More than just SlotUpdate
                                    debug!(
                                        "🎲 Ore events detected: {} events in slot {}",
                                        events.len() - 1,
                                        slot
                                    );
                                }

                                Ok(OreStreamEvent {
                                    events,
                                    latency_us,
                                    current_slot: slot,
                                })
                            }
                            Err(e) => {
                                warn!("⚠️ Failed to deserialize entries: {}", e);
                                // Return slot update even if deserialization fails
                                Ok(OreStreamEvent {
                                    events: vec![OreEvent::SlotUpdate { slot }],
                                    latency_us: start.elapsed().as_micros() as f64,
                                    current_slot: slot,
                                })
                            }
                        }
                    }
                    Err(e) => {
                        warn!("⚠️ ShredStream error: {}", e);
                        Err(anyhow::anyhow!("ShredStream error: {}", e))
                    }
                }
            }
            Ok(None) | Err(_) => {
                // Stream ended or timeout - auto-reconnect
                warn!(
                    "🔄 ShredStream timeout/disconnect after {} entries - reconnecting...",
                    self.entries_processed
                );

                // Reset state
                self.initialized = false;
                self.client = None;
                self.stream = None;

                // Reconnect automatically
                self.initialize().await?;

                // Return empty event after reconnect
                Ok(OreStreamEvent {
                    events: vec![],
                    latency_us: start.elapsed().as_micros() as f64,
                    current_slot: self.current_slot,
                })
            }
        }
    }

    /// Parse Ore program transaction for events
    fn parse_ore_transaction(
        &self,
        tx: &solana_sdk::transaction::VersionedTransaction,
    ) -> Option<Vec<OreEvent>> {
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
                            ix.data[1], ix.data[2], ix.data[3], ix.data[4], ix.data[5], ix.data[6],
                            ix.data[7], ix.data[8],
                        ]);

                        // Parse squares bitmask (little-endian u32)
                        let squares =
                            u32::from_le_bytes([ix.data[9], ix.data[10], ix.data[11], ix.data[12]]);

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

                        // CRITICAL FIX: amount_lamports is TOTAL for all cells, not per-cell!
                        // Must divide by number of cells to get amount per cell
                        let num_cells = squares.count_ones() as u64;
                        let amount_per_cell = if num_cells > 0 {
                            amount_lamports / num_cells
                        } else {
                            0
                        };

                        // Log all cells in the bitmask
                        for cell_id in 0..32 {
                            if (squares & (1 << cell_id)) != 0 {
                                debug!("🎲 Detected Deploy: cell_id={}, amount_per_cell={:.6} SOL ({} cells, total={:.6} SOL), authority={}",
                                       cell_id, amount_per_cell as f64 / 1e9, num_cells,
                                       amount_lamports as f64 / 1e9, &authority[..8]);
                                events.push(OreEvent::CellDeployed {
                                    cell_id: cell_id as u8,
                                    authority: authority.clone(),
                                    amount_lamports: amount_per_cell,  // Per-cell amount, not total!
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
    pub fn get_current_slot(&self) -> u64 {
        self.current_slot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_processor_creation() {
        let processor =
            OreShredStreamProcessor::new("https://shredstream.rpcpool.com:443".to_string());

        assert!(!processor.initialized);
        assert_eq!(processor.get_current_slot(), 0);
    }
}
