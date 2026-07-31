use std::{io, net::Ipv4Addr, path::Path, ptr::null_mut, slice, thread, time::Duration};

use ipnet::Ipv4Net;
use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, ERROR_SUCCESS},
    NetworkManagement::{
        IpHelper::{
            CreateIpForwardEntry2, CreateUnicastIpAddressEntry, DeleteIpForwardEntry2,
            DeleteUnicastIpAddressEntry, FreeMibTable, GetIpForwardTable2,
            GetUnicastIpAddressEntry, InitializeIpForwardEntry, InitializeUnicastIpAddressEntry,
            MIB_IPFORWARD_ROW2, MIB_IPFORWARD_TABLE2, MIB_UNICASTIPADDRESS_ROW,
        },
        Ndis::NET_LUID_LH,
    },
    Networking::WinSock::{
        AF_INET, IpDadStateDeprecated as IP_DAD_STATE_DEPRECATED,
        IpDadStateDuplicate as IP_DAD_STATE_DUPLICATE, IpDadStateInvalid as IP_DAD_STATE_INVALID,
        IpDadStatePreferred as IP_DAD_STATE_PREFERRED,
        IpDadStateTentative as IP_DAD_STATE_TENTATIVE, MIB_IPPROTO_NETMGMT, NlroManual,
        SOCKADDR_IN, SOCKADDR_INET,
    },
};
use xs_windows_private_storage::{read_private, remove_private, write_private_atomic};

use crate::{
    DadState, HostNetworkBackend, MAX_SYSTEM_ROUTES, PROJECT_ROUTE_METRIC, RouteBackend, RouteKey,
    SystemRoute,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpHelperError(pub u32);

impl From<IpHelperError> for io::Error {
    fn from(value: IpHelperError) -> Self {
        Self::from_raw_os_error(i32::try_from(value.0).unwrap_or(i32::MAX))
    }
}

pub struct IpHelperBackend;

impl RouteBackend for IpHelperBackend {
    type Error = IpHelperError;

    fn create(&mut self, route: RouteKey) -> Result<(), Self::Error> {
        let row = route_row(route);
        win_result(unsafe { CreateIpForwardEntry2(&raw const row) })
    }

    fn delete(&mut self, route: RouteKey) -> Result<(), Self::Error> {
        let row = route_row(route);
        win_result(unsafe { DeleteIpForwardEntry2(&raw const row) })
    }
}

impl HostNetworkBackend for IpHelperBackend {
    fn create_address(
        &mut self,
        interface_luid: u64,
        address: Ipv4Addr,
        prefix_length: u8,
    ) -> Result<(), Self::Error> {
        create_address(interface_luid, address, prefix_length)
    }

    fn delete_address(
        &mut self,
        interface_luid: u64,
        address: Ipv4Addr,
        prefix_length: u8,
    ) -> Result<(), Self::Error> {
        delete_address(interface_luid, address, prefix_length)
    }

    fn dad_state(
        &mut self,
        interface_luid: u64,
        address: Ipv4Addr,
        prefix_length: u8,
    ) -> Result<DadState, Self::Error> {
        query_dad_state(interface_luid, address, prefix_length)
    }

    fn wait_dad_poll(&mut self) {
        thread::sleep(Duration::from_millis(100));
    }
}

struct OwnedForwardTable(*mut MIB_IPFORWARD_TABLE2);

impl Drop for OwnedForwardTable {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FreeMibTable(self.0.cast()) };
        }
    }
}

/// Copies a bounded IPv4 route table before releasing the native allocation.
///
/// # Errors
///
/// Rejects IP Helper failures, null tables, oversized tables, malformed IPv4 rows, and routes
/// whose prefix cannot be represented canonically.
pub fn snapshot_routes() -> Result<Vec<SystemRoute>, IpHelperError> {
    let mut raw = null_mut();
    win_result(unsafe { GetIpForwardTable2(AF_INET, &raw mut raw) })?;
    if raw.is_null() {
        return Err(IpHelperError(13));
    }
    let owned = OwnedForwardTable(raw);
    let count = unsafe { (*owned.0).NumEntries as usize };
    if count > MAX_SYSTEM_ROUTES {
        return Err(IpHelperError(534));
    }
    let first = unsafe { (*owned.0).Table.as_ptr() };
    let rows = unsafe { slice::from_raw_parts(first, count) };
    rows.iter().map(system_route).collect()
}

/// Creates one non-persistent IPv4 address on the exact interface LUID.
///
/// # Errors
///
/// Returns validation or IP Helper failure without treating an existing address as success.
pub fn create_address(
    interface_luid: u64,
    address: Ipv4Addr,
    prefix_length: u8,
) -> Result<(), IpHelperError> {
    let row = address_row(interface_luid, address, prefix_length)?;
    win_result(unsafe { CreateUnicastIpAddressEntry(&raw const row) })
}

/// Deletes the exact IPv4 address previously recorded by the project manifest.
///
/// # Errors
///
/// Returns validation or IP Helper failure; no interface-wide cleanup is attempted.
pub fn delete_address(
    interface_luid: u64,
    address: Ipv4Addr,
    prefix_length: u8,
) -> Result<(), IpHelperError> {
    let row = address_row(interface_luid, address, prefix_length)?;
    win_result(unsafe { DeleteUnicastIpAddressEntry(&raw const row) })
}

