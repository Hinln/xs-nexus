#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::{error::Error, fmt};

const MAX_SERVICE_NAME_UNITS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceError {
    UnsupportedPlatform,
    InvalidName,
    AlreadyInitialized,
    DispatcherFailed,
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedPlatform => "Windows Service is unavailable",
            Self::InvalidName => "Windows Service name is invalid",
            Self::AlreadyInitialized => "Windows Service host is already initialized",
            Self::DispatcherFailed => "Windows Service dispatcher failed",
        })
    }
}

impl Error for ServiceError {}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform;

#[cfg(windows)]
pub use platform::{ServiceShutdown, run_service};

fn validate_service_name(name: &str) -> Result<(), ServiceError> {
    let units = name.encode_utf16().count();
    if units == 0
        || units > MAX_SERVICE_NAME_UNITS
        || name
            .chars()
            .any(|character| character == '\0' || character == '/' || character == '\\')
    {
        return Err(ServiceError::InvalidName);
    }
    Ok(())
}

#[cfg(not(windows))]
#[derive(Clone, Copy, Debug)]
pub struct ServiceShutdown;

#[cfg(not(windows))]
impl ServiceShutdown {
    pub async fn cancelled(self) {
        std::future::pending::<()>().await;
    }
}

#[cfg(not(windows))]
/// Rejects Windows Service hosting on non-Windows platforms.
///
/// # Errors
///
/// Returns [`ServiceError::InvalidName`] for an invalid service name and
/// [`ServiceError::UnsupportedPlatform`] for every valid name.
pub fn run_service<F>(name: &str, _handler: F) -> Result<(), ServiceError>
where
    F: FnOnce(ServiceShutdown) -> bool + Send + 'static,
{
    validate_service_name(name)?;
    Err(ServiceError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::{ServiceError, validate_service_name};

    #[test]
    fn service_name_is_fixed_width_and_path_free() {
        assert_eq!(validate_service_name("XsNexusAgent"), Ok(()));
        assert_eq!(validate_service_name(""), Err(ServiceError::InvalidName));
        assert_eq!(
            validate_service_name("xs/nexus"),
            Err(ServiceError::InvalidName)
        );
        assert_eq!(
            validate_service_name("xs\\nexus"),
            Err(ServiceError::InvalidName)
        );
        assert_eq!(
            validate_service_name(&"x".repeat(257)),
            Err(ServiceError::InvalidName)
        );
    }
}
