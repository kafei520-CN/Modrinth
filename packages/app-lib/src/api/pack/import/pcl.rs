use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    install::{InstallPhaseDetails, InstallProgressReporter},
    pack::import::version_folder::{
        self, GameDirConfig, VersionImportOptions, is_minecraft_root,
    },
    util::io,
};

pub fn find_default_path() -> Option<PathBuf> {
    pcl_candidates()
        .into_iter()
        .find(|path| looks_like_pcl(path))
}

pub async fn list_instances(base_path: PathBuf) -> crate::Result<Vec<String>> {
    let minecraft_root = resolve_minecraft_root(&base_path).await;
    version_folder::list_version_folders(&minecraft_root).await
}

pub async fn import_pcl(
    base_path: PathBuf,
    instance_folder: String,
    instance_id: &str,
    reporter: InstallProgressReporter,
    details: InstallPhaseDetails,
) -> crate::Result<()> {
    let minecraft_root = resolve_minecraft_root(&base_path).await;
    let version_dir = minecraft_root.join("versions").join(&instance_folder);
    let version_ini = read_kv_file(&version_dir.join("PCL.ini")).await;
    let mut launcher_ini = read_launcher_kv(&base_path).await;
    if launcher_ini.is_empty()
        && let Some(parent) = minecraft_root.parent()
    {
        launcher_ini = read_launcher_kv(parent).await;
    }

    let display_name = version_ini
        .get("VersionArgumentTitle")
        .cloned()
        .filter(|name| !name.is_empty());
    let mut icon_candidates = Vec::new();
    if let Some(logo) = version_ini.get("VersionArgumentLogo") {
        let logo_path = PathBuf::from(logo);
        icon_candidates.push(if logo_path.is_absolute() {
            logo_path
        } else {
            base_path.join(logo)
        });
    }

    let game_dir =
        isolation_from_ini(&version_ini, &launcher_ini).or_else(|| {
            version_folder::looks_isolated(&version_dir)
                .then_some(GameDirConfig::VersionFolder)
        });

    version_folder::import_version_instance(
        &minecraft_root,
        &instance_folder,
        instance_id,
        format!("PCL-{instance_folder}"),
        VersionImportOptions {
            display_name,
            icon_candidates,
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
    if let Some(from_ini) = minecraft_root_from_ini(base_path).await
        && is_minecraft_root(&from_ini)
    {
        return from_ini;
    }
    if looks_like_pcl(base_path)
        && let Some(official) =
            dirs::config_dir().map(|dir| dir.join(".minecraft"))
        && is_minecraft_root(&official)
    {
        return official;
    }
    nested
}

fn looks_like_pcl(path: &Path) -> bool {
    path.join("PCL.ini").is_file()
        || path.join("PCL").join("Setup.ini").is_file()
        || path.join("Setup.ini").is_file()
        || path.join("Plain Craft Launcher 2.exe").is_file()
        || path.join("PCL2.exe").is_file()
        || path.join("PCL.exe").is_file()
}

fn pcl_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for name in ["PCL", "PCL2", "Plain Craft Launcher 2"] {
            candidates.push(home.join(name));
            candidates.push(home.join("Desktop").join(name));
            candidates.push(home.join("Downloads").join(name));
            candidates.push(home.join("Documents").join(name));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for drive in ["C", "D", "E"] {
            candidates.push(PathBuf::from(format!("{drive}:\\PCL")));
            candidates.push(PathBuf::from(format!("{drive}:\\PCL2")));
            candidates.push(PathBuf::from(format!("{drive}:\\Games\\PCL")));
        }
    }
    candidates
}

async fn minecraft_root_from_ini(launcher_dir: &Path) -> Option<PathBuf> {
    let ini = read_launcher_kv(launcher_dir).await;
    match ini.get("LaunchFolderSelect").map(String::as_str) {
        Some("0") => dirs::config_dir().map(|dir| dir.join(".minecraft")),
        Some("2") => ini.get("LaunchFolder").map(PathBuf::from),
        _ => Some(launcher_dir.join(".minecraft")),
    }
}

pub async fn game_dir_for_version(
    base_path: &Path,
    minecraft_root: &Path,
    version_dir: &Path,
) -> Option<GameDirConfig> {
    let version_ini = read_kv_file(&version_dir.join("PCL.ini")).await;
    let mut launcher_ini = read_launcher_kv(base_path).await;
    if launcher_ini.is_empty()
        && let Some(parent) = minecraft_root.parent()
    {
        launcher_ini = read_launcher_kv(parent).await;
    }
    isolation_from_ini(&version_ini, &launcher_ini).or_else(|| {
        version_folder::looks_isolated(version_dir)
            .then_some(GameDirConfig::VersionFolder)
    })
}

fn isolation_from_ini(
    version_ini: &HashMap<String, String>,
    launcher_ini: &HashMap<String, String>,
) -> Option<GameDirConfig> {
    isolation_value(version_ini.get("VersionArgumentIndie")).or_else(|| {
        isolation_value(
            launcher_ini
                .get("LaunchArgumentIndie")
                .or(launcher_ini.get("VersionArgumentIndieDefault")),
        )
    })
}

fn isolation_value(raw: Option<&String>) -> Option<GameDirConfig> {
    match raw.map(String::as_str) {
        Some("1") => Some(GameDirConfig::RootFolder),
        Some(value) if value.parse::<i32>().ok().is_some_and(|n| n >= 2) => {
            Some(GameDirConfig::VersionFolder)
        }
        _ => None,
    }
}

async fn read_launcher_kv(launcher_dir: &Path) -> HashMap<String, String> {
    for path in [
        launcher_dir.join("PCL").join("Setup.ini"),
        launcher_dir.join("PCL.ini"),
        launcher_dir.join("Setup.ini"),
    ] {
        let values = read_kv_file(&path).await;
        if !values.is_empty() {
            return values;
        }
    }
    HashMap::new()
}

async fn read_kv_file(path: &Path) -> HashMap<String, String> {
    let Ok((text, _)) = io::read_any_encoding_to_string(path).await else {
        return HashMap::new();
    };
    parse_kv_file(&text)
}

fn parse_kv_file(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(';')
            || line.starts_with('[')
        {
            continue;
        }
        let Some((key, value)) =
            line.split_once(':').or_else(|| line.split_once('='))
        else {
            continue;
        };
        values.insert(key.trim().to_string(), value.trim().to_string());
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pcl_setup_ini() {
        let values = parse_kv_file(
            "LaunchFolderSelect:2\r\nLaunchFolder:D:\\Games\\.minecraft\r\nLaunchArgumentIndie:2\r\n",
        );
        assert_eq!(values.get("LaunchFolderSelect").unwrap(), "2");
        assert_eq!(
            values.get("LaunchFolder").unwrap(),
            "D:\\Games\\.minecraft"
        );
        assert!(matches!(
            isolation_value(values.get("LaunchArgumentIndie")),
            Some(GameDirConfig::VersionFolder)
        ));
    }
}
