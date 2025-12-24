use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use log::{info, warn, error, debug};
use tokio::time::sleep;
use uuid::Uuid;

use crate::immich::{ImmichClient, ImmichConfig};
use crate::sharedstreams::SharedStreamClient;
use crate::AnisetteProvider;
use crate::PushError;

/// Configuration for the Immich sync service
#[derive(Debug, Clone)]
pub struct ImmichSyncConfig {
    /// Immich server configuration
    pub immich: ImmichConfig,

    /// Whether to automatically approve new shared album invitations
    pub auto_approve_shares: bool,

    /// Whether to send push notifications for approval (when auto_approve is false)
    pub push_notification_approval: bool,

    /// Sync interval in seconds
    pub sync_interval_secs: u64,

    /// Directory to use for temporary file storage during sync
    pub temp_dir: PathBuf,

    /// Device ID to use for Immich uploads
    pub device_id: String,
}

impl Default for ImmichSyncConfig {
    fn default() -> Self {
        Self {
            immich: ImmichConfig {
                server_url: "http://localhost:2283".to_string(),
                api_key: String::new(),
            },
            auto_approve_shares: false,
            push_notification_approval: true,
            sync_interval_secs: 60,
            temp_dir: PathBuf::from("/tmp/rustpush-immich-sync"),
            device_id: Uuid::new_v4().to_string(),
        }
    }
}

/// Represents the sync state between an Apple shared album and an Immich album
#[derive(Debug, Clone)]
struct AlbumSyncState {
    /// Apple shared album GUID
    apple_album_guid: String,

    /// Immich album ID
    immich_album_id: String,

    /// Map of Apple asset GUID to Immich asset ID
    apple_to_immich: HashMap<String, String>,

    /// Map of Immich asset ID to Apple asset GUID
    immich_to_apple: HashMap<String, String>,
}

/// Service for synchronizing Apple shared albums with Immich
pub struct ImmichSyncService<P: AnisetteProvider> {
    config: ImmichSyncConfig,
    immich_client: ImmichClient,
    apple_client: SharedStreamClient<P>,
    sync_states: HashMap<String, AlbumSyncState>,
}

impl<P: AnisetteProvider> ImmichSyncService<P> {
    /// Create a new sync service
    pub fn new(
        config: ImmichSyncConfig,
        apple_client: SharedStreamClient<P>,
    ) -> Self {
        let immich_client = ImmichClient::new(config.immich.clone());

        Self {
            config,
            immich_client,
            apple_client,
            sync_states: HashMap::new(),
        }
    }

    /// Start the sync service (runs indefinitely)
    pub async fn run(&mut self) -> Result<(), PushError> {
        info!("Starting Immich sync service");

        // Create temp directory if it doesn't exist
        tokio::fs::create_dir_all(&self.config.temp_dir)
            .await
            .map_err(|e| PushError::FilePackageError(format!("Failed to create temp directory: {}", e)))?;

        loop {
            if let Err(e) = self.sync_cycle().await {
                error!("Sync cycle failed: {}", e);
            }

            debug!("Sleeping for {} seconds", self.config.sync_interval_secs);
            sleep(Duration::from_secs(self.config.sync_interval_secs)).await;
        }
    }

    /// Perform one sync cycle
    async fn sync_cycle(&mut self) -> Result<(), PushError> {
        info!("Starting sync cycle");

        // Step 1: Check for new album shares and handle approval
        self.handle_new_shares().await?;

        // Step 2: Sync from Apple to Immich
        self.sync_apple_to_immich().await?;

        // Step 3: Sync from Immich to Apple
        self.sync_immich_to_apple().await?;

        info!("Sync cycle completed");
        Ok(())
    }

    /// Handle new shared album invitations
    async fn handle_new_shares(&mut self) -> Result<(), PushError> {
        debug!("Checking for new album shares");

        // Get changes from Apple
        self.apple_client.get_changes().await?;

        // Check each album
        let albums = self.apple_client.state.read().await.albums.clone();
        for album in &albums {
            // Skip if already synced
            if self.sync_states.contains_key(&album.albumguid) {
                continue;
            }

            // Check if this is a new invitation (not yet subscribed)
            if album.subscriptiondate.is_none() {
                info!("New album share detected: {} ({})",
                    album.name.as_deref().unwrap_or("Unnamed"),
                    album.albumguid
                );

                if self.config.auto_approve_shares {
                    // Auto-approve
                    info!("Auto-approving album share: {}", album.albumguid);
                    self.approve_and_setup_album(&album.albumguid).await?;
                } else if self.config.push_notification_approval {
                    // TODO: Send push notification for manual approval
                    warn!("Manual approval required for album: {} (push notification not yet implemented)",
                        album.albumguid
                    );
                } else {
                    info!("Album requires manual approval: {}", album.albumguid);
                }
            }
        }

        Ok(())
    }

