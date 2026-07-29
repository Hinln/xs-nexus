use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("invalid agent configuration")]
    Configuration,
    #[error("local state is unavailable or invalid")]
    State,
    #[error("enrollment was rejected")]
    Enrollment,
    #[error("controller response failed validation")]
    ControllerTrust,
    #[error("controller connection is unavailable")]
    Control,
    #[error("network resource operation failed")]
    Network,
    #[error("encrypted data plane operation failed")]
    DataPlane,
    #[error("local management interface failed")]
    Ipc,
    #[error("agent runtime failed")]
    Runtime,
    #[error("requested operation is unsupported on this platform")]
    UnsupportedPlatform,
}

impl AgentError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Configuration => "agent_configuration_invalid",
            Self::State => "agent_state_invalid",
            Self::Enrollment => "agent_enrollment_rejected",
            Self::ControllerTrust => "agent_controller_trust_failed",
            Self::Control => "agent_control_unavailable",
            Self::Network => "agent_network_operation_failed",
            Self::DataPlane => "agent_data_plane_failed",
            Self::Ipc => "agent_ipc_failed",
            Self::Runtime => "agent_runtime_failed",
            Self::UnsupportedPlatform => "agent_platform_unsupported",
        }
    }
}

pub type Result<T> = std::result::Result<T, AgentError>;
