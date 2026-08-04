#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use xs_agent::windows_xsnet::{ClientState, Win32DeviceTransport, XsnetDeviceSession};

    fn open_session() -> Option<XsnetDeviceSession<Win32DeviceTransport>> {
        match XsnetDeviceSession::open(1280, 4, 4) {
            Ok(session) => Some(session),
            Err(error) => {
                eprintln!("windows_xsnet_smoke open_error={error}");
                None
            }
        }
    }

    let Some(mut session) = open_session() else {
        return std::process::ExitCode::FAILURE;
    };
    let interface_luid = session.interface_luid();
    if interface_luid == 0 || session.state() != ClientState::LinkUp {
        eprintln!("windows_xsnet_smoke invalid_started_session");
        return std::process::ExitCode::FAILURE;
    }

    // Minimal valid IPv4 packet. The driver intentionally validates only the
    // version, IHL, total length, and negotiated MTU at this boundary.
    let packet = [
        0x45, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x40, 0xfd, 0x00, 0x00, 100, 88, 0, 2, 100,
        88, 0, 1,
    ];
    if let Err(error) = session.enqueue_receive(&[&packet]) {
        eprintln!("windows_xsnet_smoke receive_error={error}");
        return std::process::ExitCode::FAILURE;
    }
    if let Err(error) = session.shutdown() {
        eprintln!("windows_xsnet_smoke shutdown_error={error}");
        return std::process::ExitCode::FAILURE;
    }

    // Opening a new exclusive owner proves that detach and handle cleanup did
    // not leave the driver session orphaned.
    let Some(mut reopened) = open_session() else {
        return std::process::ExitCode::FAILURE;
    };
    if reopened.interface_luid() != interface_luid {
        eprintln!("windows_xsnet_smoke interface_luid_changed");
        return std::process::ExitCode::FAILURE;
    }
    if let Err(error) = reopened.shutdown() {
        eprintln!("windows_xsnet_smoke reopen_shutdown_error={error}");
        return std::process::ExitCode::FAILURE;
    }

    println!("windows_xsnet_smoke passed interface_luid={interface_luid}");
    std::process::ExitCode::SUCCESS
}

#[cfg(not(windows))]
fn main() {
    eprintln!("windows_xsnet_smoke requires Windows");
}
