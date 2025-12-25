# Immich Integration for Apple Shared Albums

This integration enables bidirectional synchronization between Apple Shared Albums and [Immich](https://github.com/immich-app/immich), a self-hosted photo and video management solution.

## Features

- ✅ **Auto-approve shared album invitations** - Automatically accept new shared album invites
- 📥 **Apple → Immich sync** - Download photos from Apple Shared Albums to Immich
- 📤 **Immich → Apple sync** - Upload photos from Immich albums to Apple Shared Albums (in development)
- 🔔 **Push notification approval** - Get notified when new albums need approval (optional)
- 🔄 **Continuous sync** - Runs as a daemon, continuously syncing new photos
- 📝 **Album metadata** - Preserves album names and photo metadata

## Prerequisites

1. **Immich Server** - A running Immich instance
2. **Immich API Key** - Generate from Immich settings with these permissions:
   - `asset.read`
   - `asset.upload`
   - `album.read`
   - `album.write`
3. **Apple ID** - Already authenticated with rustpush (run `rustpush-test` first)
4. **macOS Validation Data** - Required for Apple authentication

## Installation

### Option 1: Docker (Recommended)

The easiest way to run the sync service is using Docker:

```bash
# See full Docker documentation
```

**[📦 Docker Deployment Guide →](DOCKER.md)**

### Option 2: Build from Source

Build the immich-sync binary:

```bash
cargo build --release --features macos-validation-data --bin immich-sync
```

## Configuration

### 1. Authenticate with Apple

First, run the main rustpush test binary to authenticate with your Apple ID:

```bash
cargo run --release --features macos-validation-data --bin rustpush-test
```

This will create `config.plist` and `hwconfig.plist` with your authentication data.

### 2. Create Immich Configuration

Create `immich-config.plist` in the project root:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>immich_server_url</key>
    <string>http://localhost:2283</string>

    <key>immich_api_key</key>
    <string>YOUR_API_KEY_HERE</string>

    <key>auto_approve_shares</key>
    <true/>

    <key>push_notification_approval</key>
    <true/>

    <key>sync_interval_secs</key>
    <integer>60</integer>

    <key>temp_dir</key>
    <string>/tmp/rustpush-immich-sync</string>

    <key>device_id</key>
    <string>rustpush-sync-device</string>
</dict>
</plist>
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `immich_server_url` | String | `http://localhost:2283` | Base URL of your Immich server |
| `immich_api_key` | String | (required) | Immich API key with proper permissions |
| `auto_approve_shares` | Boolean | `false` | Automatically approve new album invitations |
| `push_notification_approval` | Boolean | `true` | Send push notifications for manual approval |
| `sync_interval_secs` | Integer | `60` | Sync interval in seconds |
| `temp_dir` | String | `/tmp/rustpush-immich-sync` | Temporary directory for file downloads |
| `device_id` | String | (auto-generated) | Device ID for Immich uploads |

## Usage

### Running the Sync Service

```bash
cargo run --release --features macos-validation-data --bin immich-sync
```

Or if you've built the binary:

```bash
./target/release/immich-sync
```

The service will:

1. Connect to Apple Shared Albums
2. Check for new album invitations
3. Auto-approve (if enabled) or notify you
4. Sync photos from Apple → Immich
5. Sync photos from Immich → Apple (when implemented)
6. Repeat at the configured interval

### Running as a Service (systemd)

Create `/etc/systemd/system/immich-sync.service`:

```ini
[Unit]
Description=Immich Sync Service for Apple Shared Albums
After=network.target

[Service]
Type=simple
User=your-username
WorkingDirectory=/path/to/rustpush
ExecStart=/path/to/rustpush/target/release/immich-sync
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Then enable and start:

```bash
sudo systemctl enable immich-sync
sudo systemctl start immich-sync
sudo systemctl status immich-sync
```

## How It Works

### Sync Flow

1. **Discovery**: Service polls Apple for album changes
2. **New Albums**: When a new shared album invitation is detected:
   - If `auto_approve_shares=true`: Automatically subscribe
   - If `auto_approve_shares=false`: Wait for manual approval
3. **Album Mapping**: Creates corresponding album in Immich (or finds existing)
4. **Asset Sync**:
   - Downloads new photos from Apple
   - Uploads to Immich with original timestamps
   - Adds to the corresponding album
5. **State Tracking**: Maintains mapping between Apple and Immich assets

### Album Naming

Albums in Immich are created with the same name as the Apple Shared Album. The service will:
- Use the album's display name if available
- Fall back to the owner's full name
- Create a unique identifier if neither is available

### Metadata Preservation

The following metadata is preserved during sync:
- Original filename
- Creation date
- Modification date
- Asset GUID (stored as device asset ID)

## API Integration

### Immich API Endpoints Used

- `GET /api/albums` - List all albums
- `GET /api/albums/{id}` - Get album details
- `POST /api/albums` - Create new album
- `POST /api/assets` - Upload asset (photo/video)
- `PUT /api/albums/{id}/assets` - Add assets to album
- `GET /api/assets/{id}/original` - Download asset

### Apple Shared Streams Protocol

The integration uses rustpush's `SharedStreamClient` which implements:
- Album subscription/unsubscription
- Asset enumeration and download
- Change polling via `getchanges` API
- MMCS (MobileMe Cloud Storage) for file transfer

## Troubleshooting

### "Immich API key is not set"

Make sure you've created `immich-config.plist` with a valid API key from your Immich instance.

### "No saved state found"

Run `rustpush-test` first to authenticate with Apple ID and create the authentication state.

### "Failed to upload asset"

Check:
- Immich server is accessible
- API key has `asset.upload` permission
- Sufficient storage space in Immich
- File format is supported by Immich

### Photos not syncing

Enable debug logging:

```bash
RUST_LOG=debug ./target/release/immich-sync
```

Check for errors in the logs related to:
- Apple authentication
- Album discovery
- File downloads
- Immich uploads

## Limitations

- **Immich → Apple sync**: Currently logs a warning but doesn't upload (under development)
- **Push notifications**: Currently logged but not fully implemented
- **Delete sync**: Deletions are not synchronized between platforms
- **Live Photos**: Treated as separate photo and video files

## Future Enhancements

- [ ] Complete Immich → Apple upload functionality
- [ ] Implement push notification system for approvals
- [ ] Support for Live Photos
- [ ] Bi-directional delete synchronization
- [ ] Album sharing/collaboration sync
- [ ] Web UI for approval management
- [ ] Support for multiple Immich instances
- [ ] Selective album sync (whitelist/blacklist)

## References

- [Immich API Documentation](https://api.immich.app/endpoints)
- [Immich GitHub Repository](https://github.com/immich-app/immich)
- [Apple Shared Albums Protocol](https://github.com/cxmeel/unimpaired/wiki/Shared-Albums)

## License

Same as rustpush - see main LICENSE file.
