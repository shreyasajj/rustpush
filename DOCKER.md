# Docker Deployment Guide

Run the Immich sync service in a Docker container for easy deployment and management.

## Quick Start

### 1. Prepare Configuration Directory

```bash
# Create config directory
mkdir -p config

# Copy example config
cp immich-config.example.plist config/immich-config.plist

# Edit with your settings
nano config/immich-config.plist
```

### 2. Add Apple Authentication Files

You need to authenticate with Apple first to get the required files:

```bash
# Run the authentication process (outside Docker)
cargo run --release --features macos-validation-data --bin rustpush-test

# Copy the generated files to config directory
cp config.plist config/
cp hwconfig.plist config/
```

### 3. Configure Immich Connection

Edit `config/immich-config.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>immich_server_url</key>
    <string>http://immich-server:2283</string>  <!-- Use container name or IP -->

    <key>immich_api_key</key>
    <string>YOUR_API_KEY_HERE</string>

    <key>auto_approve_shares</key>
    <true/>

    <key>sync_interval_secs</key>
    <integer>60</integer>
</dict>
</plist>
```

### 4. Build and Run

Using Docker Compose (recommended):

```bash
# Build the image
docker-compose build

# Start the service
docker-compose up -d

# View logs
docker-compose logs -f immich-sync

# Stop the service
docker-compose down
```

Using Docker directly:

```bash
# Build the image
docker build -t rustpush-immich-sync .

# Run the container
docker run -d \
  --name rustpush-immich-sync \
  --restart unless-stopped \
  -v $(pwd)/config:/app/config \
  -v rustpush-temp:/tmp/rustpush-immich-sync \
  -e RUST_LOG=info \
  rustpush-immich-sync

# View logs
docker logs -f rustpush-immich-sync
```

## Configuration

### Directory Structure

```
.
├── config/
│   ├── config.plist           # Apple authentication state
│   ├── hwconfig.plist         # Hardware configuration
│   └── immich-config.plist    # Immich sync configuration
├── docker-compose.yml
└── Dockerfile
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level: `error`, `warn`, `info`, `debug`, `trace` |

### Volumes

| Volume | Purpose |
|--------|---------|
| `/app/config` | Configuration files and authentication state |
| `/tmp/rustpush-immich-sync` | Temporary storage for photo downloads |

## Networking

### Option 1: Same Docker Network as Immich

If Immich runs in Docker, add the sync service to the same network:

```yaml
# docker-compose.yml
networks:
  immich:
    external: true
    name: immich_default  # Replace with your Immich network name
```

Then use the Immich container name in config:
```xml
<key>immich_server_url</key>
<string>http://immich-server:2283</string>
```

### Option 2: Host Network

If Immich runs on the host:

```yaml
# docker-compose.yml
services:
  immich-sync:
    network_mode: host
```

Then use localhost:
```xml
<key>immich_server_url</key>
<string>http://localhost:2283</string>
```

### Option 3: Bridge Network

Access Immich via host IP:

```xml
<key>immich_server_url</key>
<string>http://192.168.1.100:2283</string>  <!-- Your host IP -->
```

## Monitoring

### View Logs

```bash
# Follow logs
docker-compose logs -f immich-sync

# Last 100 lines
docker-compose logs --tail=100 immich-sync

# With timestamps
docker-compose logs -f --timestamps immich-sync
```

### Health Check

```bash
# Check container health
docker-compose ps

# Manual health check
docker exec rustpush-immich-sync pgrep -f immich-sync
```

### Enable Debug Logging

Edit `docker-compose.yml`:

```yaml
environment:
  - RUST_LOG=debug
```

Then restart:

```bash
docker-compose down
docker-compose up -d
```

## Updating

### Update the Container

```bash
# Pull latest code
git pull

