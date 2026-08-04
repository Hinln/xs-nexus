use std::{io, path::Path};

use axum::{
    body::{Body, Bytes},
    extract::{Path as AxumPath, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::stream;
use tokio::io::AsyncReadExt as _;

use crate::{error::ApiError, state::AppState};

pub(crate) const LINUX_RELEASE_FILES: [&str; 7] = [
    "release-public-key.pem",
    "xs-nexus-0.1.0-x86_64-unknown-linux-gnu.manifest",
    "xs-nexus-0.1.0-x86_64-unknown-linux-gnu.manifest.sig",
    "xs-nexus-0.1.0-x86_64-unknown-linux-gnu.tar.gz",
    "xs-nexus-0.1.0-aarch64-unknown-linux-gnu.manifest",
    "xs-nexus-0.1.0-aarch64-unknown-linux-gnu.manifest.sig",
    "xs-nexus-0.1.0-aarch64-unknown-linux-gnu.tar.gz",
];

pub(crate) const WINDOWS_RELEASE_FILES: [&str; 3] = [
    "install.ps1",
    "xs-nexus-0.1.0-x86_64-pc-windows-msvc.manifest.json",
    "xs-nexus-0.1.0-x86_64-pc-windows-msvc.zip",
];

const INSTALL_SCRIPT: &str = include_str!("../../../installers/linux/xs-nexus-one-click.sh");
const WINDOWS_INSTALL_SCRIPT: &str = "install.ps1";
const MAX_LINUX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_WINDOWS_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) async fn install_script(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if requests_windows_install(&headers) {
        return windows_install_script_for(&state).await;
    }
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

pub(crate) async fn windows_install_script(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    windows_install_script_for(&state).await
}

async fn windows_install_script_for(state: &AppState) -> Result<Response, ApiError> {
    let directory = state
        .windows_release_directory
        .as_deref()
        .ok_or_else(ApiError::unavailable)?;
    let path = directory.join(WINDOWS_INSTALL_SCRIPT);
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| ApiError::unavailable())?;
    if !metadata.is_file()
        || !valid_windows_release_file_size(WINDOWS_INSTALL_SCRIPT, metadata.len())
    {
        return Err(ApiError::unavailable());
    }
    let script = tokio::fs::read_to_string(path)
        .await
        .map_err(|_| ApiError::unavailable())?;
    if script.contains("__RELEASE_MANIFEST_SHA256__") {
        return Err(ApiError::unavailable());
    }
    let mut response = script.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
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

pub(crate) async fn linux_release_file(
    State(state): State<AppState>,
    AxumPath(file_name): AxumPath<String>,
) -> Result<Response, ApiError> {
    let directory = state
        .linux_release_directory
        .as_deref()
        .ok_or_else(ApiError::unavailable)?;
    release_file_from_directory(
        directory,
        &file_name,
        &LINUX_RELEASE_FILES,
        valid_linux_release_file_size,
    )
    .await
}

pub(crate) async fn windows_release_file(
    State(state): State<AppState>,
    AxumPath(file_name): AxumPath<String>,
) -> Result<Response, ApiError> {
    let directory = state
        .windows_release_directory
        .as_deref()
        .ok_or_else(ApiError::unavailable)?;
    release_file_from_directory(
        directory,
        &file_name,
        &WINDOWS_RELEASE_FILES,
        valid_windows_release_file_size,
    )
    .await
}

async fn release_file_from_directory(
    directory: &Path,
    file_name: &str,
    allowlist: &[&str],
    valid_size: fn(&str, u64) -> bool,
) -> Result<Response, ApiError> {
    if !allowlist.contains(&file_name) {
        return Err(ApiError::not_found());
    }
    let path = directory.join(file_name);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::unavailable())?;
    let metadata = file.metadata().await.map_err(|_| ApiError::unavailable())?;
    if !metadata.is_file() || !valid_size(file_name, metadata.len()) {
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
        Some("zip") => "application/zip",
        Some("json") => "application/json",
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
    validate_release_files(
        directory,
        &LINUX_RELEASE_FILES,
        valid_linux_release_file_size,
    )
}

pub(crate) fn validate_windows_release_directory(directory: &Path) -> io::Result<()> {
    validate_release_files(
        directory,
        &WINDOWS_RELEASE_FILES,
        valid_windows_release_file_size,
    )
}

fn validate_release_files(
    directory: &Path,
    allowlist: &[&str],
    valid_size: fn(&str, u64) -> bool,
) -> io::Result<()> {
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
    for file_name in allowlist {
        let path = directory.join(file_name);
        let metadata = path.symlink_metadata()?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || !valid_size(file_name, metadata.len())
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

fn valid_linux_release_file_size(file_name: &str, size: u64) -> bool {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("pem") if file_name == "release-public-key.pem" => (1..=8_192).contains(&size),
        Some("manifest") => (1..=4_096).contains(&size),
        Some("sig") => size == 64,
        Some("gz") => (1..=MAX_LINUX_ARCHIVE_BYTES).contains(&size),
        _ => false,
    }
}

fn valid_windows_release_file_size(file_name: &str, size: u64) -> bool {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("ps1") if file_name == WINDOWS_INSTALL_SCRIPT => (1..=256 * 1024).contains(&size),
        Some("json") if file_name.ends_with(".manifest.json") => (1..=8_192).contains(&size),
        Some("zip") => (1..=MAX_WINDOWS_ARCHIVE_BYTES).contains(&size),
        _ => false,
    }
}

fn requests_windows_install(headers: &HeaderMap) -> bool {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("PowerShell"))
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
        assert_eq!(LINUX_RELEASE_FILES.len(), 7);
        assert!(
            LINUX_RELEASE_FILES
                .iter()
                .any(|name| name.contains("x86_64"))
        );
        assert!(
            LINUX_RELEASE_FILES
                .iter()
                .any(|name| name.contains("aarch64"))
        );
        assert!(!valid_linux_release_file_size("unexpected", 1));
    }

    #[test]
    fn windows_release_allowlist_is_fixed_and_bounded() {
        assert_eq!(WINDOWS_RELEASE_FILES.len(), 3);
        assert!(WINDOWS_RELEASE_FILES.contains(&WINDOWS_INSTALL_SCRIPT));
        assert!(WINDOWS_RELEASE_FILES.iter().any(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        }));
        assert!(valid_windows_release_file_size(WINDOWS_INSTALL_SCRIPT, 1));
        assert!(!valid_windows_release_file_size("other.ps1", 1));
        assert!(!valid_windows_release_file_size("unexpected", 1));
    }

    #[test]
    fn power_shell_user_agents_select_the_windows_bootstrap() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            HeaderValue::from_static("WindowsPowerShell/5.1"),
        );
        assert!(requests_windows_install(&headers));
        headers.insert(header::USER_AGENT, HeaderValue::from_static("curl/8.0"));
        assert!(!requests_windows_install(&headers));
    }
}
