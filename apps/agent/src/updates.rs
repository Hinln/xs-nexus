use std::{
    fs::{File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey, pkcs8::DecodePublicKey};
use futures_util::{Stream, StreamExt as _};
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use uuid::Uuid;
use xs_core::{
    LinuxReleaseManifest, MAX_UPDATE_ARCHIVE_BYTES, ReleaseVersion, UpdateChannel, UpdateDecision,
    UpdateDirective,
};

use crate::{
    config::AgentConfig,
    storage::{ensure_private_directory, read_json, write_json},
};

const MANIFEST_FILE: &str = "release.manifest";
const SIGNATURE_FILE: &str = "release.manifest.sig";
const READY_FILE: &str = "ready.json";
const UPDATE_REQUEST_FILE: &str = "update-request.json";
const REVOCATIONS_FILE: &str = "release-revocations";
const MAX_PUBLIC_KEY_BYTES: u64 = 8192;
const MAX_RELEASE_KEYS: usize = 4;
const MAX_REVOCATIONS_BYTES: u64 = 64 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StagedUpdateRequest {
    pub schema_version: u8,
    pub release_id: Uuid,
    pub policy_generation: u64,
    pub decision: UpdateDecision,
    pub version: ReleaseVersion,
    pub directory_name: String,
    pub archive_name: String,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum UpdateStagingError {
    #[error("update directive failed local validation")]
    Directive,
    #[error("pinned update trust failed validation")]
    Trust,
    #[error("update download failed")]
    Download,
    #[error("update archive failed integrity validation")]
    Archive,
    #[error("update staging state is unavailable")]
    State,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum UpdateApplyError {
    #[error("staged update request failed validation")]
    Request,
    #[error("staged update trust failed validation")]
    Trust,
    #[error("root update execution environment is invalid")]
    Environment,
    #[error("root update installer rejected the release")]
    Installer,
    #[error("root update state operation failed")]
    State,
}

impl UpdateApplyError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Request => "update_request_invalid",
            Self::Trust => "update_apply_trust_invalid",
            Self::Environment => "update_apply_environment_invalid",
            Self::Installer => "update_installer_failed",
            Self::State => "update_apply_state_failed",
        }
    }
}

impl UpdateStagingError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Directive => "update_directive_invalid",
            Self::Trust => "update_trust_invalid",
            Self::Download => "update_download_failed",
            Self::Archive => "update_archive_invalid",
            Self::State => "update_staging_failed",
        }
    }
}

#[derive(Debug)]
struct VerifiedDirective {
    manifest: LinuxReleaseManifest,
    manifest_bytes: Vec<u8>,
    signature_bytes: [u8; 64],
    archive_url: Url,
}

/// Independently verifies, downloads, and atomically publishes one update for the root helper.
///
/// # Errors
///
/// Returns a bounded staging error when directive, trust, transport, archive, or local state
/// validation fails. No partially downloaded directory is published as ready.
pub async fn stage_update(
    config: &AgentConfig,
    assigned_channel: UpdateChannel,
    directive: &UpdateDirective,
) -> Result<StagedUpdateRequest, UpdateStagingError> {
    let verified = verify_directive(config, assigned_channel, directive)?;
    let staging_root = config.state_directory.join("update-staging");
    ensure_private_directory(&staging_root).map_err(|_| UpdateStagingError::State)?;
    let directory_name = directive.release_id.simple().to_string();
    let final_directory = staging_root.join(&directory_name);
    let request = StagedUpdateRequest {
        schema_version: 1,
        release_id: directive.release_id,
        policy_generation: directive.policy_generation,
        decision: directive.decision,
        version: directive.version,
        directory_name,
        archive_name: directive.archive_name.clone(),
    };

    if final_directory.exists() {
        verify_existing_stage(&final_directory, &request, &verified).await?;
        publish_request(config, &request)?;
        return Ok(request);
    }

    let temporary_directory = staging_root.join(format!(
        ".{}.{}.tmp",
        directive.release_id.simple(),
        Uuid::new_v4().simple()
    ));
    ensure_private_directory(&temporary_directory).map_err(|_| UpdateStagingError::State)?;
    if let Err(error) = stage_into(
        &temporary_directory,
        &request,
        &verified,
        directive.archive_size,
        &directive.archive_sha256,
    )
    .await
    {
        let _ = tokio::fs::remove_dir_all(&temporary_directory).await;
        return Err(error);
    }
    if std::fs::rename(&temporary_directory, &final_directory).is_err() {
        let _ = tokio::fs::remove_dir_all(&temporary_directory).await;
        verify_existing_stage(&final_directory, &request, &verified).await?;
    }
    sync_directory(&staging_root)?;
    publish_request(config, &request)?;
    Ok(request)
}

/// Revalidates an Agent-owned staged release through a private copy and invokes the installer.
///
/// # Errors
///
/// Returns a bounded apply error when the request, root boundary, copied release, installer, or
/// cleanup state fails validation. Production application at `/` requires effective UID zero.
pub fn apply_staged_update(
    config: &AgentConfig,
    installer: &Path,
    install_root: &Path,
    service_manager: Option<&Path>,
) -> Result<(), UpdateApplyError> {
    validate_apply_environment(installer, install_root, service_manager)?;

    let request_path = config.state_directory.join(UPDATE_REQUEST_FILE);
    let request_bytes =
        read_bounded_regular(&request_path, 16 * 1024).map_err(|_| UpdateApplyError::Request)?;
    let request: StagedUpdateRequest =
        serde_json::from_slice(&request_bytes).map_err(|_| UpdateApplyError::Request)?;
    validate_apply_request(&request)?;

    let staging_directory = config
        .state_directory
        .join("update-staging")
        .join(&request.directory_name);
    let staging_metadata = staging_directory
        .symlink_metadata()
        .map_err(|_| UpdateApplyError::State)?;
    if !staging_metadata.is_dir() || staging_metadata.file_type().is_symlink() {
        return Err(UpdateApplyError::State);
    }

    let prepared = prepare_update_copy(config, &request, &staging_directory, install_root)?;
    run_update_installer(installer, install_root, service_manager, config, &prepared)?;

    if read_bounded_regular(&request_path, 16 * 1024).is_ok_and(|current| current == request_bytes)
    {
        std::fs::remove_file(&request_path).map_err(|_| UpdateApplyError::State)?;
        sync_directory(&config.state_directory).map_err(|_| UpdateApplyError::State)?;
    }
    Ok(())
}

