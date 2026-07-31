#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

pub const AGENT_PIPE_NAME: &str = r"\\.\pipe\xs-nexus-agent";

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform;

#[cfg(windows)]
pub use platform::create_agent_pipe_server;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_pipe_name_is_fixed_and_local() {
        assert_eq!(AGENT_PIPE_NAME, r"\\.\pipe\xs-nexus-agent");
        assert!(!AGENT_PIPE_NAME.contains(".."));
    }
}
