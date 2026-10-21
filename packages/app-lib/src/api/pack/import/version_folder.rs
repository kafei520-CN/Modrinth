use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use anyhow::bail;

use crate::{
    ErrorKind, State,
    event::InstancePayloadType,
    event::emit::emit_instance,
    install::{
        InstallPhaseDetails, InstallPhaseId, InstallProgress,
        InstallProgressReporter,
    },
    pack::{import, install_from::PackDependency},
    state::{
        AppliedContentSetPatch, EditInstance, InstanceInstallStage, ModLoader,
    },
    util::io,
};

#[derive(Debug, Clone)]
pub enum GameDirConfig {
    VersionFolder,
    RootFolder,
    Custom(PathBuf),
}

const EXTERNAL_LINK_FILE: &str = "external-instance.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalInstanceLink {
    pub minecraft_root: PathBuf,
    pub run_directory: PathBuf,
    pub version_id: String,
}

pub fn read_external_link(instance_dir: &Path) -> Option<ExternalInstanceLink> {
    let path = instance_dir.join(EXTERNAL_LINK_FILE);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

#[derive(Default)]
pub struct VersionImportOptions {
    pub display_name: Option<String>,
    pub icon_candidates: Vec<PathBuf>,
    pub game_dir: Option<GameDirConfig>,
}

#[derive(Deserialize)]
struct VersionJson {
    id: Option<String>,
    #[serde(rename = "inheritsFrom")]
    inherits_from: Option<String>,
    #[serde(default, rename = "clientVersion")]
    client_version: Option<String>,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
    #[serde(default)]
    libraries: Vec<serde_json::Value>,
    #[serde(default)]
    patches: Vec<VersionPatch>,
}

#[derive(Deserialize)]
struct VersionPatch {
    id: Option<String>,
    version: Option<String>,
}

pub fn is_minecraft_root(path: &Path) -> bool {
    path.join("versions").is_dir()
}

pub fn looks_isolated(version_dir: &Path) -> bool {
    ["mods", "saves", "config", "resourcepacks", "shaderpacks"]
        .iter()
        .any(|name| version_dir.join(name).is_dir())
}

pub fn resolve_game_dir(
    minecraft_root: &Path,
    version_dir: &Path,
    configured: Option<GameDirConfig>,
) -> PathBuf {
    match configured {
        Some(GameDirConfig::VersionFolder) => version_dir.to_path_buf(),
        Some(GameDirConfig::RootFolder) => minecraft_root.to_path_buf(),
        Some(GameDirConfig::Custom(path)) => path,
        None if looks_isolated(version_dir) => version_dir.to_path_buf(),
        None => minecraft_root.to_path_buf(),
    }
}

pub async fn is_valid_version_folder(instance_folder: PathBuf) -> bool {
    let Some(name) = instance_folder.file_name() else {
        return false;
    };
    let json_path =
        instance_folder.join(format!("{}.json", name.to_string_lossy()));
    let Ok((json, _)) = io::read_any_encoding_to_string(&json_path).await
    else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<VersionJson>(&json) else {
        return false;
    };
    parsed.id.is_some()
        || parsed.inherits_from.is_some()
        || !parsed.libraries.is_empty()
        || !parsed.patches.is_empty()
}

pub async fn list_version_folders(
    minecraft_root: &Path,
) -> crate::Result<Vec<String>> {
    let versions = minecraft_root.join("versions");
    let mut dir = match io::read_dir(&versions).await {
        Ok(dir) => dir,
        Err(_) => return Ok(Vec::new()),
    };
    let mut instances = Vec::new();
    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| io::IOError::with_path(e, &versions))?
    {
        let path = entry.path();
        if path.is_dir() && is_valid_version_folder(path.clone()).await {
            if let Some(name) = path.file_name() {
                instances.push(name.to_string_lossy().to_string());
            }
        }
    }
    Ok(instances)
}

