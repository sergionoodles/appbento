//! Shared models for local library state.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GithubUpdateConfig {
    /// "owner/repo"
    pub repo: String,
    /// Regex matched against release asset names, e.g. `.*x86_64\.AppImage$`
    pub asset_regex: String,
    pub architecture: String,
    pub tag_regex: String,
    #[serde(default)]
    pub prerelease: bool,
    /// stable | beta | nightly | continuous
    #[serde(default = "default_channel")]
    pub channel: String,
}

fn default_channel() -> String {
    "stable".to_string()
}

/// Update source saved with the installed app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "strategy", rename_all = "kebab-case")]
pub enum UpdateConfig {
    GithubRelease(GithubUpdateConfig),
    GitlabRelease(GithubUpdateConfig),
    DirectUrl {
        url: String,
    },
    /// HTTPS URL of a `.zsync` control file.
    Zsync {
        url: String,
    },
    /// HTTPS base URL of an electron-builder feed directory.
    ElectronFeed {
        url: String,
    },
}

/// Who chose the saved update source; only detected sources are refreshed automatically.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateConfigOrigin {
    Detected,
    User,
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    #[default]
    AppImage,
    Binary,
}

impl<'de> Deserialize<'de> for AppKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "appimage" => Ok(Self::AppImage),
            "binary" => Ok(Self::Binary),
            value => Err(serde::de::Error::custom(format!(
                "invalid app kind: {value}"
            ))),
        }
    }
}

/// Local library state for applications actually on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledApp {
    #[serde(default)]
    pub kind: AppKind,
    #[serde(default)]
    pub update_config: Option<UpdateConfig>,
    #[serde(default)]
    pub update_config_origin: Option<UpdateConfigOrigin>,
    pub id: String,
    pub name: String,
    pub file_path: String,
    #[serde(default)]
    pub source_path: Option<String>,
    pub file_name: String,
    pub size_bytes: u64,
    pub executable: bool,
    pub integrated: bool,
    pub version_current: Option<String>,
    pub icon_path: Option<String>,
    pub desktop_id: Option<String>,
    pub launch_count: u64,
    pub last_launched: Option<DateTime<Utc>>,
    pub added_at: DateTime<Utc>,
    /// Filesystem creation time, or modification time where the filesystem has no creation time.
    #[serde(default)]
    pub file_created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub launch_config: Option<LaunchConfig>,
    #[serde(default)]
    pub reviewed_digest: Option<String>,
}

impl InstalledApp {
    pub fn validate_kind(&self) -> Result<(), String> {
        if self.kind == AppKind::Binary
            && (self.integrated || self.source_path.is_some() || self.update_config.is_some())
        {
            return Err("Executable entries cannot be integrated, have a source path, or have update settings".into());
        }
        Ok(())
    }

    /// Saves an update source the user chose; clearing it also stops automatic detection.
    pub fn set_user_update_config(&mut self, config: Option<UpdateConfig>) {
        self.update_config = config;
        self.update_config_origin = Some(UpdateConfigOrigin::User);
    }

    pub fn require_appimage_update(&self) -> Result<(), String> {
        if self.kind == AppKind::Binary {
            return Err("Executable updates are not supported yet".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanWarning {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanReport {
    pub apps: Vec<InstalledApp>,
    pub warnings: Vec<ScanWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub app_id: String,
    pub status: UpdateStatus,
    pub current: Option<String>,
    pub latest: String,
    pub download_url: String,
    pub asset_name: String,
    pub published_at: Option<DateTime<Utc>>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateStatus {
    Available,
    UpToDate,
    UnknownCurrent,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub app_id: String,
    pub status: UpdateStatus,
    pub latest: Option<String>,
    pub update: Option<UpdateInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckError {
    pub app_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateReport {
    pub updates: Vec<UpdateInfo>,
    pub errors: Vec<UpdateCheckError>,
    pub unknown_versions: Vec<String>,
    pub checked_count: usize,
    pub unsupported_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchMode {
    Direct,
    Bubblewrap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LaunchConfig {
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub environment: std::collections::BTreeMap<String, String>,
    pub mode: LaunchMode,
    #[serde(default)]
    pub permissions: SandboxPermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PathAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PathPermission {
    pub path: String,
    pub access: PathAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SandboxPermissions {
    pub network: bool,
    pub display: bool,
    pub audio: bool,
    pub gpu: bool,
    pub persistent_data: bool,
    pub paths: Vec<PathPermission>,
    pub desktop_integration: DesktopIntegration,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbusAccess {
    Talk,
    Own,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DbusRule {
    pub name: String,
    pub access: DbusAccess,
}

/// Session bus names a sandboxed app may use through a filtering proxy.
/// Everything is off by default so apps saved before this setting keep running without a bus.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct DesktopIntegration {
    pub portals: bool,
    pub notifications: bool,
    pub tray: bool,
    pub media_controls: bool,
    pub secrets: bool,
    pub custom: Vec<DbusRule>,
}

impl DesktopIntegration {
    /// Defaults offered when an app is reviewed for the first time.
    pub fn recommended() -> Self {
        Self {
            portals: true,
            notifications: true,
            ..Self::default()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.portals
            || self.notifications
            || self.tray
            || self.media_controls
            || self.secrets
            || !self.custom.is_empty()
    }
}

impl Default for SandboxPermissions {
    fn default() -> Self {
        Self {
            network: false,
            display: true,
            audio: false,
            gpu: false,
            persistent_data: true,
            paths: Vec::new(),
            desktop_integration: DesktopIntegration::default(),
        }
    }
}
