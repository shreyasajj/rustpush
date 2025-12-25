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

**Option 1: Docker (Recommended)**

```bash
# 1. Prepare config directory
mkdir -p config
cp immich-config.example.plist config/immich-config.plist
# Edit config/immich-config.plist with your Immich server URL and API key

# 2. Authenticate with Apple (one-time setup)
cargo run --release --features macos-validation-data --bin rustpush-test
cp config.plist hwconfig.plist config/

# 3. Run with Docker Compose
docker-compose up -d

# 4. View logs
docker-compose logs -f immich-sync
```

**Option 2: Build from Source**

```bash
# 1. Build the binary
cargo build --release --features macos-validation-data --bin immich-sync

# 2. Configure
cp immich-config.example.plist immich-config.plist
# Edit immich-config.plist with your settings

# 3. Run
./target/release/immich-sync
```

**Documentation:**
- 📖 [Full Integration Guide](IMMICH_INTEGRATION.md)
- 🐳 [Docker Deployment Guide](DOCKER.md)