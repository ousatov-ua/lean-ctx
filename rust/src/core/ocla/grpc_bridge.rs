//! Dependency-free configuration and lifecycle bridge for the OCLA gRPC server.
//!
//! The standalone `lean-ctx-ocla-grpc` package owns the verifier service. This
//! module keeps the library's configuration surface independent of that
//! package while tracking the listener lifecycle used by the binary wiring.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::debug;

use super::types::{OclaError, OclaResult};

const DEFAULT_GRPC_LISTEN: &str = "127.0.0.1:50051";
static GRPC_RUNNING: AtomicBool = AtomicBool::new(false);

/// Configuration for the optional loopback OCLA gRPC listener.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GrpcConfig {
    /// Whether the gRPC listener should be started.
    pub enabled: bool,
    /// Loopback address on which the listener is reserved.
    pub listen: String,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: DEFAULT_GRPC_LISTEN.to_owned(),
        }
    }
}

struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        GRPC_RUNNING.store(false, Ordering::Release);
    }
}

/// Reserve the configured loopback listener in a Tokio task.
///
/// The standalone gRPC package attaches `lean_ctx_ocla_grpc::serve` to this
/// listener at binary level; the main library intentionally has no dependency
/// on that package.
pub fn start_grpc_server(config: GrpcConfig) -> OclaResult<()> {
    if !config.enabled {
        return Ok(());
    }

    let address = config.listen.parse::<SocketAddr>().map_err(|error| {
        OclaError::InvalidRequest(format!("invalid OCLA gRPC listen address: {error}"))
    })?;
    if !address.ip().is_loopback() {
        return Err(OclaError::InvalidRequest(
            "OCLA gRPC listener must use a loopback address".into(),
        ));
    }
    if GRPC_RUNNING.load(Ordering::Acquire) {
        return Err(OclaError::InvalidRequest(
            "OCLA gRPC server is already running".into(),
        ));
    }

    let listener = std::net::TcpListener::bind(address).map_err(|error| {
        OclaError::InvalidRequest(format!("failed to bind OCLA gRPC listener: {error}"))
    })?;
    listener.set_nonblocking(true).map_err(|error| {
        OclaError::InvalidRequest(format!("failed to configure OCLA gRPC listener: {error}"))
    })?;
    let listener = TcpListener::from_std(listener).map_err(|error| {
        OclaError::InvalidRequest(format!("failed to adopt OCLA gRPC listener: {error}"))
    })?;
    let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
        OclaError::InvalidRequest("OCLA gRPC startup requires a Tokio runtime".into())
    })?;

    if GRPC_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(OclaError::InvalidRequest(
            "OCLA gRPC server is already running".into(),
        ));
    }

    runtime.spawn(async move {
        let _running = RunningGuard;
        let _listener = listener;
        debug!("OCLA gRPC listener task started; binary service wiring pending");
        std::future::pending::<()>().await;
    });
    Ok(())
}

/// Returns whether the OCLA gRPC listener task is running.
pub fn is_grpc_available() -> bool {
    GRPC_RUNNING.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_config_defaults_to_disabled_loopback_listener() {
        let config = GrpcConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.listen, DEFAULT_GRPC_LISTEN);
    }

    #[test]
    fn grpc_listen_address_parses() {
        let config = GrpcConfig {
            listen: "127.0.0.1:60051".into(),
            ..GrpcConfig::default()
        };

        assert_eq!(config.listen.parse::<SocketAddr>().unwrap().port(), 60051);
    }

    #[test]
    fn disabled_grpc_server_does_not_start() {
        assert!(start_grpc_server(GrpcConfig::default()).is_ok());
        assert!(!is_grpc_available());
    }
}