pub(crate) fn cancel_staged_update(
    config: &AgentConfig,
    expected_release_id: Uuid,
) -> Result<bool, UpdateStagingError> {
    let request_path = config.state_directory.join(UPDATE_REQUEST_FILE);
    let request_bytes = match read_bounded_regular(&request_path, 16 * 1024) {
        Ok(bytes) => bytes,
        Err(UpdateStagingError::State)
            if request_path
                .symlink_metadata()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    let request: StagedUpdateRequest =
        serde_json::from_slice(&request_bytes).map_err(|_| UpdateStagingError::State)?;
    if request.release_id != expected_release_id
        || request.directory_name != request.release_id.simple().to_string()
    {
        return Ok(false);
    }
    std::fs::remove_file(&request_path).map_err(|_| UpdateStagingError::State)?;
    sync_directory(&config.state_directory)?;

    let staging_directory = config
        .state_directory
        .join("update-staging")
        .join(&request.directory_name);
    match staging_directory.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            std::fs::remove_dir_all(&staging_directory).map_err(|_| UpdateStagingError::State)?;
            sync_directory(
                staging_directory
                    .parent()
                    .ok_or(UpdateStagingError::State)?,
            )?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(UpdateStagingError::State),
    }
    Ok(true)
}

fn validate_apply_environment(
    installer: &Path,
    install_root: &Path,
    service_manager: Option<&Path>,
) -> Result<(), UpdateApplyError> {
    if !cfg!(target_os = "linux")
        || !install_root.is_absolute()
        || matches!(install_root.to_str(), Some("/proc" | "/sys" | "/dev"))
        || (install_root == Path::new("/") && !effective_user_is_root())
        || !trusted_installer(installer, install_root == Path::new("/"))
        || service_manager.is_some_and(|path| !trusted_installer(path, false))
    {
        return Err(UpdateApplyError::Environment);
    }
    let root_metadata = install_root
        .symlink_metadata()
        .map_err(|_| UpdateApplyError::Environment)?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(UpdateApplyError::Environment);
    }
    Ok(())
}

fn prepare_update_copy(
    config: &AgentConfig,
    request: &StagedUpdateRequest,
    staging_directory: &Path,
    install_root: &Path,
) -> Result<PreparedUpdate, UpdateApplyError> {
    let private_copy = PrivateUpdateCopy::create(install_root)?;
    let copied_manifest = private_copy.path.join(MANIFEST_FILE);
    let copied_signature = private_copy.path.join(SIGNATURE_FILE);
    let copied_archive = private_copy.path.join(&request.archive_name);
    copy_regular_file(
        &staging_directory.join(MANIFEST_FILE),
        &copied_manifest,
        4096,
    )?;
    copy_regular_file(
        &staging_directory.join(SIGNATURE_FILE),
        &copied_signature,
        64,
    )?;
    let manifest_bytes =
        read_bounded_regular(&copied_manifest, 4096).map_err(|_| UpdateApplyError::State)?;
    let manifest =
        LinuxReleaseManifest::parse(&manifest_bytes).map_err(|_| UpdateApplyError::Request)?;
    if manifest.version != request.version
        || manifest.archive != request.archive_name
        || manifest.architecture
            != supported_architecture().map_err(|_| UpdateApplyError::Request)?
    {
        return Err(UpdateApplyError::Request);
    }
    copy_regular_file(
        &staging_directory.join(&request.archive_name),
        &copied_archive,
        manifest.archive_size,
    )?;

    let signature =
        read_bounded_regular(&copied_signature, 64).map_err(|_| UpdateApplyError::State)?;
    let signature: [u8; 64] = signature
        .as_slice()
        .try_into()
        .map_err(|_| UpdateApplyError::Trust)?;
    verify_release_signature(
        &config.update_signing_public_key_path,
        &manifest_bytes,
        &Signature::from_bytes(&signature),
    )
    .map_err(|_| UpdateApplyError::Trust)?;
    reject_revoked_manifest(&config.update_signing_public_key_path, &manifest_bytes)
        .map_err(|_| UpdateApplyError::Trust)?;
    verify_archive_file_sync(
        &copied_archive,
        manifest.archive_size,
        &manifest.archive_sha256,
    )?;
    Ok(PreparedUpdate {
        _private_copy: private_copy,
        manifest: copied_manifest,
        signature: copied_signature,
        archive: copied_archive,
    })
}

fn run_update_installer(
    installer: &Path,
    install_root: &Path,
    service_manager: Option<&Path>,
    config: &AgentConfig,
    prepared: &PreparedUpdate,
) -> Result<(), UpdateApplyError> {
    let mut command = Command::new(installer);
    command
        .arg("install")
        .arg("--archive")
        .arg(&prepared.archive)
        .arg("--manifest")
        .arg(&prepared.manifest)
        .arg("--signature")
        .arg(&prepared.signature)
        .arg("--public-key")
        .arg(&config.update_signing_public_key_path)
        .arg("--root")
        .arg(install_root)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::null());
    if let Some(service_manager) = service_manager {
        command.arg("--service-manager").arg(service_manager);
    }
    if !command
        .status()
        .map_err(|_| UpdateApplyError::Installer)?
        .success()
    {
        return Err(UpdateApplyError::Installer);
    }
    Ok(())
}

