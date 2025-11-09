//! Ore ShredStream Service - Dedicated event monitoring for Ore V2 lottery
//!
//! Monitors Ore program via ShredStream and provides REST API for events

use anyhow::Result;
use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use solana_entry::entry::Entry;
use solana_stream_sdk::{CommitmentLevel, ShredstreamClient};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

const ORE_PROGRAM_ID: &str = "oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv";

/// Ore event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OreEvent {
    BoardReset { slot: u64, timestamp: String },
    Deploy { cell_id: u8, authority: String, timestamp: String },
    SlotUpdate { slot: u64, timestamp: String },
}

/// Response for /events endpoint
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    pub events: Vec<OreEvent>,
    pub current_slot: u64,
    pub total_events: usize,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub current_slot: u64,
    pub events_processed: u64,
}

/// Shared application state
#[derive(Clone)]
struct AppState {
    /// Recent events (last 100)
    events: Arc<DashMap<u64, OreEvent>>,
    /// Current slot
    current_slot: Arc<parking_lot::Mutex<u64>>,
    /// Total events processed
    events_processed: Arc<parking_lot::Mutex<u64>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            current_slot: Arc::new(parking_lot::Mutex::new(0)),
            events_processed: Arc::new(parking_lot::Mutex::new(0)),
        }
    }

    fn add_event(&self, event: OreEvent) {
        let mut count = self.events_processed.lock();
        *count += 1;
        let event_id = *count;

        self.events.insert(event_id, event);

        // Keep only last 100 events
        if self.events.len() > 100 {
            if let Some(oldest) = self.events.iter().map(|e| *e.key()).min() {
                self.events.remove(&oldest);
            }
        }
    }

    fn update_slot(&self, slot: u64) {
        let mut current = self.current_slot.lock();
        *current = slot;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    info!("🎲 Ore ShredStream Service v0.1.0");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Create shared state
    let state = AppState::new();

    // Start ShredStream processor
    let processor_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_shredstream_processor(processor_state).await {
            error!("❌ ShredStream processor error: {}", e);
        }
    });

    // Start REST API server
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/events", get(events_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "127.0.0.1:8081";
    info!("🚀 Starting REST API server on http://{}", addr);
    info!("   GET /health  - Service health check");
    info!("   GET /events  - Recent Ore events");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// ShredStream processor - monitors Ore program
async fn run_shredstream_processor(state: AppState) -> Result<()> {
    info!("📡 Connecting to ShredStream...");

    // Use ERPC ShredStream endpoint (global access)
    let endpoint = "https://shreds-ny6-1.erpc.global";

    let mut client = ShredstreamClient::connect(endpoint).await
        .map_err(|e| anyhow::anyhow!("ShredStream connection failed: {}", e))?;

    info!("✅ ShredStream connected");
    info!("🎯 Subscribing to Ore program: {}", ORE_PROGRAM_ID);

    // Subscribe to Ore program only
    let request = ShredstreamClient::create_entries_request_for_accounts(
        vec![ORE_PROGRAM_ID.to_string()],
        vec![],
        vec![],
        Some(CommitmentLevel::Processed),
    );

    let mut stream = client.subscribe_entries(request).await?;
    info!("✅ Subscribed to Ore V2 events");

    let mut entries_processed = 0u64;

    while let Some(slot_entry_result) = stream.next().await {
        match slot_entry_result {
            Ok(slot_entry) => {
                let slot = slot_entry.slot;
                state.update_slot(slot);

                // Add slot update event
                state.add_event(OreEvent::SlotUpdate {
                    slot,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });

                // Deserialize entries
                match bincode::deserialize::<Vec<Entry>>(&slot_entry.entries) {
                    Ok(entries) => {
                        entries_processed += entries.len() as u64;

                        // Parse entries for Ore events
                        for entry in entries {
                            for tx in &entry.transactions {
                                // TODO: Parse transaction logs for BoardReset and Deploy events
                                // For now, just count transactions
                            }
                        }

                        if entries_processed % 100 == 0 {
                            info!("📦 Processed {} entries (slot {})", entries_processed, slot);
                        }
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to deserialize entries: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ ShredStream error: {}", e);
                // Retry connection after error
                sleep(Duration::from_secs(1)).await;
            }
        }
    }

    warn!("🛑 ShredStream stream ended - reconnecting...");
    Ok(())
}

/// Health check endpoint
async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let current_slot = *state.current_slot.lock();
    let events_processed = *state.events_processed.lock();

    Json(HealthResponse {
        status: "ok".to_string(),
        service: "ore-shredstream-service".to_string(),
        current_slot,
        events_processed,
    })
}

/// Events endpoint - returns recent Ore events
async fn events_handler(State(state): State<AppState>) -> Json<EventsResponse> {
    let current_slot = *state.current_slot.lock();

    let mut events: Vec<OreEvent> = state
        .events
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    // Sort by timestamp (newest first)
    events.sort_by(|a, b| {
        let time_a = match a {
            OreEvent::BoardReset { timestamp, .. } => timestamp,
            OreEvent::Deploy { timestamp, .. } => timestamp,
            OreEvent::SlotUpdate { timestamp, .. } => timestamp,
        };
        let time_b = match b {
            OreEvent::BoardReset { timestamp, .. } => timestamp,
            OreEvent::Deploy { timestamp, .. } => timestamp,
            OreEvent::SlotUpdate { timestamp, .. } => timestamp,
        };
        time_b.cmp(time_a)
    });

    Json(EventsResponse {
        total_events: events.len(),
        current_slot,
        events,
    })
}
