use std::{ffi::c_void, io, iter, ptr::null_mut};

use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    },
};

use crate::AGENT_PIPE_NAME;

const PRIVATE_PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)";
const MAX_PIPE_INSTANCES: usize = 17;
const MAX_REQUEST_BYTES: u32 = 4096;
const MAX_RESPONSE_BYTES: u32 = 512 * 1024;

struct OwnedSecurityDescriptor(PSECURITY_DESCRIPTOR);

impl OwnedSecurityDescriptor {
    fn private_pipe() -> io::Result<Self> {
        let wide = PRIVATE_PIPE_SDDL
            .encode_utf16()
            .chain(iter::once(0))
            .collect::<Vec<_>>();
        let mut descriptor = null_mut();
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                null_mut(),
            )
        };
        if converted == 0 || descriptor.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for OwnedSecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

/// Creates one bounded local Agent pipe instance with a protected DACL.
///
/// # Errors
///
/// Returns an OS error when the security descriptor or pipe instance cannot be created.
pub fn create_agent_pipe_server(first_instance: bool) -> io::Result<NamedPipeServer> {
    let descriptor = OwnedSecurityDescriptor::private_pipe()?;
    let n_length = u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: n_length,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let mut options = ServerOptions::new();
    options
        .pipe_mode(PipeMode::Byte)
        .access_inbound(true)
        .access_outbound(true)
        .first_pipe_instance(first_instance)
        .reject_remote_clients(true)
        .max_instances(MAX_PIPE_INSTANCES)
        .in_buffer_size(MAX_REQUEST_BYTES)
        .out_buffer_size(MAX_RESPONSE_BYTES);
    unsafe {
        options.create_with_security_attributes_raw(
            AGENT_PIPE_NAME,
            (&raw mut attributes).cast::<c_void>(),
        )
    }
}