fn validate_apply_request(request: &StagedUpdateRequest) -> Result<(), UpdateApplyError> {
    let current_version = env!("CARGO_PKG_VERSION")
        .parse::<ReleaseVersion>()
        .map_err(|_| UpdateApplyError::Environment)?;
    if request.schema_version != 1
        || request.policy_generation == 0
        || !matches!(
            request.decision,
            UpdateDecision::Eligible | UpdateDecision::Required
        )
        || request.version <= current_version
        || request.directory_name != request.release_id.simple().to_string()
        || request.archive_name
            != format!(
                "xs-nexus-{}-{}-unknown-linux-gnu.tar.gz",
                request.version,
                supported_architecture().map_err(|_| UpdateApplyError::Request)?
            )
    {
        return Err(UpdateApplyError::Request);
    }
    Ok(())
}

struct PrivateUpdateCopy {
    path: PathBuf,
}

struct PreparedUpdate {
    _private_copy: PrivateUpdateCopy,
    manifest: PathBuf,
    signature: PathBuf,
    archive: PathBuf,
}

impl PrivateUpdateCopy {
    fn create(install_root: &Path) -> Result<Self, UpdateApplyError> {
        let run_root = if install_root == Path::new("/") {
            PathBuf::from("/run/xs-nexus-update")
        } else {
            install_root.join("run/xs-nexus-update")
        };
        ensure_private_directory(&run_root).map_err(|_| UpdateApplyError::State)?;
        let path = run_root.join(Uuid::new_v4().simple().to_string());
        ensure_private_directory(&path).map_err(|_| UpdateApplyError::State)?;
        Ok(Self { path })
    }
}

impl Drop for PrivateUpdateCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn copy_regular_file(
    source: &Path,
    destination: &Path,
    maximum: u64,
) -> Result<(), UpdateApplyError> {
    let mut source = open_regular_nofollow(source).map_err(|_| UpdateApplyError::State)?;
    let metadata = source.metadata().map_err(|_| UpdateApplyError::State)?;
    if metadata.len() == 0 || metadata.len() > maximum {
        return Err(UpdateApplyError::Request);
    }
    let mut destination =
        create_new_private_sync(destination).map_err(|_| UpdateApplyError::State)?;
    let copied =
        std::io::copy(&mut source, &mut destination).map_err(|_| UpdateApplyError::State)?;
    if copied != metadata.len() {
        return Err(UpdateApplyError::State);
    }
    destination.flush().map_err(|_| UpdateApplyError::State)?;
    destination.sync_all().map_err(|_| UpdateApplyError::State)
}

fn verify_archive_file_sync(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateApplyError> {
    let mut file = open_regular_nofollow(path).map_err(|_| UpdateApplyError::State)?;
    let metadata = file.metadata().map_err(|_| UpdateApplyError::State)?;
    if metadata.len() != expected_size {
        return Err(UpdateApplyError::Request);
    }
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| UpdateApplyError::State)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    if encode_lower_hex(&digest.finalize()) != expected_sha256 {
        return Err(UpdateApplyError::Trust);
    }
    Ok(())
}

fn trusted_installer(path: &Path, require_root_owner: bool) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let Ok(metadata) = path.symlink_metadata() else {
        return false;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    trusted_executable_metadata(&metadata, require_root_owner)
}

#[cfg(unix)]
fn trusted_executable_metadata(metadata: &std::fs::Metadata, require_root_owner: bool) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    metadata.mode() & 0o111 != 0
        && metadata.mode() & 0o022 == 0
        && (!require_root_owner || metadata.uid() == 0)
}

#[cfg(not(unix))]
fn trusted_executable_metadata(_metadata: &std::fs::Metadata, _require_root_owner: bool) -> bool {
    false
}

fn effective_user_is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("Uid:"))
                .and_then(|line| line.split_ascii_whitespace().nth(2))
                .and_then(|value| value.parse::<u32>().ok())
        })
        == Some(0)
}

fn publish_request(
    config: &AgentConfig,
    request: &StagedUpdateRequest,
) -> Result<(), UpdateStagingError> {
    write_json(&config.state_directory.join(UPDATE_REQUEST_FILE), request)
        .map_err(|_| UpdateStagingError::State)
}

