# Rustpush

Rustpush is a portable library for interplatform communication.

## Features

- iMessage protocol implementation
- Apple Shared Albums (Shared Streams)
- FaceTime support
- Find My network integration
- CloudKit integration
- **NEW: Immich integration for Apple Shared Albums**

## Immich Integration

Sync your Apple Shared Albums with [Immich](https://github.com/immich-app/immich), a self-hosted photo management solution!

Features:
- ✅ Auto-approve shared album invitations
- 📥 Sync photos from Apple Shared Albums to Immich
- 📤 Sync photos from Immich to Apple Shared Albums (in development)
- 🔄 Continuous bidirectional synchronization
- 📝 Preserve album names and metadata

**[View full Immich Integration documentation →](IMMICH_INTEGRATION.md)**

### Quick Start

1. Build the immich-sync binary:
   ```bash
   cargo build --release --features macos-validation-data --bin immich-sync
   ```

2. Copy and configure the example config:
   ```bash
   cp immich-config.example.plist immich-config.plist
   # Edit immich-config.plist with your Immich server URL and API key
   ```

3. Run the sync service:
   ```bash
   ./target/release/immich-sync
   ```

See [IMMICH_INTEGRATION.md](IMMICH_INTEGRATION.md) for detailed setup instructions.