//! Controlled Windows VM smoke test for the pinned Wintun session boundary.
//!
//! Run with the absolute path to the official, hash-pinned x64 `wintun.dll`. The adapter is
//! deliberately short-lived: creation, LUID/index discovery, session initialization, and Drop
//! cleanup are all exercised without configuring addresses, routes, or a default route.

use std::{env, error::Error, path::PathBuf};

use xs_windows_wintun::{DEFAULT_RING_CAPACITY, WintunSession, parse_sha256};

const WINTUN_SHA256: &str = "e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce";

fn main() -> Result<(), Box<dyn Error>> {
    let library_path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: session_smoke <absolute-wintun-dll-path>")?;
    let expected_sha256 = parse_sha256(WINTUN_SHA256)?;
    let session = WintunSession::create(
        &library_path,
        &expected_sha256,
        "xsn-wintun-smoke",
        "XS Nexus VM smoke",
        DEFAULT_RING_CAPACITY,
    )?;
    if session.interface_luid() == 0 || session.interface_index() == 0 {
        return Err("Wintun returned an unusable interface identity".into());
    }
    println!(
        "created Wintun smoke session: luid={} interface_index={}",
        session.interface_luid(),
        session.interface_index()
    );
    drop(session);
    Ok(())
}