pub async fn import_version_instance(
    minecraft_root: &Path,
    version_id: &str,
    instance_id: &str,
    backup_name: String,
    options: VersionImportOptions,
    reporter: InstallProgressReporter,
    details: InstallPhaseDetails,
) -> crate::Result<()> {
    let version_dir = minecraft_root.join("versions").join(version_id);
    let json_path = version_dir.join(format!("{version_id}.json"));
    let json = io::read_any_encoding_to_string(&json_path).await?.0;
    let Some(dependencies) = parse_version_dependencies(&json) else {
        anyhow::bail!(crate::ErrorKind::InputError(format!(
            "could not parse Minecraft version metadata for {version_id}"
        )));
    };

    let mut icon_candidates = options.icon_candidates;
    icon_candidates.push(version_dir.join(format!("{version_id}.png")));
    icon_candidates.push(version_dir.join("icon.png"));
    let mut icon = None;
    for candidate in icon_candidates {
        if candidate.is_file() {
            icon = import::recache_icon(candidate).await?;
            if icon.is_some() {
                break;
            }
        }
    }

    let (mod_loader, loader_version) = loader_from_dependencies(&dependencies);
    let minecraft = dependencies
        .get(&PackDependency::Minecraft)
        .cloned()
        .ok_or(ErrorKind::InputError(format!(
            "could not parse Minecraft version metadata for {version_id}"
        )))?;
    let run_directory = match options.game_dir {
        Some(GameDirConfig::Custom(path)) => path,
        Some(GameDirConfig::VersionFolder) => version_dir.clone(),
        Some(GameDirConfig::RootFolder) => minecraft_root.to_path_buf(),
        None if looks_isolated(&version_dir) => version_dir.clone(),
        None => minecraft_root.to_path_buf(),
    };
    let state = State::get().await?;
    write_external_link(
        instance_id,
        &ExternalInstanceLink {
            minecraft_root: minecraft_root.to_path_buf(),
            run_directory,
            version_id: version_id.to_string(),
        },
        &state,
        reporter,
        details,
    )
    .await?;

    crate::state::edit_instance(
        instance_id,
        EditInstance {
            install_stage: Some(InstanceInstallStage::Installed),
            name: Some(options.display_name.unwrap_or(backup_name)),
            icon_path: Some(
                icon.map(|path| path.to_string_lossy().to_string()),
            ),
            content_set_patch: Some(AppliedContentSetPatch {
                source_kind: None,
                game_version: Some(minecraft),
                protocol_version: Some(None),
                loader: Some(mod_loader),
                loader_version: Some(loader_version),
            }),
            ..EditInstance::default()
        },
        &state,
    )
    .await?;
    emit_instance(instance_id, InstancePayloadType::Edited).await?;
    Ok(())
}

fn loader_from_dependencies(
    dependencies: &HashMap<PackDependency, String>,
) -> (ModLoader, Option<String>) {
    if let Some(version) = dependencies.get(&PackDependency::NeoForge) {
        (ModLoader::NeoForge, Some(version.clone()))
    } else if let Some(version) = dependencies.get(&PackDependency::Forge) {
        (ModLoader::Forge, Some(version.clone()))
    } else if let Some(version) =
        dependencies.get(&PackDependency::FabricLoader)
    {
        (ModLoader::Fabric, Some(version.clone()))
    } else if let Some(version) = dependencies.get(&PackDependency::QuiltLoader)
    {
        (ModLoader::Quilt, Some(version.clone()))
    } else {
        (ModLoader::Vanilla, None)
    }
}