fn verify_directive(
    config: &AgentConfig,
    assigned_channel: UpdateChannel,
    directive: &UpdateDirective,
) -> Result<VerifiedDirective, UpdateStagingError> {
    let current_version = env!("CARGO_PKG_VERSION")
        .parse::<ReleaseVersion>()
        .map_err(|_| UpdateStagingError::State)?;
    let architecture = supported_architecture()?;
    let required_is_consistent = directive
        .minimum_version
        .is_some_and(|minimum| current_version < minimum);
    if directive.schema_version != 1
        || directive.policy_generation == 0
        || !matches!(
            directive.decision,
            UpdateDecision::Eligible | UpdateDecision::Required
        )
        || directive.channel != assigned_channel
        || directive.platform != "linux"
        || directive.architecture != architecture
        || directive.version <= current_version
        || directive
            .minimum_version
            .is_some_and(|minimum| minimum > directive.version)
        || (directive.decision == UpdateDecision::Required) != required_is_consistent
        || !(1..=MAX_UPDATE_ARCHIVE_BYTES).contains(&directive.archive_size)
        || !valid_lower_sha256(&directive.archive_sha256)
    {
        return Err(UpdateStagingError::Directive);
    }

    let manifest_bytes = decode_canonical(&directive.manifest_base64, 4096)
        .map_err(|()| UpdateStagingError::Directive)?;
    let signature_bytes = decode_canonical(&directive.signature_base64, 64)
        .map_err(|()| UpdateStagingError::Directive)?;
    let signature_bytes: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|_| UpdateStagingError::Directive)?;
    let manifest =
        LinuxReleaseManifest::parse(&manifest_bytes).map_err(|_| UpdateStagingError::Directive)?;
    if manifest.version != directive.version
        || manifest.architecture != directive.architecture
        || manifest.archive != directive.archive_name
        || manifest.archive_size != directive.archive_size
        || manifest.archive_sha256 != directive.archive_sha256
    {
        return Err(UpdateStagingError::Directive);
    }

    let archive_url = validate_archive_url(&directive.archive_url, &directive.archive_name)?;
    verify_release_signature(
        &config.update_signing_public_key_path,
        &manifest_bytes,
        &Signature::from_bytes(&signature_bytes),
    )?;
    reject_revoked_manifest(&config.update_signing_public_key_path, &manifest_bytes)?;
    Ok(VerifiedDirective {
        manifest,
        manifest_bytes,
        signature_bytes,
        archive_url,
    })
}

async fn stage_into(
    directory: &Path,
    request: &StagedUpdateRequest,
    verified: &VerifiedDirective,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateStagingError> {
    write_new_private(&directory.join(MANIFEST_FILE), &verified.manifest_bytes).await?;
    write_new_private(&directory.join(SIGNATURE_FILE), &verified.signature_bytes).await?;
    download_archive(
        &verified.archive_url,
        &directory.join(&request.archive_name),
        expected_size,
        expected_sha256,
    )
    .await?;
    write_json(&directory.join(READY_FILE), request).map_err(|_| UpdateStagingError::State)?;
    sync_directory(directory)
}

async fn verify_existing_stage(
    directory: &Path,
    request: &StagedUpdateRequest,
    verified: &VerifiedDirective,
) -> Result<(), UpdateStagingError> {
    let metadata = directory
        .symlink_metadata()
        .map_err(|_| UpdateStagingError::State)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(UpdateStagingError::State);
    }
    let persisted: StagedUpdateRequest =
        read_json(&directory.join(READY_FILE)).map_err(|_| UpdateStagingError::State)?;
    if persisted != *request
        || read_bounded_regular(&directory.join(MANIFEST_FILE), 4096)? != verified.manifest_bytes
        || read_bounded_regular(&directory.join(SIGNATURE_FILE), 64)? != verified.signature_bytes
    {
        return Err(UpdateStagingError::Archive);
    }
    verify_archive_file(
        &directory.join(&request.archive_name),
        verified.manifest.archive_size,
        &verified.manifest.archive_sha256,
    )
    .await
}

async fn download_archive(
    url: &Url,
    destination: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateStagingError> {
    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .redirect(Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| UpdateStagingError::Download)?;
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| UpdateStagingError::Download)?;
    if response.status() != StatusCode::OK
        || response
            .content_length()
            .is_some_and(|length| length != expected_size)
    {
        return Err(UpdateStagingError::Download);
    }
    let mut file = create_new_private(destination)?;
    write_archive_stream(
        response.bytes_stream(),
        &mut file,
        expected_size,
        expected_sha256,
    )
    .await?;
    file.sync_all().await.map_err(|_| UpdateStagingError::State)
}

async fn write_archive_stream<S, B, E, W>(
    stream: S,
    file: &mut W,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateStagingError>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
    W: tokio::io::AsyncWrite + Unpin,
{
    futures_util::pin_mut!(stream);
    let mut received = 0_u64;
    let mut digest = Sha256::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| UpdateStagingError::Download)?;
        let chunk = chunk.as_ref();
        received = received
            .checked_add(u64::try_from(chunk.len()).map_err(|_| UpdateStagingError::Archive)?)
            .ok_or(UpdateStagingError::Archive)?;
        if received > expected_size || received > MAX_UPDATE_ARCHIVE_BYTES {
            return Err(UpdateStagingError::Archive);
        }
        digest.update(chunk);
        file.write_all(chunk)
            .await
            .map_err(|_| UpdateStagingError::State)?;
    }
    if received != expected_size || encode_lower_hex(&digest.finalize()) != expected_sha256 {
        return Err(UpdateStagingError::Archive);
    }
    Ok(())
}

async fn verify_archive_file(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateStagingError> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| UpdateStagingError::State)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != expected_size {
        return Err(UpdateStagingError::Archive);
    }
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| UpdateStagingError::State)?;
    let mut remaining = expected_size;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut digest = Sha256::new();
    while remaining > 0 {
        let maximum = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| UpdateStagingError::Archive)?;
        let read = file
            .read(&mut buffer[..maximum])
            .await
            .map_err(|_| UpdateStagingError::State)?;
        if read == 0 {
            return Err(UpdateStagingError::Archive);
        }
        digest.update(&buffer[..read]);
        remaining -= u64::try_from(read).map_err(|_| UpdateStagingError::Archive)?;
    }
    let mut extra = [0_u8; 1];
    if file
        .read(&mut extra)
        .await
        .map_err(|_| UpdateStagingError::State)?
        != 0
        || encode_lower_hex(&digest.finalize()) != expected_sha256
    {
        return Err(UpdateStagingError::Archive);
    }
    Ok(())
}