/// Reads the authoritative DAD state for one exact IPv4 address.
///
/// # Errors
///
/// Returns validation or IP Helper failure. Callers must not claim availability before
/// [`DadState::Preferred`].
pub fn query_dad_state(
    interface_luid: u64,
    address: Ipv4Addr,
    prefix_length: u8,
) -> Result<DadState, IpHelperError> {
    let mut row = address_row(interface_luid, address, prefix_length)?;
    win_result(unsafe { GetUnicastIpAddressEntry(&raw mut row) })?;
    Ok(match row.DadState {
        IP_DAD_STATE_INVALID => DadState::Invalid,
        IP_DAD_STATE_TENTATIVE => DadState::Tentative,
        IP_DAD_STATE_DUPLICATE => DadState::Duplicate,
        IP_DAD_STATE_DEPRECATED => DadState::Deprecated,
        IP_DAD_STATE_PREFERRED => DadState::Preferred,
        value => DadState::Unknown(value),
    })
}

/// Reports whether one exact address exists without accepting other interface addresses.
///
/// # Errors
///
/// Returns validation or IP Helper failure other than the authoritative not-found status.
pub fn exact_address_present(
    interface_luid: u64,
    address: Ipv4Addr,
    prefix_length: u8,
) -> Result<bool, IpHelperError> {
    let mut row = address_row(interface_luid, address, prefix_length)?;
    let status = unsafe { GetUnicastIpAddressEntry(&raw mut row) };
    if status == ERROR_NOT_FOUND {
        Ok(false)
    } else {
        win_result(status).map(|()| true)
    }
}

fn route_row(route: RouteKey) -> MIB_IPFORWARD_ROW2 {
    let mut row = MIB_IPFORWARD_ROW2::default();
    unsafe { InitializeIpForwardEntry(&raw mut row) };
    row.InterfaceLuid = luid(route.interface_luid);
    row.DestinationPrefix.Prefix = sockaddr(route.prefix.network());
    row.DestinationPrefix.PrefixLength = route.prefix.prefix_len();
    row.NextHop = sockaddr(route.next_hop);
    row.SitePrefixLength = route.prefix.prefix_len();
    row.Metric = route.metric;
    row.Protocol = MIB_IPPROTO_NETMGMT;
    row.Origin = NlroManual;
    row
}

fn address_row(
    interface_luid: u64,
    address: Ipv4Addr,
    prefix_length: u8,
) -> Result<MIB_UNICASTIPADDRESS_ROW, IpHelperError> {
    if interface_luid == 0 || !(1..=30).contains(&prefix_length) || address.is_unspecified() {
        return Err(IpHelperError(87));
    }
    let mut row = MIB_UNICASTIPADDRESS_ROW::default();
    unsafe { InitializeUnicastIpAddressEntry(&raw mut row) };
    row.InterfaceLuid = luid(interface_luid);
    row.Address = sockaddr(address);
    row.OnLinkPrefixLength = prefix_length;
    Ok(row)
}

fn system_route(row: &MIB_IPFORWARD_ROW2) -> Result<SystemRoute, IpHelperError> {
    let family = unsafe { row.DestinationPrefix.Prefix.si_family };
    let next_family = unsafe { row.NextHop.si_family };
    if family != AF_INET || next_family != AF_INET {
        return Err(IpHelperError(13));
    }
    let address = ipv4(unsafe { row.DestinationPrefix.Prefix.Ipv4 });
    let prefix =
        Ipv4Net::new(address, row.DestinationPrefix.PrefixLength).map_err(|_| IpHelperError(13))?;
    if prefix.addr() != prefix.network() {
        return Err(IpHelperError(13));
    }
    let key = RouteKey {
        prefix,
        interface_luid: unsafe { row.InterfaceLuid.Value },
        next_hop: ipv4(unsafe { row.NextHop.Ipv4 }),
        metric: row.Metric,
    };
    let project_owned = key.next_hop == Ipv4Addr::UNSPECIFIED
        && row.SitePrefixLength == prefix.prefix_len()
        && key.metric == PROJECT_ROUTE_METRIC
        && row.Protocol == MIB_IPPROTO_NETMGMT
        && row.Origin == NlroManual;
    Ok(SystemRoute { key, project_owned })
}

fn luid(value: u64) -> NET_LUID_LH {
    NET_LUID_LH { Value: value }
}

fn sockaddr(address: Ipv4Addr) -> SOCKADDR_INET {
    let mut value = SOCKADDR_IN {
        sin_family: AF_INET,
        ..SOCKADDR_IN::default()
    };
    value.sin_addr.S_un.S_addr = u32::from_ne_bytes(address.octets());
    SOCKADDR_INET { Ipv4: value }
}

fn ipv4(value: SOCKADDR_IN) -> Ipv4Addr {
    Ipv4Addr::from(unsafe { value.sin_addr.S_un.S_addr }.to_ne_bytes())
}

fn win_result(status: u32) -> Result<(), IpHelperError> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(IpHelperError(status))
    }
}

/// Reads and strictly validates one private network manifest.
///
/// # Errors
///
/// Returns private-storage, size, JSON, schema, or ownership validation failure.
pub fn read_network_manifest(path: &Path) -> io::Result<crate::NetworkManifest> {
    let bytes = read_private(path, crate::MAX_MANIFEST_BYTES as u64)?;
    crate::NetworkManifest::decode(&bytes).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
}

/// Atomically writes one validated manifest through the protected storage boundary.
///
/// # Errors
///
/// Returns manifest validation, private-storage, ACL, or atomic replacement failure.
pub fn write_network_manifest_atomic(
    path: &Path,
    temporary: &Path,
    manifest: &crate::NetworkManifest,
) -> io::Result<()> {
    let bytes = manifest
        .encode()
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    write_private_atomic(path, temporary, &bytes)
}

/// Removes one exact validated private manifest.
///
/// # Errors
///
/// Returns path, ACL, reparse, object type, or removal failure.
pub fn remove_network_manifest(path: &Path) -> io::Result<()> {
    remove_private(path)
}
