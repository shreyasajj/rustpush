use std::{fs, io::Cursor, path::PathBuf, sync::Arc, time::Duration};

use icloud_auth::AppleAccount;
use log::{debug, error, info, warn};
use omnisette::default_provider;
use rustpush::{
    authenticate_apple, immich::ImmichConfig, immich_sync::{ImmichSyncConfig, ImmichSyncService},
    login_apple_delegates, sharedstreams::SharedStreamClient, APSConnectionResource, APSState,
    IDSNGMIdentity, IDSUser, MacOSConfig, OSConfig, PushError, TokenProvider,
};
use serde::{Deserialize, Serialize};
use tokio::{io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader}, sync::RwLock};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone)]
struct SavedState {
    push: APSState,
    users: Vec<IDSUser>,
    identity: IDSNGMIdentity,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ImmichSyncAppConfig {
    /// Immich server URL
    immich_server_url: String,

    /// Immich API key
    immich_api_key: String,

    /// Auto-approve new album shares
    #[serde(default)]
    auto_approve_shares: bool,

    /// Send push notifications for approval
    #[serde(default = "default_true")]
    push_notification_approval: bool,

    /// Sync interval in seconds
    #[serde(default = "default_sync_interval")]
    sync_interval_secs: u64,

    /// Temporary directory for file storage
    #[serde(default = "default_temp_dir")]
    temp_dir: PathBuf,

    /// Device ID for uploads
    #[serde(default = "default_device_id")]
    device_id: String,
}

fn default_true() -> bool {
    true
}

fn default_sync_interval() -> u64 {
    60
}

fn default_temp_dir() -> PathBuf {
    PathBuf::from("/tmp/rustpush-immich-sync")
}

fn default_device_id() -> String {
    Uuid::new_v4().to_string()
}

impl Default for ImmichSyncAppConfig {
    fn default() -> Self {
        Self {
            immich_server_url: "http://localhost:2283".to_string(),
            immich_api_key: String::new(),
            auto_approve_shares: false,
            push_notification_approval: true,
            sync_interval_secs: 60,
            temp_dir: PathBuf::from("/tmp/rustpush-immich-sync"),
            device_id: Uuid::new_v4().to_string(),
        }
    }
}

pub fn plist_to_buf<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, plist::Error> {
    let mut buf: Vec<u8> = Vec::new();
    let writer = Cursor::new(&mut buf);
    plist::to_writer_xml(writer, &value)?;
    Ok(buf)
}

pub fn plist_to_string<T: serde::Serialize>(value: &T) -> Result<String, plist::Error> {
    plist_to_buf(value).map(|val| String::from_utf8(val).unwrap())
}

async fn read_input() -> String {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut input = String::new();
    reader.read_line(&mut input).await.unwrap();
    input.trim().to_string()
}

#[tokio::main]
async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::try_init().unwrap();

    info!("🚀 Starting Immich sync service for Apple Shared Albums");

    // Load or create Immich sync configuration
    let immich_config: ImmichSyncAppConfig = match fs::read_to_string("immich-config.plist") {
        Ok(data) => match plist::from_bytes(data.as_bytes()) {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to parse immich-config.plist: {}", e);
                info!("Creating default configuration...");
                let config = ImmichSyncAppConfig::default();
                let config_str = plist_to_string(&config).unwrap();
                fs::write("immich-config.plist", config_str).unwrap();
                println!("\n📝 Created immich-config.plist with default values.");
                println!("Please edit this file with your Immich server URL and API key.");
                std::process::exit(1);
            }
        },
        Err(_) => {
            info!("immich-config.plist not found, creating default...");
            let config = ImmichSyncAppConfig::default();
            let config_str = plist_to_string(&config).unwrap();
            fs::write("immich-config.plist", config_str).unwrap();
            println!("\n📝 Created immich-config.plist with default values.");
            println!("Please edit this file with your Immich server URL and API key, then restart.");
            std::process::exit(1);
        }
    };

    if immich_config.immich_api_key.is_empty() {
        error!("Immich API key is not set in immich-config.plist");
        println!("Please set 'immich_api_key' in immich-config.plist");
        std::process::exit(1);
    }

    info!("📦 Immich server: {}", immich_config.immich_server_url);
    info!("🔄 Sync interval: {} seconds", immich_config.sync_interval_secs);
    info!(
        "✅ Auto-approve shares: {}",
        immich_config.auto_approve_shares
    );

    // Load Apple configuration
    let data: String = match tokio::fs::read_to_string("config.plist").await {
        Ok(v) => v,
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                let _ = tokio::fs::File::create("config.plist")
                    .await
                    .expect("Unable to create file")
                    .write_all(b"{}");
                "{}".to_string()
            }
            _ => {
                error!("Unable to read config.plist");
                std::process::exit(1);
            }
        },
    };

    let config: Arc<MacOSConfig> = Arc::new(if let Ok(config) = plist::from_file("hwconfig.plist")
    {
        config
    } else {
        println!("Missing hardware config!");
        println!(
            "The easiest way to get your hardware config is to extract it from validation data."
        );
        println!("Run the main rustpush-test to set up authentication first.");
        std::process::exit(1);
    });

    let saved: SavedState = match plist::from_bytes(data.as_bytes()) {
        Ok(v) => v,
        Err(_) => {
            println!("No saved state found. You need to authenticate first.");
            println!("Please run rustpush-test to set up Apple authentication.");
            std::process::exit(1);
        }
    };

    let provider = default_provider("https://ani.sidestore.io/");

    // Authenticate with Apple
    let push_res = APSConnectionResource::new(saved.push.clone())
        .await
        .unwrap();

    let account = AppleAccount::new(&provider, saved.users[0].clone().into(), config.clone())
        .await
        .unwrap();

    let token = Arc::new(RwLock::new(
        TokenProvider::new(account, saved.users[0].handles[0].clone())
            .await
            .unwrap(),
    ));

    let dsid = saved.users[0]
        .raw_properties
        .get("dsid")
        .unwrap()
        .as_string()
        .unwrap()
        .to_string();

    info!("✅ Authenticated with Apple ID (DSID: {})", dsid);

    // Create shared streams client
    let apple_client = SharedStreamClient::new(
        token.clone(),
        push_res.state.clone(),
        dsid,
        "ckdatabase.apple.com".to_string(),
    )
    .await
    .unwrap();

    info!("✅ Connected to Apple Shared Streams");

    // Create sync configuration
    let sync_config = ImmichSyncConfig {
        immich: ImmichConfig {
            server_url: immich_config.immich_server_url,
            api_key: immich_config.immich_api_key,
        },
        auto_approve_shares: immich_config.auto_approve_shares,
        push_notification_approval: immich_config.push_notification_approval,
        sync_interval_secs: immich_config.sync_interval_secs,
        temp_dir: immich_config.temp_dir,
        device_id: immich_config.device_id,
    };

    // Create sync service
    let mut sync_service = ImmichSyncService::new(sync_config, apple_client);

    info!("🎯 Sync service initialized");
    info!("Starting synchronization loop...");

    // Run the sync service
    if let Err(e) = sync_service.run().await {
        error!("Sync service failed: {}", e);
        std::process::exit(1);
    }
}
