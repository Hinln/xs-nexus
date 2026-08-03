use std::{io, path::Path};

use axum::{
    body::{Body, Bytes},
    extract::{Path as AxumPath, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::stream;
use tokio::io::AsyncReadExt as _;

use crate::{error::ApiError, state::AppState};

pub(crate) const RELEASE_FILES: [&str; 7] = [
    "release-public-key.pem",
    "xs-nexus-0.1.0-x86_64-unknown-linux-gnu.manifest",
    "xs-nexus-0.1.0-x86_64-unknown-linux-gnu.manifest.sig",
    "xs-nexus-0.1.0-x86_64-unknown-linux-gnu.tar.gz",
    "xs-nexus-0.1.0-aarch64-unknown-linux-gnu.manifest",
    "xs-nexus-0.1.0-aarch64-unknown-linux-gnu.manifest.sig",
    "xs-nexus-0.1.0-aarch64-unknown-linux-gnu.tar.gz",
];

const INSTALL_SCRIPT: &str = include_str!("../../../installers/linux/xs-nexus-one-click.sh");
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) async fn install_script(State(state): State<AppState>) -> Result<Response, ApiError> {
    if state.linux_release_directory.is_none() {
        return Err(ApiError::unavailable());
    }
    let mut response = INSTALL_SCRIPT.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/x-shellscript; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    headers.insert(
        header::HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

pub(crate) async fn release_file(
    State(state): State<AppState>,
    AxumPath(file_name): AxumPath<String>,
) -> Result<Response, ApiError> {
    if !RELEASE_FILES.contains(&file_name.as_str()) {
        return Err(ApiError::not_found());
    }
    let directory = state
        .linux_release_directory
        .as_deref()
        .ok_or_else(ApiError::unavailable)?;
    let path = directory.join(&file_name);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::unavailable())?;
    let metadata = file.metadata().await.map_err(|_| ApiError::unavailable())?;
    if !metadata.is_file() || !valid_release_file_size(&file_name, metadata.len()) {
        return Err(ApiError::unavailable());
    }

    let stream = stream::try_unfold(file, |mut file| async move {
        let mut chunk = vec![0_u8; 64 * 1024];
        let read = file.read(&mut chunk).await?;
        if read == 0 {
            Ok::<_, io::Error>(None)
        } else {
            chunk.truncate(read);
            Ok(Some((Bytes::from(chunk), file)))
        }
    });
    let content_type = match Path::new(&file_name)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("gz") => "application/gzip",
        Some("sig") => "application/octet-stream",
        _ => "text/plain; charset=utf-8",
    };
    let disposition = HeaderValue::try_from(format!("attachment; filename=\"{file_name}\""))
        .map_err(|_| ApiError::internal())?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, metadata.len())
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CACHE_CONTROL, "public, max-age=3600, immutable")
        .header("x-content-type-options", "nosniff")
        .body(Body::from_stream(stream))
        .map_err(|_| ApiError::internal())
}

pub(crate) fn validate_release_directory(directory: &Path) -> io::Result<()> {
    let metadata = directory.symlink_metadata()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() || !directory.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "release directory must be an absolute non-symlink directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = metadata.permissions().mode();
        if mode & 0o022 != 0 || mode & 0o005 != 0o005 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "release directory permissions are unsafe",
            ));
        }
    }
    for file_name in RELEASE_FILES {
        let path = directory.join(file_name);
        let metadata = path.symlink_metadata()?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || !valid_release_file_size(file_name, metadata.len())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "release file is invalid",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = metadata.permissions().mode();
            if mode & 0o022 != 0 || mode & 0o004 == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "release file permissions are unsafe",
                ));
            }
        }
    }
    Ok(())
}

fn valid_release_file_size(file_name: &str, size: u64) -> bool {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("pem") if file_name == "release-public-key.pem" => (1..=8_192).contains(&size),
        Some("manifest") => (1..=4_096).contains(&size),
        Some("sig") => size == 64,
        Some("gz") => (1..=MAX_ARCHIVE_BYTES).contains(&size),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_is_fixed_to_the_production_controller() {
        assert!(INSTALL_SCRIPT.contains("readonly CONTROLLER_URL='https://vpn.qinwen.co/'"));
        assert!(
            INSTALL_SCRIPT.contains(
                "readonly RELEASE_BASE_URL='https://vpn.qinwen.co/downloads/linux/stable'"
            )
        );
        assert!(INSTALL_SCRIPT.contains("IFS= read -r -s token </dev/tty"));
        assert!(!INSTALL_SCRIPT.contains("--token "));
        assert!(!INSTALL_SCRIPT.contains("Liyunran"));
    }

    #[test]
    fn release_allowlist_is_complete_for_both_architectures() {
        assert_eq!(RELEASE_FILES.len(), 7);
        assert!(RELEASE_FILES.iter().any(|name| name.contains("x86_64")));
        assert!(RELEASE_FILES.iter().any(|name| name.contains("aarch64")));
        assert!(!valid_release_file_size("unexpected", 1));
    }
}
