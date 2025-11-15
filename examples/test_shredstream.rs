// Test ShredStream connection directly
use anyhow::Result;
use futures::StreamExt;
use solana_entry::entry::Entry;
use solana_stream_sdk::ShredstreamClient;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 Testing ShredStream Connection");
    println!("==================================\n");

    let endpoint = "https://shreds-ny6-1.erpc.global";
    println!("📡 Endpoint: {}", endpoint);

    // Connect to ShredStream
    println!("🔌 Connecting...");
    let mut client = ShredstreamClient::connect(endpoint).await?;
    println!("✅ Connected successfully!");

    // Subscribe to entries
    println!("📡 Subscribing to entries...");
    let request = ShredstreamClient::create_empty_entries_request();
    let mut stream = client.subscribe_entries(request).await?;
    println!("✅ Subscribed successfully!");

    // Try to receive first few entries
    println!("\n⏳ Waiting for entries (30 second timeout)...\n");

    let mut count = 0;
    let start = std::time::Instant::now();
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while let Some(result) = stream.next().await {
            match result {
                Ok(slot_entry) => {
                    let slot = slot_entry.slot;
                    let bytes = slot_entry.entries.len();

                    println!("📦 Entry {}: slot={}, bytes={}", count + 1, slot, bytes);

                    // Try to deserialize
                    match bincode::deserialize::<Vec<Entry>>(&slot_entry.entries) {
                        Ok(entries) => {
                            println!("   ✅ Deserialized {} entries", entries.len());

                            // Show first transaction details
                            for (i, entry) in entries.iter().take(1).enumerate() {
                                println!(
                                    "   Entry {}: {} transactions",
                                    i,
                                    entry.transactions.len()
                                );
                            }
                        }
                        Err(e) => {
                            println!("   ❌ Deserialize error: {}", e);
                        }
                    }

                    count += 1;
                    if count >= 5 {
                        println!("\n✅ Received {} entries successfully!", count);
                        break;
                    }
                }
                Err(e) => {
                    println!("❌ Stream error: {}", e);
                    break;
                }
            }
        }
    });

    match timeout.await {
        Ok(_) => {
            println!(
                "\n🎉 Test completed in {:.2}s",
                start.elapsed().as_secs_f64()
            );
            println!("   Total entries received: {}", count);

            if count > 0 {
                println!("\n✅ ShredStream is WORKING correctly!");
            } else {
                println!("\n⚠️  Stream closed without sending entries");
            }
        }
        Err(_) => {
            println!("\n⏱️  Timeout after 30 seconds");
            println!("   Entries received: {}", count);

            if count == 0 {
                println!("\n❌ No entries received - connection might be broken");
            }
        }
    }

    Ok(())
}
