use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    install::{InstallPhaseDetails, InstallProgressReporter},
    pack::import::version_folder::{
        self, GameDirConfig, VersionImportOptions, is_minecraft_root,
    },
    util::io,
};

#[derive(Deserialize, Default)]
struct HmclConfig {
    #[serde(default, rename = "commonDir")]
    common_dir: Option<String>,
    #[serde(default, rename = "commonDirectory")]
    common_directory: Option<String>,
    #[serde(default, rename = "gameDir")]
    game_dir: Option<String>,
}

#[derive(Deserialize, Default)]
struct HmclVersionSetting {
    #[serde(default, rename = "gameDirType")]
    game_dir_type: Option<serde_json::Value>,
    #[serde(default, rename = "gameDir")]
    game_dir: Option<String>,
}

pub fn find_default_path() -> Option<PathBuf> {
    hmcl_candidates()
        .into_iter()
        .find(|path| looks_like_hmcl(path))
}

pub async fn list_instances(base_path: PathBuf) -> crate::Result<Vec<String>> {
    let minecraft_root = resolve_minecraft_root(&base_path).await;
    version_folder::list_version_folders(&minecraft_root).await
}

pub async fn import_hmcl(
    base_path: PathBuf,
    instance_folder: String,
    instance_id: &str,
    reporter: InstallProgressReporter,
    details: InstallPhaseDetails,
) -> crate::Result<()> {
    let minecraft_root = resolve_minecraft_root(&base_path).await;
    let version_dir = minecraft_root.join("versions").join(&instance_folder);
    let game_dir = game_dir_from_version_setting(&version_dir, &minecraft_root)
        .await
        .or_else(|| {
            version_folder::looks_isolated(&version_dir)
                .then_some(GameDirConfig::VersionFolder)
        });

    version_folder::import_version_instance(
        &minecraft_root,
        &instance_folder,
        instance_id,
        format!("HMCL-{instance_folder}"),
        VersionImportOptions {
            display_name: Some(instance_folder.clone()),
            icon_candidates: Vec::new(),
            game_dir,
        },
        reporter,
        details,
    )
    .await
}

pub async fn resolve_minecraft_root(base_path: &Path) -> PathBuf {
    if is_minecraft_root(base_path) {
        return base_path.to_path_buf();
    }
    let nested = base_path.join(".minecraft");
    if is_minecraft_root(&nested) {
        return nested;
    }
    if let Some(from_config) = minecraft_root_from_config(base_path).await
        && is_minecraft_root(&from_config)
    {
        return from_config;
    }
    if looks_like_hmcl(base_path)
        && let Some(official) =
            dirs::config_dir().map(|dir| dir.join(".minecraft"))
        && is_minecraft_root(&official)
    {
        return official;
    }
    nested
}

fn looks_like_hmcl(path: &Path) -> bool {
    path.join("hmcl.json").is_file()
        || path.join("HMCL.json").is_file()
        || path.join("HMCL.jar").is_file()
        || path.join("HMCL.exe").is_file()
        || executable_prefix_exists(path, "HMCL")
}

fn executable_prefix_exists(dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(|entry| entry.ok()).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.len() >= prefix.len()
            && name[..prefix.len()].eq_ignore_ascii_case(prefix)
            && (name.ends_with(".exe") || name.ends_with(".jar"))
    })
}

fn hmcl_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(config_dir) = dirs::config_dir() {
        candidates.push(config_dir.join("hmcl"));
        candidates.push(config_dir.join(".hmcl"));
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".hmcl"));
        for name in ["HMCL", "hmcl"] {
            candidates.push(home.join(name));
            candidates.push(home.join("Desktop").join(name));
            candidates.push(home.join("Downloads").join(name));
            candidates.push(home.join("Documents").join(name));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for drive in ["C", "D", "E"] {
            candidates.push(PathBuf::from(format!("{drive}:\\HMCL")));
            candidates.push(PathBuf::from(format!("{drive}:\\Games\\HMCL")));
        }
    }
    candidates
}

async fn minecraft_root_from_config(launcher_dir: &Path) -> Option<PathBuf> {
    let config = read_hmcl_config(launcher_dir).await?;
    for candidate in
        [config.common_dir, config.common_directory, config.game_dir]
            .into_iter()
            .flatten()
    {
        let path = PathBuf::from(candidate);
        if is_minecraft_root(&path) {
            return Some(path);
        }
        let nested = path.join(".minecraft");
        if is_minecraft_root(&nested) {
            return Some(nested);
        }
    }
    None
}

async fn read_hmcl_config(launcher_dir: &Path) -> Option<HmclConfig> {
    for name in ["hmcl.json", "HMCL.json"] {
        let path = launcher_dir.join(name);
        let Ok((text, _)) = io::read_any_encoding_to_string(&path).await else {
            continue;
        };
        if let Ok(config) = serde_json::from_str::<HmclConfig>(&text) {
            return Some(config);
        }
    }
    None
}

pub async fn game_dir_for_version(
    version_dir: &Path,
    minecraft_root: &Path,
) -> Option<GameDirConfig> {
    game_dir_from_version_setting(version_dir, minecraft_root).await
}

async fn game_dir_from_version_setting(
    version_dir: &Path,
    minecraft_root: &Path,
) -> Option<GameDirConfig> {
    for name in ["hmclversion.cfg", "hmclversion.json"] {
        let path = version_dir.join(name);
        let Ok((text, _)) = io::read_any_encoding_to_string(&path).await else {
            continue;
        };
        let Ok(setting) = serde_json::from_str::<HmclVersionSetting>(&text)
        else {
            continue;
        };
        return Some(parse_game_dir_config(setting, minecraft_root));
    }
    None
}

fn parse_game_dir_config(
    setting: HmclVersionSetting,
    minecraft_root: &Path,
) -> GameDirConfig {
    match setting.game_dir_type.as_ref() {
        Some(value) if is_version_folder_type(value) => {
            GameDirConfig::VersionFolder
        }
        Some(value) if is_custom_dir_type(value) => setting
            .game_dir
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .map(GameDirConfig::Custom)
            .unwrap_or_else(|| {
                GameDirConfig::Custom(minecraft_root.to_path_buf())
            }),
        _ => GameDirConfig::RootFolder,
    }
}

fn is_version_folder_type(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(number) => number.as_u64() == Some(1),
        serde_json::Value::String(text) => {
            text.eq_ignore_ascii_case("VERSION_FOLDER") || text == "1"
        }
        _ => false,
    }
}

fn is_custom_dir_type(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(number) => number.as_u64() == Some(2),
        serde_json::Value::String(text) => {
            text.eq_ignore_ascii_case("CUSTOM") || text == "2"
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_folder_game_dir_type() {
        let setting = HmclVersionSetting {
            game_dir_type: Some(serde_json::json!(1)),
            game_dir: None,
        };
        assert!(matches!(
            parse_game_dir_config(setting, Path::new("/tmp/.minecraft")),
            GameDirConfig::VersionFolder
        ));
    }
}
