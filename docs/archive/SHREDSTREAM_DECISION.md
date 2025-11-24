# ShredStream Integration Decision

## Issue Summary

The direct ShredStream integration in the bot has a persistent issue: the stream connects successfully but receives NO data, while the standalone ShredStream service (localhost:8081) works perfectly and has processed 13,000+ events.

**Root Cause**: Unknown - subscription code is identical between bot and service, but spawned background task in bot doesn't receive any entries from the stream.

## Solution: Use ShredStream Service API

**Decision**: Switch to polling the localhost:8081 REST API instead of direct ShredStream integration.

**Advantages**:
1. ✅ **Proven to work** - service has been running flawlessly for hours
2. ✅ **Separation of concerns** - dedicated service handles connection management
3. ✅ **Easier debugging** - can monitor service independently  
4. ✅ **Reliability** - service auto-reconnects on failures
5. ✅ **Simplicity** - HTTP REST vs complex gRPC stream management

**Next Steps**:
1. Update bot to poll `http://localhost:8081/events` every 100-200ms
2. Parse BoardReset and Deploy events from API response
3. Remove direct ShredStream integration from bot
4. Keep service running via systemd or tmux

**Implementation**: Simple HTTP GET requests, <5ms latency for localhost API.