async fn write_new_private(path: &Path, bytes: &[u8]) -> Result<(), UpdateStagingError> {
    let mut file = create_new_private(path)?;
    file.write_all(bytes)
        .await
        .map_err(|_| UpdateStagingError::State)?;
    file.sync_all().await.map_err(|_| UpdateStagingError::State)
}

fn create_new_private(path: &Path) -> Result<tokio::fs::File, UpdateStagingError> {
    create_new_private_sync(path)
        .map(tokio::fs::File::from_std)
        .map_err(|_| UpdateStagingError::State)
}

fn create_new_private_sync(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

fn open_regular_nofollow(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(std::io::Error::other("update path is not a regular file"))
    }
}

fn read_bounded_regular(path: &Path, maximum: u64) -> Result<Vec<u8>, UpdateStagingError> {
    let mut file = open_regular_nofollow(path).map_err(|_| UpdateStagingError::State)?;
    let metadata = file.metadata().map_err(|_| UpdateStagingError::State)?;
    if metadata.len() == 0 || metadata.len() > maximum {
        return Err(UpdateStagingError::State);
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| UpdateStagingError::State)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.read_to_end(&mut bytes)
        .map_err(|_| UpdateStagingError::State)?;
    if bytes.len() != capacity {
        return Err(UpdateStagingError::State);
    }
    Ok(bytes)
}

fn read_pinned_keys(path: &Path) -> Result<Vec<VerifyingKey>, UpdateStagingError> {
    let mut file = open_regular_nofollow(path).map_err(|_| UpdateStagingError::Trust)?;
    let metadata = file.metadata().map_err(|_| UpdateStagingError::Trust)?;
    if metadata.len() == 0
        || metadata.len() > MAX_PUBLIC_KEY_BYTES
        || public_key_is_writable_by_untrusted_principal(&metadata)
    {
        return Err(UpdateStagingError::Trust);
    }
    let mut encoded = String::with_capacity(
        usize::try_from(metadata.len()).map_err(|_| UpdateStagingError::Trust)?,
    );
    file.read_to_string(&mut encoded)
        .map_err(|_| UpdateStagingError::Trust)?;
    parse_public_key_bundle(&encoded)
}

fn parse_public_key_bundle(encoded: &str) -> Result<Vec<VerifyingKey>, UpdateStagingError> {
    const BEGIN: &str = "-----BEGIN PUBLIC KEY-----\n";
    const END: &str = "-----END PUBLIC KEY-----\n";

    if encoded.contains('\r') {
        return Err(UpdateStagingError::Trust);
    }
    let mut remaining = encoded;
    let mut keys = Vec::new();
    while !remaining.is_empty() {
        let body = remaining
            .strip_prefix(BEGIN)
            .ok_or(UpdateStagingError::Trust)?;
        let end = body.find(END).ok_or(UpdateStagingError::Trust)?;
        let block_length = BEGIN
            .len()
            .checked_add(end)
            .and_then(|length| length.checked_add(END.len()))
            .ok_or(UpdateStagingError::Trust)?;
        let (block, rest) = remaining.split_at(block_length);
        let key =
            VerifyingKey::from_public_key_pem(block).map_err(|_| UpdateStagingError::Trust)?;
        if keys.len() == MAX_RELEASE_KEYS
            || keys
                .iter()
                .any(|existing: &VerifyingKey| existing.to_bytes() == key.to_bytes())
        {
            return Err(UpdateStagingError::Trust);
        }
        keys.push(key);
        remaining = rest;
    }
    if keys.is_empty() {
        return Err(UpdateStagingError::Trust);
    }
    Ok(keys)
}

fn verify_release_signature(
    key_path: &Path,
    message: &[u8],
    signature: &Signature,
) -> Result<(), UpdateStagingError> {
    if read_pinned_keys(key_path)?
        .iter()
        .any(|key| key.verify_strict(message, signature).is_ok())
    {
        Ok(())
    } else {
        Err(UpdateStagingError::Trust)
    }
}

fn reject_revoked_manifest(key_path: &Path, manifest: &[u8]) -> Result<(), UpdateStagingError> {
    let parent = key_path.parent().ok_or(UpdateStagingError::Trust)?;
    let path = parent.join(REVOCATIONS_FILE);
    let mut file = match open_regular_nofollow(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(UpdateStagingError::Trust),
    };
    let metadata = file.metadata().map_err(|_| UpdateStagingError::Trust)?;
    if metadata.len() == 0
        || metadata.len() > MAX_REVOCATIONS_BYTES
        || public_key_is_writable_by_untrusted_principal(&metadata)
    {
        return Err(UpdateStagingError::Trust);
    }
    let mut encoded = String::with_capacity(
        usize::try_from(metadata.len()).map_err(|_| UpdateStagingError::Trust)?,
    );
    file.read_to_string(&mut encoded)
        .map_err(|_| UpdateStagingError::Trust)?;
    if encoded.contains('\r') || !encoded.ends_with('\n') {
        return Err(UpdateStagingError::Trust);
    }
    let manifest_hash = encode_lower_hex(&Sha256::digest(manifest));
    let mut previous: Option<&str> = None;
    for hash in encoded.lines() {
        if !valid_lower_sha256(hash) || previous.is_some_and(|value| value >= hash) {
            return Err(UpdateStagingError::Trust);
        }
        if hash == manifest_hash {
            return Err(UpdateStagingError::Trust);
        }
        previous = Some(hash);
    }
    Ok(())
}

#[cfg(unix)]
fn public_key_is_writable_by_untrusted_principal(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    metadata.mode() & 0o022 != 0 || (effective_user_is_root() && metadata.uid() != 0)
}

