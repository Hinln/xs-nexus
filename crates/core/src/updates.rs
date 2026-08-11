use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

const UPDATE_BUCKET_DOMAIN: &[u8] = b"XS Nexus update rollout bucket v1";
const UPDATE_REPORT_DOMAIN: &[u8] = b"XS Nexus Agent runtime report v1";
const MAX_ROLLOUT_BASIS_POINTS: u16 = 10_000;
pub const MAX_UPDATE_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReleaseVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl ReleaseVersion {
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for ReleaseVersion {
    type Err = ReleaseVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || value
                .bytes()
                .any(|byte| !byte.is_ascii_digit() && byte != b'.')
        {
            return Err(ReleaseVersionError::Invalid);
        }
        let mut components = value.split('.');
        let major = parse_version_component(components.next())?;
        let minor = parse_version_component(components.next())?;
        let patch = parse_version_component(components.next())?;
        if components.next().is_some() {
            return Err(ReleaseVersionError::Invalid);
        }
        Ok(Self::new(major, minor, patch))
    }
}

impl Serialize for ReleaseVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ReleaseVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

fn parse_version_component(component: Option<&str>) -> Result<u32, ReleaseVersionError> {
    let component = component.ok_or(ReleaseVersionError::Invalid)?;
    if component.is_empty()
        || (component.len() > 1 && component.starts_with('0'))
        || !component.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ReleaseVersionError::Invalid);
    }
    component
        .parse::<u32>()
        .map_err(|_| ReleaseVersionError::Invalid)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ReleaseVersionError {
    #[error("release version must be a canonical major.minor.patch triplet")]
    Invalid,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    Stable,
    Testing,
    Development,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxReleaseManifest {
    pub source_commit: String,
    pub source_date_epoch: u64,
    pub protocol_version: String,
    pub version: ReleaseVersion,
    pub architecture: String,
    pub target: String,
    pub archive: String,
    pub archive_size: u64,
    pub archive_sha256: String,
}

impl LinuxReleaseManifest {
    /// Parses the exact current release manifest consumed by the Linux installer.
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseManifestError`] when the input is oversized, is not
    /// canonical UTF-8, has unknown or reordered fields, or contains release
    /// identity, target, archive, size, or hash values outside the contract.
    pub fn parse(encoded: &[u8]) -> Result<Self, ReleaseManifestError> {
        if encoded.is_empty() || encoded.len() > 4096 {
            return Err(ReleaseManifestError::Size);
        }
        let text = std::str::from_utf8(encoded).map_err(|_| ReleaseManifestError::Encoding)?;
        if text.contains('\r') || text.ends_with("\n\n") {
            return Err(ReleaseManifestError::Format);
        }
        let text = text.strip_suffix('\n').unwrap_or(text);
        let fields = text.split('\n').collect::<Vec<_>>();
        if fields.len() != 12 {
            return Err(ReleaseManifestError::FieldCount);
        }
        exact_field(fields[0], "schema_version", "2")?;
        exact_field(fields[1], "product", "xs-nexus")?;
        let version: ReleaseVersion = field_value(fields[2], "version")?
            .parse()
            .map_err(|_| ReleaseManifestError::Version)?;
        let source_commit = field_value(fields[3], "source_commit")?;
        if source_commit.len() != 40
            || !source_commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ReleaseManifestError::SourceCommit);
        }
        let source_date_epoch_text = field_value(fields[4], "source_date_epoch")?;
        if source_date_epoch_text.starts_with('0')
            || !source_date_epoch_text
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            return Err(ReleaseManifestError::SourceDateEpoch);
        }
        let source_date_epoch = source_date_epoch_text
            .parse::<u64>()
            .ok()
            .filter(|epoch| *epoch > 0)
            .ok_or(ReleaseManifestError::SourceDateEpoch)?;
        exact_field(fields[5], "protocol_version", "XSP/1")?;
        exact_field(fields[6], "platform", "linux")?;
        let architecture = field_value(fields[7], "architecture")?;
        if !matches!(architecture, "x86_64" | "aarch64") {
            return Err(ReleaseManifestError::Architecture);
        }
        let target = field_value(fields[8], "target")?;
        let expected_target = format!("{architecture}-unknown-linux-gnu");
        if target != expected_target {
            return Err(ReleaseManifestError::Target);
        }
        let archive = field_value(fields[9], "archive")?;
        let expected_archive = format!("xs-nexus-{version}-{target}.tar.gz");
        if archive != expected_archive {
            return Err(ReleaseManifestError::Archive);
        }
        let archive_size_text = field_value(fields[10], "archive_size")?;
        if archive_size_text.starts_with('0')
            || !archive_size_text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ReleaseManifestError::ArchiveSize);
        }
        let archive_size = archive_size_text
            .parse::<u64>()
            .ok()
            .filter(|size| (1..=MAX_UPDATE_ARCHIVE_BYTES).contains(size))
            .ok_or(ReleaseManifestError::ArchiveSize)?;
        let archive_sha256 = field_value(fields[11], "archive_sha256")?;
        if archive_sha256.len() != 64
            || !archive_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ReleaseManifestError::ArchiveHash);
        }
        Ok(Self {
            source_commit: source_commit.to_owned(),
            source_date_epoch,
            protocol_version: "XSP/1".to_owned(),
            version,
            architecture: architecture.to_owned(),
            target: target.to_owned(),
            archive: archive.to_owned(),
            archive_size,
            archive_sha256: archive_sha256.to_owned(),
        })
    }
}

fn field_value<'a>(field: &'a str, expected_name: &str) -> Result<&'a str, ReleaseManifestError> {
    let (name, value) = field.split_once('=').ok_or(ReleaseManifestError::Format)?;
    if name != expected_name || value.is_empty() || value.contains('=') {
        return Err(ReleaseManifestError::Format);
    }
    Ok(value)
}

fn exact_field(
    field: &str,
    expected_name: &str,
    expected_value: &str,
) -> Result<(), ReleaseManifestError> {
    if field_value(field, expected_name)? != expected_value {
        return Err(ReleaseManifestError::Format);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ReleaseManifestError {
    #[error("release manifest size is invalid")]
    Size,
    #[error("release manifest is not UTF-8")]
    Encoding,
    #[error("release manifest format is invalid")]
    Format,
    #[error("release manifest must contain exactly twelve fields")]
    FieldCount,
    #[error("release version is invalid")]
    Version,
    #[error("release source commit is invalid")]
    SourceCommit,
    #[error("release source date epoch is invalid")]
    SourceDateEpoch,
    #[error("release architecture is unsupported")]
    Architecture,
    #[error("release target does not match its architecture")]
    Target,
    #[error("release archive name does not match the manifest")]
    Archive,
    #[error("release archive size is invalid")]
    ArchiveSize,
    #[error("release archive SHA-256 is invalid")]
    ArchiveHash,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRolloutPolicy {
    pub schema_version: u8,
    pub release_id: Uuid,
    pub channel: UpdateChannel,
    pub target_version: ReleaseVersion,
    pub minimum_version: Option<ReleaseVersion>,
    pub rollout_basis_points: u16,
    pub paused: bool,
}

impl UpdateRolloutPolicy {
    /// Checks the policy schema, rollout bound, and version relationship.
    ///
    /// # Errors
    ///
    /// Returns [`UpdatePolicyError`] for an unsupported schema, more than a
    /// 100 percent rollout, or a minimum version above the target version.
    pub fn validate(&self) -> Result<(), UpdatePolicyError> {
        if self.schema_version != 1 {
            return Err(UpdatePolicyError::Schema);
        }
        if self.rollout_basis_points > MAX_ROLLOUT_BASIS_POINTS {
            return Err(UpdatePolicyError::Rollout);
        }
        if self
            .minimum_version
            .is_some_and(|minimum| minimum > self.target_version)
        {
            return Err(UpdatePolicyError::MinimumExceedsTarget);
        }
        Ok(())
    }

    /// Produces a deterministic decision for one node without mutating policy.
    ///
    /// # Errors
    ///
    /// Returns [`UpdatePolicyError`] when the policy itself is invalid.
    pub fn decide(
        &self,
        network_id: Uuid,
        node_id: [u8; 16],
        node_channel: UpdateChannel,
        current_version: ReleaseVersion,
    ) -> Result<UpdateDecision, UpdatePolicyError> {
        self.validate()?;
        if node_channel != self.channel {
            return Ok(UpdateDecision::ChannelMismatch);
        }
        if self.paused {
            return Ok(UpdateDecision::Paused);
        }
        if current_version >= self.target_version {
            return Ok(UpdateDecision::UpToDate);
        }
        if self
            .minimum_version
            .is_some_and(|minimum| current_version < minimum)
        {
            return Ok(UpdateDecision::Required);
        }
        if rollout_bucket(network_id, self.release_id, node_id) < self.rollout_basis_points.into() {
            Ok(UpdateDecision::Eligible)
        } else {
            Ok(UpdateDecision::Deferred)
        }
    }
}

fn rollout_bucket(network_id: Uuid, release_id: Uuid, node_id: [u8; 16]) -> u64 {
    let mut digest = Sha256::new();
    digest.update(UPDATE_BUCKET_DOMAIN);
    digest.update(network_id.as_bytes());
    digest.update(release_id.as_bytes());
    digest.update(node_id);
    let digest = digest.finalize();
    u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 prefix"))
        % u64::from(MAX_ROLLOUT_BASIS_POINTS)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateDecision {
    ChannelMismatch,
    Paused,
    UpToDate,
    Required,
    Eligible,
    Deferred,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum UpdatePolicyError {
    #[error("update policy schema is unsupported")]
    Schema,
    #[error("update rollout must be between 0 and 10000 basis points")]
    Rollout,
    #[error("minimum version cannot exceed target version")]
    MinimumExceedsTarget,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentUpdateState {
    Idle,
    Downloading,
    Staged,
    Applying,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRuntimeReport {
    pub schema_version: u8,
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub agent_version: ReleaseVersion,
    pub platform: String,
    pub architecture: String,
    pub update_channel: UpdateChannel,
    pub update_state: AgentUpdateState,
    pub observed_release_id: Option<Uuid>,
    pub last_error_code: Option<String>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// Encodes the domain-separated bytes signed by an Agent runtime report.
///
/// # Errors
///
/// Returns [`serde_json::Error`] if the bounded report cannot be serialized.
pub fn agent_runtime_report_signing_input(
    report: &AgentRuntimeReport,
) -> Result<Vec<u8>, serde_json::Error> {
    let payload = serde_json::to_vec(report)?;
    let mut signing_input = Vec::with_capacity(UPDATE_REPORT_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(UPDATE_REPORT_DOMAIN);
    signing_input.extend_from_slice(&payload);
    Ok(signing_input)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateDirective {
    pub schema_version: u8,
    pub policy_generation: u64,
    pub release_id: Uuid,
    pub decision: UpdateDecision,
    pub channel: UpdateChannel,
    pub version: ReleaseVersion,
    pub minimum_version: Option<ReleaseVersion>,
    pub platform: String,
    pub architecture: String,
    pub archive_name: String,
    pub archive_size: u64,
    pub archive_sha256: String,
    pub archive_url: String,
    pub manifest_base64: String,
    pub signature_base64: String,
}

#[cfg(test)]
mod tests {
    use super::{
        AgentRuntimeReport, AgentUpdateState, LinuxReleaseManifest, ReleaseManifestError,
        ReleaseVersion, UpdateChannel, UpdateDecision, UpdatePolicyError, UpdateRolloutPolicy,
        agent_runtime_report_signing_input, rollout_bucket,
    };
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    const VALID_MANIFEST: &str = concat!(
        "schema_version=2\n",
        "product=xs-nexus\n",
        "version=1.2.3\n",
        "source_commit=0123456789abcdef0123456789abcdef01234567\n",
        "source_date_epoch=1700000000\n",
        "protocol_version=XSP/1\n",
        "platform=linux\n",
        "architecture=x86_64\n",
        "target=x86_64-unknown-linux-gnu\n",
        "archive=xs-nexus-1.2.3-x86_64-unknown-linux-gnu.tar.gz\n",
        "archive_size=12345\n",
        "archive_sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
    );

    fn policy() -> UpdateRolloutPolicy {
        UpdateRolloutPolicy {
            schema_version: 1,
            release_id: Uuid::from_u128(0x1234),
            channel: UpdateChannel::Stable,
            target_version: ReleaseVersion::new(1, 2, 3),
            minimum_version: Some(ReleaseVersion::new(1, 1, 0)),
            rollout_basis_points: 5_000,
            paused: false,
        }
    }

    #[test]
    fn versions_are_canonical_and_ordered() {
        assert_eq!("1.2.3".parse(), Ok(ReleaseVersion::new(1, 2, 3)));
        assert!("1.2.3".parse::<ReleaseVersion>().expect("version") > ReleaseVersion::new(1, 2, 2));
        for invalid in ["", "1", "1.2", "1.2.3.4", "01.2.3", "1.-2.3", "v1.2.3"] {
            assert!(
                invalid.parse::<ReleaseVersion>().is_err(),
                "accepted {invalid}"
            );
        }
        assert_eq!(
            serde_json::to_string(&ReleaseVersion::new(1, 2, 3)).expect("serialize"),
            "\"1.2.3\""
        );
    }

    #[test]
    fn release_manifest_matches_the_installer_contract() {
        let manifest = LinuxReleaseManifest::parse(VALID_MANIFEST.as_bytes()).expect("manifest");
        assert_eq!(manifest.version, ReleaseVersion::new(1, 2, 3));
        assert_eq!(manifest.archive_size, 12_345);
        assert_eq!(manifest.architecture, "x86_64");
        assert_eq!(
            manifest.source_commit,
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(manifest.source_date_epoch, 1_700_000_000);
        assert_eq!(manifest.protocol_version, "XSP/1");
    }

    #[test]
    fn release_manifest_rejects_field_and_identity_drift() {
        let cases = [
            ("schema_version=1\n", ReleaseManifestError::FieldCount),
            (
                &VALID_MANIFEST.replace("schema_version=2", "schema_version=1"),
                ReleaseManifestError::Format,
            ),
            (
                &VALID_MANIFEST.replace("product=xs-nexus", "product=other"),
                ReleaseManifestError::Format,
            ),
            (
                &VALID_MANIFEST.replace("version=1.2.3", "version=01.2.3"),
                ReleaseManifestError::Version,
            ),
            (
                &VALID_MANIFEST.replace(
                    "source_commit=0123456789abcdef0123456789abcdef01234567",
                    "source_commit=0123456789abcdef0123456789abcdef0123456G",
                ),
                ReleaseManifestError::SourceCommit,
            ),
            (
                &VALID_MANIFEST.replace("source_date_epoch=1700000000", "source_date_epoch=0"),
                ReleaseManifestError::SourceDateEpoch,
            ),
            (
                &VALID_MANIFEST.replace("protocol_version=XSP/1", "protocol_version=XSP/0"),
                ReleaseManifestError::Format,
            ),
            (
                &VALID_MANIFEST.replace("architecture=x86_64", "architecture=amd64"),
                ReleaseManifestError::Architecture,
            ),
            (
                &VALID_MANIFEST.replace("target=x86_64", "target=aarch64"),
                ReleaseManifestError::Target,
            ),
            (
                &VALID_MANIFEST.replace("archive_size=12345", "archive_size=0"),
                ReleaseManifestError::ArchiveSize,
            ),
            (
                &VALID_MANIFEST.replace("0123456789abcdef", "G123456789abcdef"),
                ReleaseManifestError::ArchiveHash,
            ),
        ];
        for (encoded, expected) in cases {
            assert_eq!(
                LinuxReleaseManifest::parse(encoded.as_bytes()),
                Err(expected)
            );
        }
    }

    #[test]
    fn rollout_decision_enforces_pause_channel_and_minimum_version() {
        let network = Uuid::from_u128(7);
        let node = [9_u8; 16];
        let mut rollout = policy();
        assert_eq!(
            rollout.decide(
                network,
                node,
                UpdateChannel::Testing,
                ReleaseVersion::new(1, 0, 0)
            ),
            Ok(UpdateDecision::ChannelMismatch)
        );
        rollout.paused = true;
        assert_eq!(
            rollout.decide(
                network,
                node,
                UpdateChannel::Stable,
                ReleaseVersion::new(1, 0, 0)
            ),
            Ok(UpdateDecision::Paused)
        );
        rollout.paused = false;
        assert_eq!(
            rollout.decide(
                network,
                node,
                UpdateChannel::Stable,
                ReleaseVersion::new(1, 0, 0)
            ),
            Ok(UpdateDecision::Required)
        );
        assert_eq!(
            rollout.decide(
                network,
                node,
                UpdateChannel::Stable,
                ReleaseVersion::new(1, 2, 3)
            ),
            Ok(UpdateDecision::UpToDate)
        );
        assert_eq!(
            rollout.decide(
                network,
                node,
                UpdateChannel::Stable,
                ReleaseVersion::new(2, 0, 0)
            ),
            Ok(UpdateDecision::UpToDate)
        );
    }

    #[test]
    fn rollout_bucket_is_stable_and_percentage_is_bounded() {
        let network = Uuid::from_u128(8);
        let mut rollout = policy();
        rollout.minimum_version = None;
        let mut eligible = 0;
        let mut deferred = 0;
        for value in 0_u128..512 {
            let node = value.to_be_bytes();
            let first = rollout_bucket(network, rollout.release_id, node);
            assert_eq!(first, rollout_bucket(network, rollout.release_id, node));
            match rollout
                .decide(
                    network,
                    node,
                    UpdateChannel::Stable,
                    ReleaseVersion::new(1, 2, 0),
                )
                .expect("decision")
            {
                UpdateDecision::Eligible => eligible += 1,
                UpdateDecision::Deferred => deferred += 1,
                other => panic!("unexpected decision: {other:?}"),
            }
        }
        assert!(eligible > 180 && eligible < 332, "eligible={eligible}");
        assert!(deferred > 180 && deferred < 332, "deferred={deferred}");

        rollout.rollout_basis_points = 0;
        assert_eq!(
            rollout.decide(
                network,
                [1; 16],
                UpdateChannel::Stable,
                ReleaseVersion::new(1, 2, 0)
            ),
            Ok(UpdateDecision::Deferred)
        );
        rollout.rollout_basis_points = 10_000;
        assert_eq!(
            rollout.decide(
                network,
                [1; 16],
                UpdateChannel::Stable,
                ReleaseVersion::new(1, 2, 0)
            ),
            Ok(UpdateDecision::Eligible)
        );
        rollout.rollout_basis_points = 10_001;
        assert_eq!(rollout.validate(), Err(UpdatePolicyError::Rollout));
    }

    #[test]
    fn policy_rejects_an_impossible_minimum_version() {
        let mut rollout = policy();
        rollout.minimum_version = Some(ReleaseVersion::new(2, 0, 0));
        assert_eq!(
            rollout.validate(),
            Err(UpdatePolicyError::MinimumExceedsTarget)
        );
    }

    #[test]
    fn runtime_report_signature_input_is_domain_separated_and_stable() {
        let report = AgentRuntimeReport {
            schema_version: 1,
            network_id: Uuid::from_u128(9),
            node_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ".to_owned(),
            agent_version: ReleaseVersion::new(1, 2, 3),
            platform: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            update_channel: UpdateChannel::Stable,
            update_state: AgentUpdateState::Idle,
            observed_release_id: None,
            last_error_code: None,
            generated_at: Utc.timestamp_opt(1_800_000_000, 0).single().expect("time"),
        };
        let encoded = agent_runtime_report_signing_input(&report).expect("signing input");
        assert!(encoded.starts_with(b"XS Nexus Agent runtime report v1{"));
        assert!(
            encoded
                .windows(b"\"agent_version\":\"1.2.3\"".len())
                .any(|window| { window == b"\"agent_version\":\"1.2.3\"" })
        );
        let mut changed = report;
        changed.update_channel = UpdateChannel::Testing;
        assert_ne!(
            encoded,
            agent_runtime_report_signing_input(&changed).expect("changed signing input")
        );
    }
}