pub async fn link_version_instance(
    minecraft_root: &Path,
    version_id: &str,
    backup_name: String,
    options: VersionImportOptions,
) -> crate::Result<String> {
    let version_dir = minecraft_root.join("versions").join(version_id);
    let json_path = version_dir.join(format!("{version_id}.json"));
    let json = io::read_any_encoding_to_string(&json_path).await?.0;
    let dependencies = parse_version_dependencies(&json)
        .or_else(|| parse_sjmcl_config(&version_dir.join("sjmclcfg.json")))
        .ok_or_else(|| {
            ErrorKind::InputError(format!(
                "could not parse Minecraft version metadata for {version_id}"
            ))
        })?;
    let (mod_loader, loader_version) = loader_from_dependencies(&dependencies);
    let minecraft = dependencies
        .get(&PackDependency::Minecraft)
        .cloned()
        .ok_or(ErrorKind::InputError(format!(
            "could not parse Minecraft version metadata for {version_id}"
        )))?;
    let run_directory = match options.game_dir {
        Some(GameDirConfig::Custom(path)) => path,
        Some(GameDirConfig::VersionFolder) => version_dir.clone(),
        Some(GameDirConfig::RootFolder) => minecraft_root.to_path_buf(),
        None if looks_isolated(&version_dir) => version_dir.clone(),
        None => minecraft_root.to_path_buf(),
    };
    if !run_directory.is_dir() {
        anyhow::bail!(ErrorKind::InputError(format!(
            "game directory does not exist: {}",
            run_directory.display()
        )));
    }

    let bytes = serde_json::to_vec_pretty(&ExternalInstanceLink {
        minecraft_root: minecraft_root.to_path_buf(),
        run_directory,
        version_id: version_id.to_string(),
    })
    .map_err(|err| {
        ErrorKind::InputError(format!("failed to write instance link: {err}"))
    })?;
    let metadata = crate::api::instance::create_linked(
        options.display_name.unwrap_or(backup_name),
        minecraft,
        mod_loader,
        loader_version,
        bytes,
    )
    .await?;
    Ok(metadata.instance.id)
}

fn clear_placeholder_dirs(stub: &Path) {
    for name in [
        "mods",
        "saves",
        "config",
        "resourcepacks",
        "shaderpacks",
        "datapacks",
        "screenshots",
        "crash-reports",
    ] {
        let path = stub.join(name);
        if path.is_dir() && directory_is_empty(&path) {
            let _ = std::fs::remove_dir(&path);
        }
    }
}

fn directory_is_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_none()
}

async fn write_external_link(
    instance_id: &str,
    link: &ExternalInstanceLink,
    state: &State,
    reporter: InstallProgressReporter,
    details: InstallPhaseDetails,
) -> crate::Result<()> {
    if !link.run_directory.is_dir() {
        bail!(ErrorKind::InputError(format!(
            "game directory does not exist: {}",
            link.run_directory.display()
        )));
    }

    reporter
        .update(
            InstallPhaseId::PreparingInstance,
            Some(InstallProgress {
                current: 0,
                total: 1,
                secondary: None,
            }),
            details.clone(),
        )
        .await?;

    let metadata = crate::api::instance::get(instance_id).await?.ok_or(
        ErrorKind::InputError(
            "imported instance could not be loaded".to_string(),
        ),
    )?;
    let instance_dir = state
        .directories
        .instances_dir()
        .join(&metadata.instance.path);
    io::create_dir_all(&instance_dir).await?;
    let bytes = serde_json::to_vec_pretty(link).map_err(|err| {
        ErrorKind::InputError(format!("failed to write instance link: {err}"))
    })?;
    io::write(instance_dir.join(EXTERNAL_LINK_FILE), bytes).await?;
    clear_placeholder_dirs(&instance_dir);

    reporter
        .update(
            InstallPhaseId::Finalizing,
            Some(InstallProgress {
                current: 1,
                total: 1,
                secondary: None,
            }),
            details,
        )
        .await?;
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn skip_version_import_relative(relative: &str) -> bool {
    let relative = relative.replace('\\', "/");
    let first = relative.split('/').next().unwrap_or(&relative);

    if first.eq_ignore_ascii_case("libraries")
        || first.eq_ignore_ascii_case("assets")
        || first.eq_ignore_ascii_case("versions")
        || first.eq_ignore_ascii_case("natives")
        || first.eq_ignore_ascii_case("logs")
        || first.eq_ignore_ascii_case("crash-reports")
        || first.eq_ignore_ascii_case("PCL")
        || first.eq_ignore_ascii_case(".fabric")
        || first.eq_ignore_ascii_case(".mixin.out")
        || first.starts_with("natives")
    {
        return true;
    }

    if !relative.contains('/') {
        let lower = first.to_ascii_lowercase();
        if lower.ends_with(".jar") {
            return true;
        }
        if lower.ends_with(".json")
            && lower != "usernamecache.json"
            && lower != "usercache.json"
        {
            return true;
        }
        if matches!(
            lower.as_str(),
            "pcl.ini" | "hmclversion.cfg" | "hmclversion.json"
        ) {
            return true;
        }
    }

    false
}

fn parse_sjmcl_config(path: &Path) -> Option<HashMap<PackDependency, String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let minecraft = value.get("version")?.as_str()?;
    if !is_pure_mc_version(minecraft)
        && minecraft_from_label(minecraft).is_none()
    {
        return None;
    }
    let minecraft = minecraft_from_label(minecraft)?;
    let mut dependencies = HashMap::new();
    dependencies.insert(PackDependency::Minecraft, minecraft);
    if let Some(loader) = value.get("modLoader") {
        let kind = loader.get("loaderType").and_then(|item| item.as_str());
        let version = loader
            .get("version")
            .and_then(|item| item.as_str())
            .map(str::to_string);
        let dependency = match kind {
            Some("NeoForge") => Some(PackDependency::NeoForge),
            Some("Forge") => Some(PackDependency::Forge),
            Some("Fabric") => Some(PackDependency::FabricLoader),
            Some("Quilt") => Some(PackDependency::QuiltLoader),
            _ => None,
        };
        if let (Some(dependency), Some(version)) = (dependency, version) {
            dependencies.insert(dependency, version);
        }
    }
    Some(dependencies)
}

