//! Immutable, explicit review batches for an external agent runner.
//!
//! No agent is launched here. The Markdown is the existing clipboard export;
//! only the small versioned envelope is new. See docs/CONFIG.md#agent-queue.
use std::ffi::OsString;
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, DirBuilder};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

pub const MAX_MARKDOWN_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ENVELOPE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Serialize)]
struct Batch<'a> {
    version: u8,
    id: &'a str,
    markdown: &'a str,
}

/// A published batch stays queued even if the subsequent directory sync fails.
pub struct QueuedBatch {
    pub id: String,
    pub durability_warning: Option<io::Error>,
}

/// Environment > config > platform state directory. Resolve lazily so invalid
/// agent settings cannot prevent ordinary review/clipboard use.
pub fn resolve_queue_dir(configured: Option<&Path>) -> io::Result<PathBuf> {
    resolve_queue_dir_with(configured, |key| std::env::var_os(key))
}

fn absolute_path(label: &str, value: impl Into<PathBuf>) -> io::Result<PathBuf> {
    let path = value.into();
    if !path.is_absolute() || path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} must be a nonempty absolute path (no ~ expansion)"),
        ));
    }
    Ok(path)
}

fn resolve_queue_dir_with(
    configured: Option<&Path>,
    env: impl Fn(&str) -> Option<OsString>,
) -> io::Result<PathBuf> {
    if let Some(value) = env("TUICR_AGENT_QUEUE_DIR") {
        return absolute_path("TUICR_AGENT_QUEUE_DIR", value);
    }
    if let Some(value) = configured {
        return absolute_path("agent_queue_dir", value);
    }
    let required = |key| {
        env(key)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{key} is not set; configure TUICR_AGENT_QUEUE_DIR"),
                )
            })
            .and_then(|value| absolute_path(key, value))
    };
    #[cfg(target_os = "windows")]
    let state = required("LOCALAPPDATA")?;
    #[cfg(target_os = "macos")]
    let state = required("HOME")?.join("Library/Application Support");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let state = match env("XDG_STATE_HOME") {
        Some(value) => absolute_path("XDG_STATE_HOME", value)?,
        None => required("HOME")?.join(".local/state"),
    };
    Ok(state.join("tuicr/agent-queue"))
}

