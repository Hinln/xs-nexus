use std::{
    ffi::c_void,
    path::Path,
    ptr::{null, null_mut},
    rc::Rc,
    sync::Mutex,
};

use windows_sys::Win32::{
    Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, FreeLibrary, GetLastError, HMODULE},
    NetworkManagement::{IpHelper::ConvertInterfaceLuidToIndex, Ndis::NET_LUID_LH},
    System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LoadLibraryExW},
};

use crate::{MAX_PACKET_SIZE, WintunError, verify_library};

type AdapterHandle = *mut c_void;
type SessionHandle = *mut c_void;
type Procedure = unsafe extern "system" fn() -> isize;
type CreateAdapter =
    unsafe extern "system" fn(*const u16, *const u16, *const c_void) -> AdapterHandle;
type CloseAdapter = unsafe extern "system" fn(AdapterHandle);
type GetAdapterLuid = unsafe extern "system" fn(AdapterHandle, *mut NET_LUID_LH);
type StartSession = unsafe extern "system" fn(AdapterHandle, u32) -> SessionHandle;
type EndSession = unsafe extern "system" fn(SessionHandle);
type ReceivePacket = unsafe extern "system" fn(SessionHandle, *mut u32) -> *mut u8;
type ReleaseReceivePacket = unsafe extern "system" fn(SessionHandle, *const u8);
type AllocateSendPacket = unsafe extern "system" fn(SessionHandle, u32) -> *mut u8;
type SendPacket = unsafe extern "system" fn(SessionHandle, *const u8);

struct Api {
    library: HMODULE,
    create_adapter: CreateAdapter,
    close_adapter: CloseAdapter,
    get_adapter_luid: GetAdapterLuid,
    start_session: StartSession,
    end_session: EndSession,
    receive_packet: ReceivePacket,
    release_receive_packet: ReleaseReceivePacket,
    allocate_send_packet: AllocateSendPacket,
    send_packet: SendPacket,
}

impl Drop for Api {
    fn drop(&mut self) {
        if !self.library.is_null() {
            unsafe { FreeLibrary(self.library) };
        }
    }
}

struct Inner {
    adapter: AdapterHandle,
    session: SessionHandle,
}

/// Owns one ephemeral Wintun adapter and one bounded packet session.
///
/// The adapter is created only after the caller pins the exact library bytes. All API access is
/// serialized because XS Nexus has exactly one local data-plane owner.
pub struct WintunSession {
    api: Rc<Api>,
    inner: Mutex<Inner>,
    interface_luid: u64,
    interface_index: u32,
}

impl core::fmt::Debug for WintunSession {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WintunSession")
            .field("interface_luid", &self.interface_luid)
            .finish_non_exhaustive()
    }
}

impl WintunSession {
    /// Loads a caller-pinned vendor library, then creates a new ephemeral adapter and session.
    ///
    /// # Errors
    ///
    /// Rejects unpinned libraries, invalid names or capacity, failed API resolution, adapter
    /// creation, invalid LUIDs, and failed session creation.
    pub fn create(
        library_path: &Path,
        expected_sha256: &[u8; 32],
        adapter_name: &str,
        tunnel_type: &str,
        ring_capacity: u32,
    ) -> Result<Self, WintunError> {
        verify_library(library_path, expected_sha256)?;
        if !valid_label(adapter_name)
            || !valid_label(tunnel_type)
            || !valid_ring_capacity(ring_capacity)
        {
            return Err(if valid_ring_capacity(ring_capacity) {
                WintunError::InvalidAdapterName
            } else {
                WintunError::InvalidRingCapacity
            });
        }
        let api = Rc::new(Api::load(library_path)?);
        let adapter_name = wide(adapter_name);
        let tunnel_type = wide(tunnel_type);
        let adapter =
            unsafe { (api.create_adapter)(adapter_name.as_ptr(), tunnel_type.as_ptr(), null()) };
        if adapter.is_null() {
            return Err(WintunError::Win32(unsafe { GetLastError() }));
        }
        let mut luid = NET_LUID_LH { Value: 0 };
        unsafe { (api.get_adapter_luid)(adapter, &raw mut luid) };
        if unsafe { luid.Value } == 0 {
            unsafe { (api.close_adapter)(adapter) };
            return Err(WintunError::Win32(13));
        }
        let mut interface_index = 0_u32;
        if unsafe { ConvertInterfaceLuidToIndex(&raw const luid, &raw mut interface_index) }
            != ERROR_SUCCESS
            || interface_index == 0
        {
            unsafe { (api.close_adapter)(adapter) };
            return Err(WintunError::Win32(13));
        }
        let session = unsafe { (api.start_session)(adapter, ring_capacity) };
        if session.is_null() {
            let error = unsafe { GetLastError() };
            unsafe { (api.close_adapter)(adapter) };
            return Err(WintunError::Win32(error));
        }
        Ok(Self {
            api,
            inner: Mutex::new(Inner { adapter, session }),
            interface_luid: unsafe { luid.Value },
            interface_index,
        })
    }

    #[must_use]
    pub const fn interface_luid(&self) -> u64 {
        self.interface_luid
    }

    #[must_use]
    pub const fn interface_index(&self) -> u32 {
        self.interface_index
    }