pub(crate) fn parse_version_dependencies(
    json: &str,
) -> Option<HashMap<PackDependency, String>> {
    let parsed: VersionJson = serde_json::from_str(json).ok()?;
    let (minecraft, loader) = minecraft_and_loader(&parsed)?;
    let mut dependencies = HashMap::new();
    dependencies.insert(PackDependency::Minecraft, minecraft);
    if let Some((dep, version)) = loader {
        dependencies.insert(dep, version);
    }
    Some(dependencies)
}

fn minecraft_and_loader(
    parsed: &VersionJson,
) -> Option<(String, Option<(PackDependency, String)>)> {
    let mut loader = loader_from_patches(&parsed.patches)
        .or_else(|| loader_from_fml_arguments(parsed))
        .or_else(|| loader_from_libraries(&parsed.libraries));

    let minecraft = fml_argument(parsed, "--fml.mcVersion")
        .as_deref()
        .and_then(minecraft_from_label)
        .or_else(|| {
            parsed
                .client_version
                .as_deref()
                .and_then(minecraft_from_label)
        })
        .or_else(|| {
            parsed
                .patches
                .iter()
                .find(|patch| {
                    patch
                        .id
                        .as_deref()
                        .is_some_and(|id| id.eq_ignore_ascii_case("game"))
                })
                .and_then(|patch| patch.version.as_deref())
                .and_then(minecraft_from_label)
        })
        .or_else(|| {
            parsed
                .inherits_from
                .as_deref()
                .and_then(minecraft_from_label)
        })
        .or_else(|| parsed.id.as_deref().and_then(minecraft_from_label))
        .or_else(|| {
            loader
                .as_ref()
                .and_then(|(_, version)| minecraft_from_label(version))
        })
        .or_else(|| {
            loader.as_ref().and_then(|(dep, version)| {
                (*dep == PackDependency::NeoForge)
                    .then(|| minecraft_from_neoforge(version))
                    .flatten()
            })
        })?;

    if let Some((PackDependency::Forge | PackDependency::NeoForge, version)) =
        loader.as_mut()
        && let Some(stripped) = version.strip_prefix(&format!("{minecraft}-"))
    {
        *version = stripped.to_string();
    }

    Some((minecraft, loader))
}

fn loader_from_fml_arguments(
    parsed: &VersionJson,
) -> Option<(PackDependency, String)> {
    if let Some(version) = fml_argument(parsed, "--fml.neoForgeVersion") {
        return Some((PackDependency::NeoForge, version));
    }
    if let Some(version) = fml_argument(parsed, "--fml.forgeVersion") {
        return Some((PackDependency::Forge, version));
    }
    None
}