    /// Approve an album share and set up sync state
    pub async fn approve_and_setup_album(&mut self, album_guid: &str) -> Result<(), PushError> {
        info!("Approving and setting up album: {}", album_guid);

        // Subscribe to the album in Apple
        self.apple_client.subscribe(album_guid).await?;

        // Get album details - find the album in state
        let state = self.apple_client.state.read().await;
        let album = state.albums.iter()
            .find(|a| a.albumguid == album_guid)
            .ok_or_else(|| PushError::AlbumNotFound)?;

        let album_name = album.name.clone()
            .or_else(|| album.fullname.clone())
            .unwrap_or_else(|| format!("Album {}", album_guid));
        drop(state);

        info!("Album name: {}", album_name);

        // Create or find corresponding Immich album
        let immich_album = match self.immich_client
            .find_album_by_name(&album_name)
            .await
            .map_err(|e| PushError::FilePackageError(e.to_string()))?
        {
            Some(album) => {
                info!("Found existing Immich album: {} ({})", album_name, album.id);
                album
            }
            None => {
                info!("Creating new Immich album: {}", album_name);
                self.immich_client
                    .create_album(&album_name, Some(&format!("Synced from Apple Shared Album")))
                    .await
                    .map_err(|e| PushError::FilePackageError(e.to_string()))?
            }
        };

        // Create sync state
        let sync_state = AlbumSyncState {
            apple_album_guid: album_guid.to_string(),
            immich_album_id: immich_album.id,
            apple_to_immich: HashMap::new(),
            immich_to_apple: HashMap::new(),
        };

        self.sync_states.insert(album_guid.to_string(), sync_state);

        info!("Album setup complete: {} <-> {}", album_guid, immich_album.album_name);

        Ok(())
    }

    /// Sync assets from Apple to Immich
    async fn sync_apple_to_immich(&mut self) -> Result<(), PushError> {
        debug!("Syncing from Apple to Immich");

        let albums = self.apple_client.state.read().await.albums.clone();
        for album in albums {
            // Skip if not set up for sync
            let Some(sync_state) = self.sync_states.get(&album.albumguid) else {
                continue;
            };

            let immich_album_id = sync_state.immich_album_id.clone();
            let existing_mappings = sync_state.apple_to_immich.clone();

            // Get assets for this album
            let assets = self.apple_client.get_assets(&album.albumguid).await?;

            for asset in assets {
                // Skip if already synced
                if existing_mappings.contains_key(&asset.assetguid) {
                    continue;
                }

                info!("Syncing asset from Apple to Immich: {} ({})",
                    asset.filename, asset.assetguid
                );

                // Download asset from Apple
                let temp_file = self.config.temp_dir.join(&asset.filename);
                self.apple_client
                    .get_file(&album.albumguid, &asset.assetguid, &temp_file)
                    .await?;

                // Upload to Immich
                let file_created = DateTime::parse_from_rfc3339(
                    &asset.collectionmetadata.created
                )
                    .unwrap_or_else(|_| Utc::now().into())
                    .with_timezone(&Utc);

                let upload_response = self.immich_client
                    .upload_asset(
                        &temp_file,
                        &asset.assetguid,
                        &self.config.device_id,
                        file_created,
                        file_created,
                    )
                    .await
                    .map_err(|e| PushError::FilePackageError(e.to_string()))?;

                info!("Uploaded to Immich: {} (duplicate: {})",
                    upload_response.id, upload_response.duplicate
                );

                // Add to album
                self.immich_client
                    .add_assets_to_album(&immich_album_id, &[upload_response.id.clone()])
                    .await
                    .map_err(|e| PushError::FilePackageError(e.to_string()))?;

                // Update sync state
                if let Some(state) = self.sync_states.get_mut(&album.albumguid) {
                    state.apple_to_immich.insert(
                        asset.assetguid.clone(),
                        upload_response.id.clone(),
                    );
                    state.immich_to_apple.insert(
                        upload_response.id.clone(),
                        asset.assetguid.clone(),
                    );
                }

                // Clean up temp file
                let _ = tokio::fs::remove_file(&temp_file).await;
            }
        }

        Ok(())
    }

    /// Sync assets from Immich to Apple
    async fn sync_immich_to_apple(&mut self) -> Result<(), PushError> {
        debug!("Syncing from Immich to Apple");

        for (album_guid, sync_state) in &self.sync_states {
            let immich_album_id = &sync_state.immich_album_id;

            // Get assets from Immich album
            let immich_assets = self.immich_client
                .get_album_assets(immich_album_id)
                .await
                .map_err(|e| PushError::FilePackageError(e.to_string()))?;

            // Find assets that exist in Immich but not in Apple
            let existing_immich_ids: HashSet<_> = sync_state.immich_to_apple.keys().collect();

            for asset in immich_assets {
                // Skip if already synced
                if existing_immich_ids.contains(&asset.id) {
                    continue;
                }

                info!("Syncing asset from Immich to Apple: {} ({})",
                    asset.original_file_name, asset.id
                );

                // Download from Immich
                let temp_file = self.config.temp_dir.join(&asset.original_file_name);
                let asset_data = self.immich_client
                    .download_asset(&asset.id)
                    .await
                    .map_err(|e| PushError::FilePackageError(e.to_string()))?;

                tokio::fs::write(&temp_file, &asset_data)
                    .await
                    .map_err(|e| PushError::FilePackageError(format!("Failed to write file: {}", e)))?;

                // Upload to Apple shared album
                // Note: This requires implementing the create_asset method
                // For now, we'll log a warning
                warn!("Uploading from Immich to Apple not yet fully implemented for asset: {}",
                    asset.id
                );

                // TODO: Implement upload to Apple shared album
                // let apple_asset_guid = self.apple_client
                //     .create_asset(album_guid, &temp_file)
                //     .await?;

                // Clean up temp file
                let _ = tokio::fs::remove_file(&temp_file).await;
            }
        }

        Ok(())
    }

    /// Get the current sync state for all albums
    pub fn get_sync_states(&self) -> &HashMap<String, AlbumSyncState> {
        &self.sync_states
    }

    /// Manually approve a specific album
    pub async fn approve_album(&mut self, album_guid: &str) -> Result<(), PushError> {
        self.approve_and_setup_album(album_guid).await
    }
}
