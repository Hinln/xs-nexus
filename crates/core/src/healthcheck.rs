use std::{
    fmt,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

const MAX_RESPONSE_BYTES: usize = 8 * 1024;

#[derive(Debug)]
pub enum HttpHealthcheckError {
    Io(io::Error),
    InvalidResponse(&'static str),
}

impl fmt::Display for HttpHealthcheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "healthcheck I/O failed: {error}"),
            Self::InvalidResponse(reason) => {
                write!(formatter, "healthcheck response rejected: {reason}")
            }
        }
    }
}

impl std::error::Error for HttpHealthcheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidResponse(_) => None,
        }
    }
}

impl From<io::Error> for HttpHealthcheckError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Performs one bounded HTTP readiness probe against a loopback listener.
///
/// # Errors
///
/// Returns an error when the target or path violates the fixed local-probe
/// policy, the connection or bounded I/O fails, or the response is not a
/// complete HTTP/1.x `200 OK` header block within the size limit.
pub fn check_local_http_health(
    address: SocketAddr,
    path: &'static str,
    timeout: Duration,
) -> Result<(), HttpHealthcheckError> {
    if !address.ip().is_loopback() {
        return Err(HttpHealthcheckError::InvalidResponse(
            "target is not loopback",
        ));
    }
    if !path.starts_with('/') || path.bytes().any(|byte| byte <= b' ' || byte >= 0x7f) {
        return Err(HttpHealthcheckError::InvalidResponse(
            "invalid request path",
        ));
    }

    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()?;

    let mut response = Vec::with_capacity(512);
    let mut chunk = [0_u8; 512];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        if response.len() + count > MAX_RESPONSE_BYTES {
            return Err(HttpHealthcheckError::InvalidResponse(
                "response is too large",
            ));
        }
        response.extend_from_slice(&chunk[..count]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(HttpHealthcheckError::InvalidResponse(
            "incomplete HTTP headers",
        ))?;
    let status_end = response[..header_end]
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or(HttpHealthcheckError::InvalidResponse("missing status line"))?;
    let status = &response[..status_end];
    if status != b"HTTP/1.1 200 OK" && status != b"HTTP/1.0 200 OK" {
        return Err(HttpHealthcheckError::InvalidResponse(
            "status is not exactly 200 OK",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread};

    fn serve_once(response: &'static [u8]) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("test address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept healthcheck");
            let mut request = [0_u8; 256];
            let count = stream.read(&mut request).expect("read request");
            assert!(request[..count].starts_with(b"GET /health/ready HTTP/1.1\r\n"));
            stream.write_all(response).expect("write response");
        });
        address
    }

    #[test]
    fn accepts_exact_http_200() {
        let address = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        check_local_http_health(address, "/health/ready", Duration::from_secs(1))
            .expect("healthy response");
    }

    #[test]
    fn rejects_non_200_and_malformed_responses() {
        for response in [
            b"HTTP/1.1 503 Service Unavailable\r\n\r\n".as_slice(),
            b"HTTP/1.1 200 OK\n\n".as_slice(),
            b"garbage\r\n\r\n".as_slice(),
        ] {
            let address = serve_once(response);
            assert!(
                check_local_http_health(address, "/health/ready", Duration::from_secs(1)).is_err()
            );
        }
    }

    #[test]
    fn rejects_oversized_headers() {
        let response = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nX-Fill: {}\r\n\r\n",
                "x".repeat(MAX_RESPONSE_BYTES)
            )
            .into_bytes()
            .into_boxed_slice(),
        );
        let address = serve_once(response);
        assert!(check_local_http_health(address, "/health/ready", Duration::from_secs(1)).is_err());
    }

    #[test]
    fn rejects_non_loopback_and_invalid_paths() {
        assert!(
            check_local_http_health(
                "192.0.2.1:80".parse().expect("address"),
                "/health/ready",
                Duration::from_millis(10),
            )
            .is_err()
        );
        assert!(
            check_local_http_health(
                "127.0.0.1:1".parse().expect("address"),
                "/bad path",
                Duration::from_millis(10),
            )
            .is_err()
        );
    }

    #[test]
    fn reports_connection_refusal() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind unused port");
        let address = listener.local_addr().expect("unused address");
        drop(listener);
        assert!(
            check_local_http_health(address, "/health/ready", Duration::from_millis(100)).is_err()
        );
    }

    #[test]
    fn reports_read_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind timeout listener");
        let address = listener.local_addr().expect("timeout address");
        thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept timeout healthcheck");
            thread::sleep(Duration::from_millis(200));
        });
        assert!(
            check_local_http_health(address, "/health/ready", Duration::from_millis(25)).is_err()
        );
    }
}