fn fml_argument(parsed: &VersionJson, flag: &str) -> Option<String> {
    let game = parsed.arguments.as_ref()?.get("game")?.as_array()?;
    let mut items = game.iter();
    while let Some(item) = items.next() {
        if item.as_str() == Some(flag) {
            return items
                .next()
                .and_then(|value| value.as_str())
                .map(str::to_string);
        }
    }
    None
}

fn minecraft_from_neoforge(version: &str) -> Option<String> {
    let mut parts = version.split(['.', '-']);
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    if !(20..=30).contains(&major) {
        return None;
    }
    if minor == 0 {
        Some(format!("1.{major}"))
    } else {
        Some(format!("1.{major}.{minor}"))
    }
}

fn minecraft_from_label(value: &str) -> Option<String> {
    if is_pure_mc_version(value) {
        return Some(value.to_string());
    }
    let prefix = value.split('-').next()?;
    is_pure_mc_version(prefix).then(|| prefix.to_string())
}

fn is_pure_mc_version(value: &str) -> bool {
    is_release_mc_version(value) || is_snapshot_mc_version(value)
}

fn is_release_mc_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    if major.is_empty() || !major.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Ok(major_n) = major.parse::<u32>() else {
        return false;
    };
    if major_n != 1 && major_n < 26 {
        return false;
    }
    let Some(minor) = parts.next() else {
        return false;
    };
    if minor.is_empty() || !minor.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(patch) => {
            !patch.is_empty()
                && patch.chars().all(|c| c.is_ascii_digit())
                && parts.next().is_none()
        }
    }
}

fn is_snapshot_mc_version(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() == 6
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b'w'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_lowercase()
    {
        return true;
    }
    let Some((release, rest)) = value.split_once("-snapshot-") else {
        return false;
    };
    is_release_mc_version(release)
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit())
}

fn loader_from_patches(
    patches: &[VersionPatch],
) -> Option<(PackDependency, String)> {
    for (ids, dep) in [
        (&["neoforge", "neo-forge"][..], PackDependency::NeoForge),
        (&["forge"], PackDependency::Forge),
        (&["fabric", "fabric-loader"], PackDependency::FabricLoader),
        (&["quilt", "quilt-loader"], PackDependency::QuiltLoader),
    ] {
        if let Some(version) = patches.iter().find_map(|patch| {
            let id = patch.id.as_deref()?;
            ids.iter()
                .any(|candidate| id.eq_ignore_ascii_case(candidate))
                .then(|| patch.version.clone())
                .flatten()
        }) {
            return Some((dep, version));
        }
    }
    None
}

fn loader_from_libraries(
    libraries: &[serde_json::Value],
) -> Option<(PackDependency, String)> {
    let mut found = None;
    for library in libraries {
        let Some(name) = library.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some((group, artifact, version)) = split_maven_name(name) else {
            continue;
        };
        let candidate = if group == "net.neoforged"
            && (artifact == "neoforge" || artifact == "fmlloader")
        {
            Some((PackDependency::NeoForge, version.to_string()))
        } else if group == "net.minecraftforge"
            && (artifact == "forge" || artifact == "fmlloader")
        {
            Some((PackDependency::Forge, version.to_string()))
        } else if group == "net.fabricmc" && artifact == "fabric-loader" {
            Some((PackDependency::FabricLoader, version.to_string()))
        } else if group == "org.quiltmc" && artifact == "quilt-loader" {
            Some((PackDependency::QuiltLoader, version.to_string()))
        } else {
            None
        };
        if let Some(candidate) = candidate {
            found = Some(candidate);
        }
    }
    found
}