#[cfg(not(unix))]
fn public_key_is_writable_by_untrusted_principal(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn validate_archive_url(value: &str, archive_name: &str) -> Result<Url, UpdateStagingError> {
    if value.len() > 2048 {
        return Err(UpdateStagingError::Directive);
    }
    let url = Url::parse(value).map_err(|_| UpdateStagingError::Directive)?;
    let final_segment = url
        .path_segments()
        .and_then(Iterator::last)
        .ok_or(UpdateStagingError::Directive)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || final_segment != archive_name
    {
        return Err(UpdateStagingError::Directive);
    }
    Ok(url)
}

fn supported_architecture() -> Result<&'static str, UpdateStagingError> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x86_64"),
        "aarch64" => Ok("aarch64"),
        _ => Err(UpdateStagingError::Directive),
    }
}

fn decode_canonical(value: &str, maximum: usize) -> Result<Vec<u8>, ()> {
    let decoded = URL_SAFE_NO_PAD.decode(value).map_err(|_| ())?;
    if decoded.is_empty() || decoded.len() > maximum || URL_SAFE_NO_PAD.encode(&decoded) != value {
        return Err(());
    }
    Ok(decoded)
}

fn valid_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn sync_directory(path: &Path) -> Result<(), UpdateStagingError> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| UpdateStagingError::State)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{
        os::unix::fs::PermissionsExt as _,
        pin::Pin,
        task::{Context, Poll},
    };

    use ed25519_dalek::{
        Signer as _, SigningKey,
        pkcs8::{EncodePublicKey as _, spki::der::pem::LineEnding},
    };
    use sha2::{Digest as _, Sha256};
    use tokio::io::AsyncWrite;
    use xs_core::{UpdateChannel, UpdateDecision};

    use super::*;

    fn fixture(
        temporary: &tempfile::TempDir,
        signing_key: &SigningKey,
    ) -> (AgentConfig, UpdateDirective) {
        let key_path = temporary.path().join("release-public-key.pem");
        std::fs::write(
            &key_path,
            signing_key
                .verifying_key()
                .to_public_key_pem(LineEnding::LF)
                .expect("public key PEM"),
        )
        .expect("write public key");
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644))
            .expect("public key mode");
        let archive = format!(
            "xs-nexus-0.2.0-{}-unknown-linux-gnu.tar.gz",
            std::env::consts::ARCH
        );
        let archive_bytes = b"signed archive fixture";
        let archive_hash = encode_lower_hex(&Sha256::digest(archive_bytes));
        let manifest = format!(
            "schema_version=2\nproduct=xs-nexus\nversion=0.2.0\nsource_commit=0123456789abcdef0123456789abcdef01234567\nsource_date_epoch=1700000000\nprotocol_version=XSP/1\nplatform=linux\narchitecture={}\ntarget={}-unknown-linux-gnu\narchive={}\narchive_size={}\narchive_sha256={}\n",
            std::env::consts::ARCH,
            std::env::consts::ARCH,
            archive,
            archive_bytes.len(),
            archive_hash
        );
        let signature = signing_key.sign(manifest.as_bytes());
        (
            AgentConfig {
                controller_url: "https://controller.example/".to_owned(),
                node_name: "update-test".to_owned(),
                device_type: "linux".to_owned(),
                state_directory: temporary.path().join("state"),
                runtime_directory: temporary.path().join("run"),
                interface_name: "xsn0".to_owned(),
                mtu: 1280,
                control_sync_interval_seconds: 15,
                update_channel: UpdateChannel::Stable,
                update_signing_public_key_path: key_path,
                windows_wintun: None,
            },
            UpdateDirective {
                schema_version: 1,
                policy_generation: 1,
                release_id: Uuid::from_u128(7),
                decision: UpdateDecision::Eligible,
                channel: UpdateChannel::Stable,
                version: "0.2.0".parse().expect("version"),
                minimum_version: None,
                platform: "linux".to_owned(),
                architecture: std::env::consts::ARCH.to_owned(),
                archive_name: archive.clone(),
                archive_size: archive_bytes.len() as u64,
                archive_sha256: archive_hash,
                archive_url: format!("https://updates.example.test/{archive}"),
                manifest_base64: URL_SAFE_NO_PAD.encode(manifest),
                signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
            },
        )
    }

    fn materialize_stage(
        config: &AgentConfig,
        directive: &UpdateDirective,
        archive_bytes: &[u8],
    ) -> StagedUpdateRequest {
        let request = StagedUpdateRequest {
            schema_version: 1,
            release_id: directive.release_id,
            policy_generation: directive.policy_generation,
            decision: directive.decision,
            version: directive.version,
            directory_name: directive.release_id.simple().to_string(),
            archive_name: directive.archive_name.clone(),
        };
        let stage = config
            .state_directory
            .join("update-staging")
            .join(&request.directory_name);
        ensure_private_directory(&stage).expect("stage directory");
        std::fs::write(
            stage.join(MANIFEST_FILE),
            URL_SAFE_NO_PAD
                .decode(&directive.manifest_base64)
                .expect("manifest base64"),
        )
        .expect("manifest");
        std::fs::write(
            stage.join(SIGNATURE_FILE),
            URL_SAFE_NO_PAD
                .decode(&directive.signature_base64)
                .expect("signature base64"),
        )
        .expect("signature");
        std::fs::write(stage.join(&request.archive_name), archive_bytes).expect("archive");
        write_json(&config.state_directory.join(UPDATE_REQUEST_FILE), &request).expect("request");
        request
    }

    fn fake_installer(temporary: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        let installer = temporary.path().join("fake-installer");
        let record = temporary.path().join("installer-arguments");
        std::fs::write(
            &installer,
            format!(
                "#!/bin/sh\nset -eu\ntest \"$PATH\" = /usr/sbin:/usr/bin:/sbin:/bin\nprintf '%s\\n' \"$@\" >{}\n",
                record.display()
            ),
        )
        .expect("fake installer");
        std::fs::set_permissions(&installer, std::fs::Permissions::from_mode(0o755))
            .expect("installer mode");
        (installer, record)
    }

    struct FailingWriter;

    impl AsyncWrite for FailingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _buffer: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(Err(std::io::Error::other("simulated storage exhaustion")))
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn directive_requires_the_locally_pinned_offline_signature() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[41_u8; 32]);
        let (config, directive) = fixture(&temporary, &signing_key);
        assert!(verify_directive(&config, config.update_channel, &directive).is_ok());

        let mut tampered = directive.clone();
        tampered.archive_sha256.replace_range(0..1, "0");
        assert_eq!(
            verify_directive(&config, config.update_channel, &tampered).unwrap_err(),
            UpdateStagingError::Directive
        );

        let mut insecure = directive;
        insecure.archive_url = insecure.archive_url.replacen("https", "http", 1);
        assert_eq!(
            verify_directive(&config, config.update_channel, &insecure).unwrap_err(),
            UpdateStagingError::Directive
        );
    }

    #[test]
    fn directive_rejects_wrong_platform_architecture_and_unsigned_release() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[45_u8; 32]);
        let (config, directive) = fixture(&temporary, &signing_key);

        let mut wrong_platform = directive.clone();
        wrong_platform.platform = "windows".to_owned();
        assert_eq!(
            verify_directive(&config, config.update_channel, &wrong_platform).unwrap_err(),
            UpdateStagingError::Directive
        );

        let mut wrong_architecture = directive.clone();
        wrong_architecture.architecture = if std::env::consts::ARCH == "x86_64" {
            "aarch64".to_owned()
        } else {
            "x86_64".to_owned()
        };
        assert_eq!(
            verify_directive(&config, config.update_channel, &wrong_architecture).unwrap_err(),
            UpdateStagingError::Directive
        );

        let mut unsigned = directive;
        unsigned.signature_base64 = URL_SAFE_NO_PAD.encode([0_u8; 64]);
        assert_eq!(
            verify_directive(&config, config.update_channel, &unsigned).unwrap_err(),
            UpdateStagingError::Trust
        );
    }

    #[tokio::test]
    async fn archive_stream_fails_closed_on_interruption_truncation_and_storage_error() {
        let complete = b"complete update archive".to_vec();
        let complete_hash = encode_lower_hex(&Sha256::digest(&complete));

        let mut sink = tokio::io::sink();
        assert_eq!(
            write_archive_stream(
                futures_util::stream::iter(vec![Ok::<_, ()>(complete.clone())]),
                &mut sink,
                complete.len() as u64,
                &complete_hash,
            )
            .await,
            Ok(())
        );

        let mut sink = tokio::io::sink();
        assert_eq!(
            write_archive_stream(
                futures_util::stream::iter(vec![Ok::<_, ()>(b"short".to_vec())]),
                &mut sink,
                complete.len() as u64,
                &complete_hash,
            )
            .await,
            Err(UpdateStagingError::Archive)
        );

        let mut sink = tokio::io::sink();
        assert_eq!(
            write_archive_stream(
                futures_util::stream::iter(vec![
                    Ok(complete[..8].to_vec()),
                    Err("simulated network interruption"),
                ]),
                &mut sink,
                complete.len() as u64,
                &complete_hash,
            )
            .await,
            Err(UpdateStagingError::Download)
        );

        assert_eq!(
            write_archive_stream(
                futures_util::stream::iter(vec![Ok::<_, ()>(complete.clone())]),
                &mut FailingWriter,
                complete.len() as u64,
                &complete_hash,
            )
            .await,
            Err(UpdateStagingError::State)
        );
    }

    #[tokio::test]
    async fn transport_failure_never_publishes_a_partial_stage() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[46_u8; 32]);
        let (config, mut directive) = fixture(&temporary, &signing_key);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserved local port");
        let endpoint = listener.local_addr().expect("local address");
        drop(listener);
        directive.archive_url = format!("https://{endpoint}/{}", directive.archive_name);

        assert_eq!(
            stage_update(&config, config.update_channel, &directive).await,
            Err(UpdateStagingError::Download)
        );
        assert!(!config.state_directory.join(UPDATE_REQUEST_FILE).exists());
        let staging_root = config.state_directory.join("update-staging");
        assert_eq!(
            std::fs::read_dir(staging_root)
                .expect("staging root")
                .count(),
            0
        );
    }

    #[test]
    fn pinned_key_rejects_group_or_world_writes() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[42_u8; 32]);
        let (config, directive) = fixture(&temporary, &signing_key);
        std::fs::set_permissions(
            &config.update_signing_public_key_path,
            std::fs::Permissions::from_mode(0o666),
        )
        .expect("loosen key mode");
        assert_eq!(
            verify_directive(&config, config.update_channel, &directive).unwrap_err(),
            UpdateStagingError::Trust
        );
    }

    #[test]
    fn pinned_key_bundle_supports_overlap_rotation_and_rejects_untrusted_keys() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let old_key = SigningKey::from_bytes(&[47_u8; 32]);
        let new_key = SigningKey::from_bytes(&[48_u8; 32]);
        let (config, directive) = fixture(&temporary, &new_key);
        let old_pem = old_key
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("old public key PEM");
        let new_pem = new_key
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("new public key PEM");

        std::fs::write(
            &config.update_signing_public_key_path,
            format!("{old_pem}{new_pem}"),
        )
        .expect("overlap key bundle");
        assert!(verify_directive(&config, config.update_channel, &directive).is_ok());

        std::fs::write(&config.update_signing_public_key_path, &new_pem)
            .expect("rotated key bundle");
        assert!(verify_directive(&config, config.update_channel, &directive).is_ok());

        std::fs::write(&config.update_signing_public_key_path, &old_pem)
            .expect("untrusted key bundle");
        assert_eq!(
            verify_directive(&config, config.update_channel, &directive).unwrap_err(),
            UpdateStagingError::Trust
        );

        std::fs::write(
            &config.update_signing_public_key_path,
            format!("{new_pem}{new_pem}"),
        )
        .expect("duplicate key bundle");
        assert_eq!(
            verify_directive(&config, config.update_channel, &directive).unwrap_err(),
            UpdateStagingError::Trust
        );
    }

    #[test]
    fn revoked_manifest_is_rejected_before_staging_and_again_before_apply() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[49_u8; 32]);
        let (config, directive) = fixture(&temporary, &signing_key);
        let manifest = URL_SAFE_NO_PAD
            .decode(&directive.manifest_base64)
            .expect("manifest base64");
        let revocations = config
            .update_signing_public_key_path
            .parent()
            .expect("key parent")
            .join(REVOCATIONS_FILE);
        std::fs::write(
            &revocations,
            format!("{}\n", encode_lower_hex(&Sha256::digest(&manifest))),
        )
        .expect("revocation ledger");
        std::fs::set_permissions(&revocations, std::fs::Permissions::from_mode(0o644))
            .expect("revocation mode");

        assert_eq!(
            verify_directive(&config, config.update_channel, &directive).unwrap_err(),
            UpdateStagingError::Trust
        );

        materialize_stage(&config, &directive, b"signed archive fixture");
        let install_root = temporary.path().join("install-root");
        std::fs::create_dir(&install_root).expect("install root");
        let (installer, record) = fake_installer(&temporary);
        assert_eq!(
            apply_staged_update(&config, &installer, &install_root, None),
            Err(UpdateApplyError::Trust)
        );
        assert!(!record.exists());
    }

    #[test]
    fn request_schema_rejects_unknown_fields() {
        let encoded = serde_json::json!({
            "schema_version": 1,
            "release_id": Uuid::from_u128(1),
            "policy_generation": 1,
            "decision": "eligible",
            "version": "0.2.0",
            "directory_name": "00000000000000000000000000000001",
            "archive_name": "xs-nexus-0.2.0-x86_64-unknown-linux-gnu.tar.gz",
            "extra": true
        });
        assert!(serde_json::from_value::<StagedUpdateRequest>(encoded).is_err());
    }

    #[test]
    fn root_helper_reverifies_a_private_copy_before_invoking_the_installer() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[43_u8; 32]);
        let (config, directive) = fixture(&temporary, &signing_key);
        materialize_stage(&config, &directive, b"signed archive fixture");
        let install_root = temporary.path().join("install-root");
        std::fs::create_dir(&install_root).expect("install root");
        let (installer, record) = fake_installer(&temporary);

        apply_staged_update(&config, &installer, &install_root, None).expect("apply update");

        assert!(!config.state_directory.join(UPDATE_REQUEST_FILE).exists());
        let arguments = std::fs::read_to_string(record).expect("installer arguments");
        assert!(arguments.starts_with("install\n--archive\n"));
        assert!(arguments.contains("\n--manifest\n"));
        assert!(arguments.contains("\n--signature\n"));
        assert!(arguments.contains(&format!("\n--root\n{}\n", install_root.display())));
        assert!(arguments.contains(&format!(
            "\n--public-key\n{}\n",
            config.update_signing_public_key_path.display()
        )));
        assert_eq!(
            std::fs::read_dir(install_root.join("run/xs-nexus-update"))
                .expect("private copy root")
                .count(),
            0
        );
    }

    #[test]
    fn root_helper_rejects_a_tampered_archive_without_invoking_the_installer() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[44_u8; 32]);
        let (config, directive) = fixture(&temporary, &signing_key);
        materialize_stage(&config, &directive, b"tigned archive fixture");
        let install_root = temporary.path().join("install-root");
        std::fs::create_dir(&install_root).expect("install root");
        let (installer, record) = fake_installer(&temporary);

        assert_eq!(
            apply_staged_update(&config, &installer, &install_root, None),
            Err(UpdateApplyError::Trust)
        );
        assert!(config.state_directory.join(UPDATE_REQUEST_FILE).exists());
        assert!(!record.exists());
    }

    #[test]
    fn withdrawn_directive_removes_only_the_matching_ready_stage() {
        if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
            return;
        }
        let temporary = tempfile::tempdir().expect("temporary directory");
        let signing_key = SigningKey::from_bytes(&[50_u8; 32]);
        let (config, directive) = fixture(&temporary, &signing_key);
        let request = materialize_stage(&config, &directive, b"signed archive fixture");
        let stage = config
            .state_directory
            .join("update-staging")
            .join(&request.directory_name);

        assert_eq!(
            cancel_staged_update(&config, Uuid::from_u128(999)),
            Ok(false)
        );
        assert!(config.state_directory.join(UPDATE_REQUEST_FILE).exists());
        assert!(stage.exists());
        assert_eq!(
            cancel_staged_update(&config, directive.release_id),
            Ok(true)
        );
        assert!(!config.state_directory.join(UPDATE_REQUEST_FILE).exists());
        assert!(!stage.exists());
        assert_eq!(
            cancel_staged_update(&config, directive.release_id),
            Ok(false)
        );
    }
}
