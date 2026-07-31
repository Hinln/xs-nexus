use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::ptr::{null, null_mut};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_ListW, CR_SUCCESS,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::core::GUID;

use crate::{
    IDENTITY_RESPONSE_SIZE, IOCTL_QUERY_IDENTITY, OpenError, Outcome, Request, RequestKind,
    parse_identity_response, parse_single_interface,
};

const IDENTITY_REQUEST: [u8; 8] = [1, 0, 8, 0, 0, 0, 0, 0];

const MAX_INTERFACE_LIST_CHARS: u32 = 32_768;
const GUID_DEVINTERFACE_XSNET: GUID = GUID {
    data1: 0x09a0_4405,
    data2: 0x8a02,
    data3: 0x4398,
    data4: [0xbb, 0xe0, 0x67, 0xed, 0x8a, 0x7b, 0x9e, 0x38],
};

#[derive(Debug)]
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[derive(Debug)]
pub struct DeviceTransport {
    handle: OwnedHandle,
    interface_luid: u64,
}

impl DeviceTransport {
    /// Enumerates exactly one present xsnet interface and opens it synchronously without sharing.
    ///
    /// # Errors
    ///
    /// Returns an error for enumeration failure, malformed or ambiguous paths, or open failure.
    pub fn open() -> Result<Self, OpenError> {
        let mut character_count = 0_u32;
        let size_result = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &raw mut character_count,
                &GUID_DEVINTERFACE_XSNET,
                null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if size_result != CR_SUCCESS || !(2..=MAX_INTERFACE_LIST_CHARS).contains(&character_count) {
            return Err(OpenError::EnumerationFailed);
        }
        let mut interfaces = vec![0_u16; character_count as usize];
        let list_result = unsafe {
            CM_Get_Device_Interface_ListW(
                &GUID_DEVINTERFACE_XSNET,
                null(),
                interfaces.as_mut_ptr(),
                character_count,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if list_result != CR_SUCCESS {
            return Err(OpenError::EnumerationFailed);
        }
        let interface = parse_single_interface(&interfaces)?;
        let handle = unsafe {
            CreateFileW(
                interface.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err(OpenError::OpenFailed);
        }
        let handle = OwnedHandle(handle);
        let mut response = [0_u8; IDENTITY_RESPONSE_SIZE];
        let mut bytes_returned = 0_u32;
        let succeeded = unsafe {
            DeviceIoControl(
                handle.0,
                IOCTL_QUERY_IDENTITY,
                IDENTITY_REQUEST.as_ptr().cast::<c_void>(),
                8,
                response.as_mut_ptr().cast::<c_void>(),
                16,
                &raw mut bytes_returned,
                null_mut(),
            )
        } != 0;
        if !succeeded || bytes_returned as usize != response.len() {
            return Err(OpenError::IdentityQueryFailed);
        }
        let interface_luid = parse_identity_response(&response)?;
        Ok(Self {
            handle,
            interface_luid,
        })
    }

    #[must_use]
    pub const fn interface_luid(&self) -> u64 {
        self.interface_luid
    }

    pub fn execute(&mut self, request: Request<'_>) -> Outcome {
        let buffered = request.buffered_input();
        let mut direct_input = request.direct_input().to_vec();
        let mut direct_output = vec![0_u8; request.direct_output_capacity()];
        let (input_pointer, input_length, output_pointer, output_length) = match request.kind() {
            RequestKind::Buffered => (
                buffered.as_ptr().cast::<c_void>(),
                buffered.len(),
                null_mut(),
                0,
            ),
            RequestKind::Dequeue => (
                buffered.as_ptr().cast::<c_void>(),
                buffered.len(),
                direct_output.as_mut_ptr().cast::<c_void>(),
                direct_output.len(),
            ),
            RequestKind::Enqueue => (
                null(),
                0,
                direct_input.as_mut_ptr().cast::<c_void>(),
                direct_input.len(),
            ),
        };
        let Ok(input_length) = u32::try_from(input_length) else {
            return Outcome::Indeterminate;
        };
        let Ok(output_length) = u32::try_from(output_length) else {
            return Outcome::Indeterminate;
        };
        let mut bytes_returned = 0_u32;
        let succeeded = unsafe {
            DeviceIoControl(
                self.handle.0,
                request.ioctl(),
                input_pointer,
                input_length,
                output_pointer,
                output_length,
                &raw mut bytes_returned,
                null_mut(),
            )
        } != 0;
        if !succeeded {
            return Outcome::Indeterminate;
        }
        match request.kind() {
            RequestKind::Buffered if bytes_returned == 0 => Outcome::Success(Vec::new()),
            RequestKind::Dequeue => {
                let Ok(length) = usize::try_from(bytes_returned) else {
                    return Outcome::Indeterminate;
                };
                if length > direct_output.len() {
                    return Outcome::Indeterminate;
                }
                direct_output.truncate(length);
                Outcome::Success(direct_output)
            }
            RequestKind::Enqueue
                if bytes_returned == 0 && direct_input.as_slice() == request.direct_input() =>
            {
                Outcome::Success(Vec::new())
            }
            RequestKind::Buffered | RequestKind::Enqueue => Outcome::Indeterminate,
        }
    }
}