// Sync new directory entries before publishing any job. Syncing only the
// final pending directory would not durably create the spool on first use.
fn create_directories(path: &Path) -> io::Result<()> {
    if path.try_exists()? {
        return Ok(());
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Queue directory has no parent")
    })?;
    create_directories(parent)?;
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
        Err(error) => return Err(error),
    }
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn private_directory(path: &Path) -> io::Result<()> {
    create_directories(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Queue path must be a directory, not a file or symlink: {}",
                path.display()
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no preconditions and does not mutate process state.
        if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Queue directory must be owned by you and private (0700): {}",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

/// Write privately, sync, then atomically publish without replacing any batch.
/// Failures before publication leave no visible JSON; stale temp files are not jobs.
pub fn enqueue(queue: &Path, markdown: &str) -> io::Result<QueuedBatch> {
    absolute_path("agent queue", queue)?;
    if markdown.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent batch Markdown must not be empty",
        ));
    }
    if markdown.len() > MAX_MARKDOWN_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Review exceeds agent batch limit (8 MiB Markdown)",
        ));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let content = serde_json::to_vec(&Batch {
        version: 1,
        id: &id,
        markdown,
    })?;
    if content.len() > MAX_ENVELOPE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Review exceeds agent batch limit (16 MiB JSON)",
        ));
    }
    private_directory(queue)?;
    let pending = queue.join("pending");
    private_directory(&pending)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(&format!(".{id}."))
        .suffix(".tmp")
        .tempfile_in(&pending)?;
    temporary.write_all(&content)?;
    temporary.as_file().sync_all()?;
    // persist_noclobber publishes atomically (link+unlink on Unix), unlike a
    // check-then-rename that could replace an existing file on a collision.
    temporary
        .persist_noclobber(pending.join(format!("{id}.json")))
        .map_err(|err| err.error)?;
    #[cfg(unix)]
    let durability_warning = File::open(&pending)
        .and_then(|directory| directory.sync_all())
        .err();
    #[cfg(not(unix))]
    let durability_warning = None;
    Ok(QueuedBatch {
        id,
        durability_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_overrides_config_without_mutating_process_environment() {
        let base = tempfile::tempdir().unwrap();
        let configured = base.path().join("config");
        let env_path = base.path().join("env");
        let path = resolve_queue_dir_with(Some(&configured), |key| {
            (key == "TUICR_AGENT_QUEUE_DIR").then(|| env_path.clone().into_os_string())
        })
        .unwrap();
        assert_eq!(path, env_path);
        assert_eq!(
            resolve_queue_dir_with(Some(&configured), |_| None).unwrap(),
            configured
        );
        for bad in ["", "relative", "~/queue", "/bad\0path"] {
            assert!(
                resolve_queue_dir_with(None, |key| (key == "TUICR_AGENT_QUEUE_DIR")
                    .then(|| bad.into()))
                .is_err()
            );
        }
        assert!(resolve_queue_dir_with(Some(Path::new("relative")), |_| None).is_err());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn linux_default_and_xdg_match_runner() {
        assert_eq!(
            resolve_queue_dir_with(None, |key| (key == "HOME").then(|| "/home/me".into())).unwrap(),
            Path::new("/home/me/.local/state/tuicr/agent-queue")
        );
        assert_eq!(
            resolve_queue_dir_with(None, |key| (key == "XDG_STATE_HOME")
                .then(|| "/state".into()))
            .unwrap(),
            Path::new("/state/tuicr/agent-queue")
        );
        assert!(
            resolve_queue_dir_with(None, |key| (key == "XDG_STATE_HOME").then(|| "".into()))
                .is_err()
        );
        assert!(resolve_queue_dir_with(None, |_| None).is_err());
    }

    #[test]
    fn published_envelope_preserves_exact_utf8_and_has_unique_id() {
        let temp = tempfile::tempdir().unwrap();
        let queue = temp.path().join("queue");
        let markdown = "Reviewing commit abc\n\n[ISSUE] old:1-2 — café\r\n\"quoted\"\t\\\n";
        let first = enqueue(&queue, markdown).unwrap();
        let second = enqueue(&queue, markdown).unwrap();
        assert_ne!(first.id, second.id);
        assert!(first.durability_warning.is_none());
        let files: Vec<_> = fs::read_dir(queue.join("pending"))
            .unwrap()
            .map(|f| f.unwrap().path())
            .collect();
        assert_eq!(files.len(), 2);
        for file in files {
            let envelope: serde_json::Value =
                serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
            assert_eq!(envelope["version"], 1);
            assert_eq!(envelope["id"], file.file_stem().unwrap().to_str().unwrap());
            assert_eq!(
                envelope["markdown"].as_str().unwrap().as_bytes(),
                markdown.as_bytes()
            );
            let id = envelope["id"].as_str().unwrap();
            assert_eq!(id.len(), 32);
            assert!(
                id.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(file).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [&queue, &queue.join("pending")] {
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o700
                );
            }
        }
    }

    #[test]
    fn oversize_payloads_fail_before_creating_spool() {
        let temp = tempfile::tempdir().unwrap();
        let queue = temp.path().join("queue");
        assert!(enqueue(&queue, "").is_err());
        assert!(enqueue(&queue, &"a".repeat(MAX_MARKDOWN_BYTES + 1)).is_err());
        // Control bytes expand to six-byte JSON escapes, independently hitting the envelope cap.
        assert!(enqueue(&queue, &"\0".repeat(MAX_MARKDOWN_BYTES / 2)).is_err());
        assert!(!queue.exists());
    }

    #[test]
    fn files_are_not_accepted_as_queue_directories() {
        let temp = tempfile::tempdir().unwrap();
        let queue = temp.path().join("queue");
        fs::write(&queue, "not a directory").unwrap();
        assert!(enqueue(&queue, "text").is_err());
        assert_eq!(fs::read_to_string(queue).unwrap(), "not a directory");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_private_symlink_and_unwritable_directories() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let queue = temp.path().join("queue");
        fs::create_dir(&queue).unwrap();
        fs::set_permissions(&queue, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            enqueue(&queue, "text").err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
        fs::set_permissions(&queue, fs::Permissions::from_mode(0o700)).unwrap();
        let alias = temp.path().join("alias");
        symlink(&queue, &alias).unwrap();
        assert!(enqueue(&alias, "text").is_err());
        symlink(temp.path(), queue.join("pending")).unwrap();
        assert!(enqueue(&queue, "text").is_err());
        fs::remove_file(queue.join("pending")).unwrap();
        fs::set_permissions(&queue, fs::Permissions::from_mode(0o500)).unwrap();
        // Root can write despite mode bits; ordinary user tests exercise the real OS failure.
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(
                enqueue(&queue, "text").err().unwrap().kind(),
                io::ErrorKind::PermissionDenied
            );
        }
        fs::set_permissions(&queue, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!queue.join("pending").exists());
    }
}