    /// Copies one complete packet when currently available, without blocking or retaining a ring
    /// pointer after the API call returns.
    ///
    /// # Errors
    ///
    /// Returns [`Ok(None)`] only for Wintun's authoritative empty-ring status. Any malformed or
    /// oversized packet fails closed after releasing the vendor-owned ring allocation.
    pub fn try_receive(&self, buffer: &mut [u8]) -> Result<Option<usize>, WintunError> {
        let inner = self.inner.lock().map_err(|_| WintunError::Win32(31))?;
        let mut length = 0_u32;
        let packet = unsafe { (self.api.receive_packet)(inner.session, &raw mut length) };
        if packet.is_null() {
            let error = unsafe { GetLastError() };
            return if error == ERROR_NO_MORE_ITEMS {
                Ok(None)
            } else {
                Err(WintunError::Win32(error))
            };
        }
        let length = usize::try_from(length).map_err(|_| WintunError::InvalidPacket)?;
        if length == 0 || length > MAX_PACKET_SIZE || length > buffer.len() {
            unsafe { (self.api.release_receive_packet)(inner.session, packet) };
            return Err(WintunError::InvalidPacket);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(packet, buffer.as_mut_ptr(), length);
            (self.api.release_receive_packet)(inner.session, packet);
        }
        Ok(Some(length))
    }

    /// Copies one bounded raw IP packet to the Wintun send ring.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized packets and preserves explicit vendor queue-full errors.
    pub fn send(&self, packet: &[u8]) -> Result<(), WintunError> {
        if packet.is_empty() || packet.len() > MAX_PACKET_SIZE {
            return Err(WintunError::InvalidPacket);
        }
        let length = u32::try_from(packet.len()).map_err(|_| WintunError::InvalidPacket)?;
        let inner = self.inner.lock().map_err(|_| WintunError::Win32(31))?;
        let destination = unsafe { (self.api.allocate_send_packet)(inner.session, length) };
        if destination.is_null() {
            return Err(WintunError::Win32(unsafe { GetLastError() }));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(packet.as_ptr(), destination, packet.len());
            (self.api.send_packet)(inner.session, destination);
        }
        Ok(())
    }
}

impl Drop for WintunSession {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            if !inner.session.is_null() {
                unsafe { (self.api.end_session)(inner.session) };
                inner.session = null_mut();
            }
            if !inner.adapter.is_null() {
                unsafe { (self.api.close_adapter)(inner.adapter) };
                inner.adapter = null_mut();
            }
        }
    }
}

impl Api {
    fn load(path: &Path) -> Result<Self, WintunError> {
        let path = wide_path(path)?;
        let library =
            unsafe { LoadLibraryExW(path.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR) };
        if library.is_null() {
            return Err(WintunError::Win32(unsafe { GetLastError() }));
        }
        let result = Self::resolve(library);
        if result.is_err() {
            unsafe { FreeLibrary(library) };
        }
        result
    }

    fn resolve(library: HMODULE) -> Result<Self, WintunError> {
        Ok(Self {
            library,
            create_adapter: resolve_create_adapter(library)?,
            close_adapter: resolve_close_adapter(library)?,
            get_adapter_luid: resolve_get_adapter_luid(library)?,
            start_session: resolve_start_session(library)?,
            end_session: resolve_end_session(library)?,
            receive_packet: resolve_receive_packet(library)?,
            release_receive_packet: resolve_release_receive_packet(library)?,
            allocate_send_packet: resolve_allocate_send_packet(library)?,
            send_packet: resolve_send_packet(library)?,
        })
    }
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.encode_utf16().count() <= 128
        && !value
            .chars()
            .any(|character| character == '\0' || character.is_control())
}

const fn valid_ring_capacity(capacity: u32) -> bool {
    capacity >= crate::MIN_RING_CAPACITY
        && capacity <= crate::MAX_RING_CAPACITY
        && capacity.is_power_of_two()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn wide_path(path: &Path) -> Result<Vec<u16>, WintunError> {
    use std::os::windows::ffi::OsStrExt as _;

    let value = path
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    if value.len() <= 1 || value.len() > 32_768 {
        return Err(WintunError::InvalidLibraryPath);
    }
    Ok(value)
}

fn proc(library: HMODULE, name: &'static [u8]) -> Result<Procedure, WintunError> {
    let function = unsafe { GetProcAddress(library, name.as_ptr()) };
    function.ok_or(WintunError::Win32(127))
}

fn resolve_create_adapter(library: HMODULE) -> Result<CreateAdapter, WintunError> {
    let function = proc(library, b"WintunCreateAdapter\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, CreateAdapter>(function) })
}

fn resolve_close_adapter(library: HMODULE) -> Result<CloseAdapter, WintunError> {
    let function = proc(library, b"WintunCloseAdapter\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, CloseAdapter>(function) })
}

fn resolve_get_adapter_luid(library: HMODULE) -> Result<GetAdapterLuid, WintunError> {
    let function = proc(library, b"WintunGetAdapterLUID\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, GetAdapterLuid>(function) })
}

fn resolve_start_session(library: HMODULE) -> Result<StartSession, WintunError> {
    let function = proc(library, b"WintunStartSession\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, StartSession>(function) })
}

fn resolve_end_session(library: HMODULE) -> Result<EndSession, WintunError> {
    let function = proc(library, b"WintunEndSession\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, EndSession>(function) })
}

fn resolve_receive_packet(library: HMODULE) -> Result<ReceivePacket, WintunError> {
    let function = proc(library, b"WintunReceivePacket\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, ReceivePacket>(function) })
}

fn resolve_release_receive_packet(library: HMODULE) -> Result<ReleaseReceivePacket, WintunError> {
    let function = proc(library, b"WintunReleaseReceivePacket\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, ReleaseReceivePacket>(function) })
}

fn resolve_allocate_send_packet(library: HMODULE) -> Result<AllocateSendPacket, WintunError> {
    let function = proc(library, b"WintunAllocateSendPacket\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, AllocateSendPacket>(function) })
}

fn resolve_send_packet(library: HMODULE) -> Result<SendPacket, WintunError> {
    let function = proc(library, b"WintunSendPacket\0")?;
    Ok(unsafe { std::mem::transmute::<Procedure, SendPacket>(function) })
}
