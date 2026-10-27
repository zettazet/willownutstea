use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: String,
    pub udp_listen: String,
    pub database: String,
    pub tls_cert: String,
    pub tls_key: String,
}
pub enum Action {
    Run(String),
    Check(String),
    Help,
    Version,
}
pub fn arguments() -> Result<Action> {
    let mut args = std::env::args().skip(1);
    let action = match args.next().as_deref() {
        None => Action::Run("server.json".into()),
        Some("--help" | "-h") => Action::Help,
        Some("--version") => Action::Version,
        Some("--check-config") => {
            Action::Check(args.next().context("--check-config requires a JSON file")?)
        }
        Some(s) => {
            ensure!(!s.starts_with('-'), "Unknown option: {s}. Use --help");
            Action::Run(s.into())
        }
    };
    ensure!(args.next().is_none(), "Unexpected argument. Use --help");
    Ok(action)
}
impl Config {
    /// Validate local configuration/key material without opening sockets or SQLite.
    /// Does not establish certificate trust, expiry, DNS correctness or reachability.
    pub fn load(path: &str) -> Result<(Self, tokio_rustls::rustls::ServerConfig)> {
        let data = std::fs::read(path)
            .with_context(|| format!("Cannot read server configuration {path}"))?;
        let config: Self =
            serde_json::from_slice(&data).context("Invalid JSON server configuration")?;
        config
            .listen
            .parse::<SocketAddr>()
            .context("listen must be an IP:port address")?;
        config
            .udp_listen
            .parse::<SocketAddr>()
            .context("udp_listen must be an IP:port address")?;
        ensure!(
            !config.database.is_empty(),
            "database path must not be empty"
        );
        let file = std::fs::File::open(&config.tls_cert)
            .context("Cannot open TLS certificate; check tls_cert and service permissions")?;
        let certificates = rustls_pemfile::certs(&mut std::io::BufReader::new(file))
            .collect::<std::io::Result<Vec<_>>>()
            .context("Cannot parse TLS certificate PEM")?;
        ensure!(!certificates.is_empty(), "TLS certificate chain is empty");
        let file = std::fs::File::open(&config.tls_key)
            .context("Cannot open TLS private key; check tls_key and service permissions")?;
        let key = rustls_pemfile::private_key(&mut std::io::BufReader::new(file))?
            .context("No TLS private key in PEM file")?;
        let tls = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .context("TLS certificate/key pair is invalid or mismatched")?;
        Ok((config, tls))
    }
}