# Rebuild and restart
docker-compose down
docker-compose build --no-cache
docker-compose up -d
```

## Troubleshooting

### Container Won't Start

1. **Check logs:**
   ```bash
   docker-compose logs immich-sync
   ```

2. **Verify config files exist:**
   ```bash
   ls -la config/
   # Should show: config.plist, hwconfig.plist, immich-config.plist
   ```

3. **Check permissions:**
   ```bash
   # Config directory should be readable
   chmod -R 755 config/
   ```

### Can't Connect to Immich

1. **Test connectivity from container:**
   ```bash
   docker-compose exec immich-sync /bin/bash
   # Inside container:
   apt-get update && apt-get install -y curl
   curl http://immich-server:2283/api/server-info/ping
   ```

2. **Check network:**
   ```bash
   docker network ls
   docker network inspect immich
   ```

3. **Verify API key:**
   - Log into Immich web UI
   - Check API key is valid and has correct permissions

### Authentication Errors

If you see "No saved state found":

1. **Re-run authentication outside Docker:**
   ```bash
   cargo run --release --features macos-validation-data --bin rustpush-test
   ```

2. **Copy files to config directory:**
   ```bash
   cp config.plist hwconfig.plist config/
   ```

3. **Restart container:**
   ```bash
   docker-compose restart
   ```

### High Memory Usage

The service downloads photos to temp storage. Monitor disk usage:

```bash
# Check volume size
docker system df -v | grep rustpush

# Clean up temp files
docker-compose down
docker volume rm rustpush-immich-sync_immich-sync-temp
docker-compose up -d
```

## Advanced Configuration

### Custom Dockerfile Build Args

Build with different Rust version:

```bash
docker build --build-arg RUST_VERSION=1.76 -t rustpush-immich-sync .
```

### Resource Limits

Add resource constraints in `docker-compose.yml`:

```yaml
services:
  immich-sync:
    deploy:
      resources:
        limits:
          cpus: '1.0'
          memory: 512M
        reservations:
          cpus: '0.5'
          memory: 256M
```

### Multiple Instances

Run multiple sync services for different Apple IDs:

```bash
# Create separate config directories
mkdir -p config-account1 config-account2

# Copy configs
cp config.plist config-account1/
cp config.plist config-account2/
# ... edit each with different credentials

# Run separate containers
docker run -d --name sync-account1 -v $(pwd)/config-account1:/app/config rustpush-immich-sync
docker run -d --name sync-account2 -v $(pwd)/config-account2:/app/config rustpush-immich-sync
```

## Production Deployment

### systemd Integration

Create `/etc/systemd/system/rustpush-immich-sync.service`:

```ini
[Unit]
Description=Rustpush Immich Sync
Requires=docker.service
After=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=/path/to/rustpush
ExecStart=/usr/bin/docker-compose up -d
ExecStop=/usr/bin/docker-compose down
TimeoutStartSec=0

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl enable rustpush-immich-sync
sudo systemctl start rustpush-immich-sync
```

### Backup Configuration

```bash
# Backup config directory
tar -czf rustpush-backup-$(date +%Y%m%d).tar.gz config/

# Restore
tar -xzf rustpush-backup-20240101.tar.gz
```

### Monitoring with Prometheus

Add metrics endpoint (future enhancement):

```yaml
services:
  immich-sync:
    ports:
      - "9090:9090"  # Metrics port
```

## Security Considerations

1. **Protect config files** - Contains sensitive authentication data
   ```bash
   chmod 600 config/*.plist
   ```

2. **Use secrets for API keys** (Docker Swarm/Kubernetes):
   ```yaml
   secrets:
     immich_api_key:
       external: true
   ```

3. **Network isolation** - Only expose to Immich network
   ```yaml
   networks:
     immich:
       internal: true
   ```

4. **Read-only filesystem** (optional):
   ```yaml
   read_only: true
   tmpfs:
     - /tmp
   ```

## See Also

- [Main Documentation](IMMICH_INTEGRATION.md)
- [Immich Documentation](https://immich.app/docs)
- [Docker Documentation](https://docs.docker.com/)
