use crate::ErrorKind;
use crate::state::State;
use crate::state::instances::adapters::sqlite::instance_rows;
use crate::util::io;
use anyhow::bail;

pub(crate) async fn remove_instance(
    instance_id: &str,
    state: &State,
) -> crate::Result<()> {
    finish_instance_removal(instance_id, state, true).await
}

/// Removes the instance from the library and leaves the game files in place.
/// An external import stub is removed because it only holds the pointer.
pub(crate) async fn release_instance(
    instance_id: &str,
    state: &State,
) -> crate::Result<()> {
    finish_instance_removal(instance_id, state, false).await
}

async fn finish_instance_removal(
    instance_id: &str,
    state: &State,
    delete_files: bool,
) -> crate::Result<()> {
    let instance = instance_rows::get_instance_by_id(instance_id, &state.pool)
        .await?
        .ok_or(crate::ErrorKind::InputError("unknown instance".to_string()))?;
    let _synced_options_lock = state.synced_options.lock().await;
    let _content_lock = state.instance_locks.lock_content(instance_id).await;
    let _store_lock = state.content_store.files_lock.lock().await;
    let _store_lease = state.content_store.lease().await;
    if crate::state::instance_has_running_process(instance_id, state).await? {
        bail!(ErrorKind::InputError(
            "stop this instance before removing it".to_string(),
        ));
    }
    state.content_store.recover(Some(instance_id)).await?;
    crate::api::instance::remove_generated_instance_files(instance_id, state)
        .await?;

    delete_instance_row_and_locks(&instance.id, state).await?;

    let path = state.directories.instances_dir().join(&instance.path);
    if delete_files || is_launcher_stub(&path) {
        remove_instance_directory(&path).await?;
    }

    Ok(())
}

fn is_launcher_stub(path: &std::path::Path) -> bool {
    path.join("external-instance.json").is_file() || io::is_directory_link(path)
}

async fn remove_instance_directory(
    path: &std::path::Path,
) -> crate::Result<()> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(io::IOError::with_path(err, path).into());
        }
    };
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        if tokio::fs::remove_dir(path).await.is_err() {
            tokio::fs::remove_file(path)
                .await
                .map_err(|err| io::IOError::with_path(err, path))?;
        }
        return Ok(());
    }
    io::remove_dir_all(path).await?;
    Ok(())
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

async fn delete_instance_row_and_locks(
    instance_id: &str,
    state: &State,
) -> crate::Result<()> {
    // Keep these together so deleted instances cannot leave stale entries in the per-instance lock maps.
    instance_rows::delete_instance_by_id(instance_id, &state.pool).await?;
    state.instance_locks.remove(instance_id);

    Ok(())
}