fn split_maven_name(name: &str) -> Option<(&str, &str, &str)> {
    let mut parts = name.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    Some((group, artifact, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fabric_version_json() {
        let json = r#"{
            "id": "fabric-loader-0.16.9-1.21.1",
            "inheritsFrom": "1.21.1",
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.16.9"}]
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Minecraft).unwrap(), "1.21.1");
        assert_eq!(deps.get(&PackDependency::FabricLoader).unwrap(), "0.16.9");
    }

    #[test]
    fn parses_hmcl_forge_patches() {
        let json = r#"{
            "id": "1.20.1",
            "patches": [
                {"id": "game", "version": "1.20.1"},
                {"id": "forge", "version": "47.2.0"}
            ]
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Minecraft).unwrap(), "1.20.1");
        assert_eq!(deps.get(&PackDependency::Forge).unwrap(), "47.2.0");
    }

    #[test]
    fn strips_minecraft_prefix_from_forge_library() {
        let json = r#"{
            "id": "1.20.1-forge-47.2.0",
            "inheritsFrom": "1.20.1",
            "libraries": [{"name": "net.minecraftforge:forge:1.20.1-47.2.0"}]
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Forge).unwrap(), "47.2.0");
    }

    #[test]
    fn parses_gtnh_style_named_forge_instance() {
        let json = r#"{
            "id": "-=GTNH-2.8.4=-",
            "libraries": [{"name": "net.minecraftforge:forge:1.7.10-10.13.4.1614"}]
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Minecraft).unwrap(), "1.7.10");
        assert_eq!(deps.get(&PackDependency::Forge).unwrap(), "10.13.4.1614");
    }

    #[test]
    fn parses_minecraft_version_from_forge_inherits_from() {
        let json = r#"{
            "id": "-=GTNH-2.8.4=-",
            "inheritsFrom": "1.7.10-Forge10.13.4.1614-1.7.10"
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Minecraft).unwrap(), "1.7.10");
    }

    #[test]
    fn rejects_pack_folder_name_as_minecraft_version() {
        let json = r#"{ "id": "-=GTNH-2.8.4=-" }"#;
        assert!(parse_version_dependencies(json).is_none());
    }

    #[test]
    fn parses_modern_minecraft_26_versions() {
        let json = r#"{
            "id": "26.1.2-Fabric 0.19.3",
            "inheritsFrom": "26.1.2",
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.19.3"}]
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Minecraft).unwrap(), "26.1.2");
        assert_eq!(deps.get(&PackDependency::FabricLoader).unwrap(), "0.19.3");
    }

    #[test]
    fn parses_merged_neoforge_arguments() {
        let json = r#"{
            "id": "NSUK轻量建筑包",
            "arguments": {
                "game": ["--fml.neoForgeVersion", "21.1.233", "--fml.mcVersion", "1.21.1"]
            },
            "clientVersion": "1.21.1"
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Minecraft).unwrap(), "1.21.1");
        assert_eq!(deps.get(&PackDependency::NeoForge).unwrap(), "21.1.233");
    }

    #[test]
    fn parses_neoforge_21_1_as_minecraft_1_21_1() {
        let json = r#"{
            "id": "NSUK轻量建筑包",
            "libraries": [{"name": "net.neoforged:neoforge:21.1.233"}]
        }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(deps.get(&PackDependency::Minecraft).unwrap(), "1.21.1");
        assert_eq!(deps.get(&PackDependency::NeoForge).unwrap(), "21.1.233");
    }

    #[test]
    fn parses_modern_snapshot_id() {
        let json = r#"{ "id": "26.3-snapshot-8" }"#;
        let deps = parse_version_dependencies(json).unwrap();
        assert_eq!(
            deps.get(&PackDependency::Minecraft).unwrap(),
            "26.3-snapshot-8"
        );
    }

    #[test]
    fn skips_launcher_and_library_paths() {
        assert!(skip_version_import_relative("libraries/foo.jar"));
        assert!(skip_version_import_relative("assets/indexes/1.20.json"));
        assert!(skip_version_import_relative("versions/1.20.1/1.20.1.json"));
        assert!(skip_version_import_relative("1.20.1.jar"));
        assert!(skip_version_import_relative("1.20.1.json"));
        assert!(skip_version_import_relative("PCL.ini"));
        assert!(skip_version_import_relative("natives-windows-x86_64/x.dll"));
        assert!(!skip_version_import_relative("mods/sodium.jar"));
        assert!(!skip_version_import_relative("saves/world/level.dat"));
        assert!(!skip_version_import_relative("options.txt"));
        assert!(!skip_version_import_relative("usernamecache.json"));
    }
}
