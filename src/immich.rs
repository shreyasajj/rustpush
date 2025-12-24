use std::path::Path;
use std::collections::HashMap;

use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use chrono::{DateTime, Utc};

#[derive(Error, Debug)]
pub enum ImmichError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Failed to read file: {0}")]
    FileReadError(#[from] std::io::Error),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Album not found: {0}")]
    AlbumNotFound(String),

    #[error("Asset not found: {0}")]
    AssetNotFound(String),
}

pub type Result<T> = std::result::Result<T, ImmichError>;

/// Configuration for connecting to an Immich server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmichConfig {
    /// Base URL of the Immich server (e.g., "http://localhost:2283")
    pub server_url: String,

    /// API key for authentication
    pub api_key: String,
}

/// Represents an album in Immich
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmichAlbum {
    pub id: String,
    pub album_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub owner_id: String,
    pub album_thumbnail_asset_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub asset_count: u32,
    #[serde(default)]
    pub assets: Vec<ImmichAsset>,
}

/// Represents an asset (photo/video) in Immich
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmichAsset {
    pub id: String,
    pub device_asset_id: String,
    pub owner_id: String,
    pub device_id: String,
    #[serde(rename = "type")]
    pub asset_type: String,
    pub original_path: String,
    pub original_file_name: String,
    pub file_created_at: String,
    pub file_modified_at: String,
    pub is_favorite: bool,
    pub is_archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exif_info: Option<ExifInfo>,
}

/// EXIF information for an asset
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_time_original: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

/// Request to create a new album
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlbumRequest {
    pub album_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Response from uploading an asset
#[derive(Debug, Deserialize)]
pub struct UploadAssetResponse {
    pub id: String,
    pub duplicate: bool,
}

/// Client for interacting with the Immich API
pub struct ImmichClient {
    config: ImmichConfig,
    client: Client,
}

impl ImmichClient {
    /// Create a new Immich client
    pub fn new(config: ImmichConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Get the base API URL
    fn api_url(&self, path: &str) -> String {
        format!("{}/api{}", self.config.server_url.trim_end_matches('/'), path)
    }

    /// List all albums
    pub async fn list_albums(&self) -> Result<Vec<ImmichAlbum>> {
        let url = self.api_url("/albums");

        let response = self.client
            .get(&url)
            .header("x-api-key", &self.config.api_key)
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ImmichError::ApiError(format!(
                "Failed to list albums: {}",
                response.status()
            )));
        }

        Ok(response.json().await?)
    }

    /// Get a specific album by ID
    pub async fn get_album(&self, album_id: &str) -> Result<ImmichAlbum> {
        let url = self.api_url(&format!("/albums/{}", album_id));

        let response = self.client
            .get(&url)
            .header("x-api-key", &self.config.api_key)
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ImmichError::AlbumNotFound(album_id.to_string()));
        }

        Ok(response.json().await?)
    }

    /// Find an album by name
    pub async fn find_album_by_name(&self, name: &str) -> Result<Option<ImmichAlbum>> {
        let albums = self.list_albums().await?;
        Ok(albums.into_iter().find(|a| a.album_name == name))
    }

    /// Create a new album
    pub async fn create_album(&self, name: &str, description: Option<&str>) -> Result<ImmichAlbum> {
        let url = self.api_url("/albums");

        let request = CreateAlbumRequest {
            album_name: name.to_string(),
            description: description.map(|s| s.to_string()),
        };

        let response = self.client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ImmichError::ApiError(format!(
                "Failed to create album: {}",
                response.status()
            )));
        }

        Ok(response.json().await?)
    }

    /// Upload an asset (photo or video) to Immich
    pub async fn upload_asset<P: AsRef<Path>>(
        &self,
        file_path: P,
        device_asset_id: &str,
        device_id: &str,
        file_created_at: DateTime<Utc>,
        file_modified_at: DateTime<Utc>,
    ) -> Result<UploadAssetResponse> {
        let url = self.api_url("/assets");
        let path = file_path.as_ref();

        // Read file
        let file_bytes = tokio::fs::read(path).await?;
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Determine MIME type
        let mime_type = if file_name.to_lowercase().ends_with(".jpg")
            || file_name.to_lowercase().ends_with(".jpeg") {
            "image/jpeg"
        } else if file_name.to_lowercase().ends_with(".png") {
            "image/png"
        } else if file_name.to_lowercase().ends_with(".heic") {
            "image/heic"
        } else if file_name.to_lowercase().ends_with(".mp4") {
            "video/mp4"
        } else if file_name.to_lowercase().ends_with(".mov") {
            "video/quicktime"
        } else {
            "application/octet-stream"
        };

        // Build multipart form
        let file_part = multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str(mime_type)?;

        let form = multipart::Form::new()
            .part("assetData", file_part)
            .text("deviceAssetId", device_asset_id.to_string())
            .text("deviceId", device_id.to_string())
            .text("fileCreatedAt", file_created_at.to_rfc3339())
            .text("fileModifiedAt", file_modified_at.to_rfc3339());

        let response = self.client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("Accept", "application/json")
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ImmichError::ApiError(format!(
                "Failed to upload asset: {} - {}",
                status, body
            )));
        }

        Ok(response.json().await?)
    }

    /// Add assets to an album
    pub async fn add_assets_to_album(&self, album_id: &str, asset_ids: &[String]) -> Result<()> {
        let url = self.api_url(&format!("/albums/{}/assets", album_id));

        let mut body = HashMap::new();
        body.insert("ids", asset_ids);

        let response = self.client
            .put(&url)
            .header("x-api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ImmichError::ApiError(format!(
                "Failed to add assets to album: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Download an asset from Immich
    pub async fn download_asset(&self, asset_id: &str) -> Result<Vec<u8>> {
        let url = self.api_url(&format!("/assets/{}/original", asset_id));

        let response = self.client
            .get(&url)
            .header("x-api-key", &self.config.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ImmichError::AssetNotFound(asset_id.to_string()));
        }

        Ok(response.bytes().await?.to_vec())
    }

    /// Get all assets in an album
    pub async fn get_album_assets(&self, album_id: &str) -> Result<Vec<ImmichAsset>> {
        let album = self.get_album(album_id).await?;
        Ok(album.assets)
    }

    /// Delete an asset
    pub async fn delete_asset(&self, asset_id: &str) -> Result<()> {
        let url = self.api_url(&format!("/assets/{}", asset_id));

        let response = self.client
            .delete(&url)
            .header("x-api-key", &self.config.api_key)
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ImmichError::ApiError(format!(
                "Failed to delete asset: {}",
                response.status()
            )));
        }

        Ok(())
    }
}
