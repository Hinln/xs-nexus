use std::{
    net::IpAddr,
    path::{Path, PathBuf},
};

use reqwest::Url;
use serde::Deserialize;

use crate::error::{AgentError, Result};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub controller_url: String,
    pub node_name: String,
    pub device_type: String,
    pub state_directory: PathBuf,
    pub runtime_directory: PathBuf,
    #[serde(default = "default_interface_name")]
    pub interface_name: String,
    #[serde(default = "default_mtu")]
    pub mtu: u16,
    #[serde(default = "default_sync_interval")]
    pub control_sync_interval_seconds: u64,
}

const fn default_mtu() -> u16 {
    1280
}

fn default_interface_name() -> String {
    "xsn0".to_owned()
}

const fn default_sync_interval() -> u64 {
    15
}

impl AgentConfig {
    /// Loads and validates a bounded JSON configuration file.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Configuration`] when the file is not a regular bounded file,
    /// contains invalid JSON, or violates the Agent configuration policy.
    pub fn load(path: &Path) -> Result<Self> {
        let metadata = path
            .symlink_metadata()
            .map_err(|_| AgentError::Configuration)?;
        if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
            return Err(AgentError::Configuration);
        }
        let bytes = std::fs::read(path).map_err(|_| AgentError::Configuration)?;
        let config: Self = serde_json::from_slice(&bytes).map_err(|_| AgentError::Configuration)?;
        config.validate()?;
        Ok(config)
    }

    /// Validates endpoint, path, name, interface, MTU, and timing constraints.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Configuration`] when any constraint is violated.
    pub fn validate(&self) -> Result<()> {
        let url = self.controller_url()?;
        if url.username() != ""
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
            || self.node_name.is_empty()
            || self.node_name.len() > 63
            || !self
                .node_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || self.device_type.is_empty()
            || self.device_type.len() > 32
            || !self.device_type.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
            || !self.state_directory.is_absolute()
            || !self.runtime_directory.is_absolute()
            || self.state_directory == self.runtime_directory
            || !valid_interface_name(&self.interface_name)
            || !(1280..=1500).contains(&self.mtu)
            || !(5..=300).contains(&self.control_sync_interval_seconds)
        {
            return Err(AgentError::Configuration);
        }
        Ok(())
    }

    /// Returns the validated HTTP Controller base URL.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Configuration`] for malformed URLs or non-loopback plaintext HTTP.
    pub fn controller_url(&self) -> Result<Url> {
        let url = Url::parse(&self.controller_url).map_err(|_| AgentError::Configuration)?;
        match url.scheme() {
            "https" => Ok(url),
            "http" if loopback_host(&url) => Ok(url),
            _ => Err(AgentError::Configuration),
        }
    }

    /// Returns the validated enrollment endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Configuration`] when the Controller URL is invalid.
    pub fn enrollment_url(&self) -> Result<Url> {
        self.controller_url()?
            .join("v1/enroll")
            .map_err(|_| AgentError::Configuration)
    }

    /// Returns the validated WebSocket control endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Configuration`] when the Controller URL is invalid.
    pub fn control_url(&self) -> Result<Url> {
        let mut url = self
            .controller_url()?
            .join("v1/control")
            .map_err(|_| AgentError::Configuration)?;
        let scheme = match url.scheme() {
            "https" => "wss",
            "http" => "ws",
            _ => return Err(AgentError::Configuration),
        };
        url.set_scheme(scheme)
            .map_err(|()| AgentError::Configuration)?;
        Ok(url)
    }

    #[must_use]
    pub fn identity_path(&self) -> PathBuf {
        self.state_directory.join("identity.key")
    }

    #[must_use]
    pub fn node_state_path(&self) -> PathBuf {
        self.state_directory.join("node-state.json")
    }

    #[must_use]
    pub fn network_manifest_path(&self) -> PathBuf {
        self.state_directory.join("network-manifest.json")
    }

    #[must_use]
    pub fn network_manifest_temporary_path(&self) -> PathBuf {
        self.state_directory.join("network-manifest.tmp")
    }

    #[must_use]
    #[cfg(unix)]
    pub fn socket_path(&self) -> PathBuf {
        self.runtime_directory.join("agent.sock")
    }

    #[must_use]
    #[cfg(windows)]
    pub fn socket_path(&self) -> PathBuf {
        PathBuf::from(xs_windows_local_ipc::AGENT_PIPE_NAME)
    }
}

fn loopback_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host == "localhost"
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn valid_interface_name(name: &str) -> bool {
    (3..=15).contains(&name.len())
        && name.starts_with("xs")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> AgentConfig {
        AgentConfig {
            controller_url: "https://controller.example/".to_owned(),
            node_name: "linux-node-1".to_owned(),
            device_type: "linux".to_owned(),
            state_directory: PathBuf::from("/var/lib/xs-nexus"),
            runtime_directory: PathBuf::from("/run/xs-nexus"),
            interface_name: "xsn0".to_owned(),
            mtu: 1280,
            control_sync_interval_seconds: 15,
        }
    }

    #[test]
    fn config_requires_tls_except_loopback() {
        assert!(fixture().validate().is_ok());
        let mut insecure = fixture();
        insecure.controller_url = "http://controller.example/".to_owned();
        assert!(insecure.validate().is_err());
        insecure.controller_url = "http://127.0.0.1:8080/".to_owned();
        assert!(insecure.validate().is_ok());
    }

    #[test]
    fn config_rejects_unsafe_interface_and_default_mtu() {
        let mut config = fixture();
        config.interface_name = "eth0".to_owned();
        assert!(config.validate().is_err());
        config.interface_name = "xsn0".to_owned();
        config.mtu = 1200;
        assert!(config.validate().is_err());
    }
}
