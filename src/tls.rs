/*
 * Copyright (C) 2024 Aspect
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    fmt::Write as _,
    net::{IpAddr, SocketAddr},
    path::Path,
};

use anyhow::{bail, Context, Result};
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use base64::prelude::*;
use log::{info, warn};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use rcgen::{CertificateParams, KeyPair};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const AUTO_CERT_FILENAME: &str = "cert.pem";
pub const AUTO_KEY_FILENAME: &str = "key.pem";

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TlsMode {
    #[default]
    Auto,
    Off,
    Custom,
}

impl TlsMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TlsMode::Auto => "auto",
            TlsMode::Off => "off",
            TlsMode::Custom => "custom",
        }
    }
}

fn is_bad_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
        }
    }
}

/// Collect subject alternative names for an auto-generated certificate:
/// the loopback names plus every non-loopback, non-link-local local address.
fn collect_sans() -> Vec<String> {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "tenebra".into());

    let mut sans: Vec<String> = vec![
        "localhost".into(),
        "127.0.0.1".into(),
        "::1".into(),
        hostname,
    ];

    if let Ok(interfaces) = NetworkInterface::show() {
        for iface in interfaces {
            for addr in iface.addr {
                let ip = addr.ip();
                if !is_bad_ip(&ip) {
                    sans.push(ip.to_string());
                }
            }
        }
    }

    sans.sort();
    sans.dedup();
    sans
}

/// Ensure a self-signed certificate/key pair exists in `state_dir`,
/// generating both files if either one is missing.
fn ensure_auto_cert(state_dir: &Path, cert_path: &Path, key_path: &Path) -> Result<()> {
    if cert_path.exists() && key_path.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(state_dir)
        .with_context(|| format!("Failed to create state directory {}", state_dir.display()))?;

    let key_pair = KeyPair::generate().context("Failed to generate self-signed key pair")?;
    let sans = collect_sans();
    let params = CertificateParams::new(sans).context("Failed to build certificate parameters")?;
    let cert = params
        .self_signed(&key_pair)
        .context("Failed to generate self-signed certificate")?;

    std::fs::write(cert_path, cert.pem())
        .with_context(|| format!("Failed to write certificate file {}", cert_path.display()))?;
    std::fs::write(key_path, key_pair.serialize_pem())
        .with_context(|| format!("Failed to write private key file {}", key_path.display()))?;

    info!(
        "Generated self-signed TLS certificate at {} (key {})",
        cert_path.display(),
        key_path.display()
    );

    Ok(())
}

/// Decode the first PEM certificate in `pem` to DER and return its
/// SHA-256 fingerprint as uppercase, colon-separated hex pairs.
fn fingerprint_from_pem(pem: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(pem).context("Certificate file is not valid UTF-8 PEM")?;

    let mut b64 = String::new();
    let mut in_cert = false;
    for line in text.lines() {
        let line = line.trim();
        if line == "-----BEGIN CERTIFICATE-----" {
            in_cert = true;
            continue;
        }
        if line == "-----END CERTIFICATE-----" {
            in_cert = false;
            continue;
        }
        if in_cert {
            b64.push_str(line);
        }
    }

    if b64.is_empty() {
        bail!("Could not find a PEM certificate in the given file");
    }

    let der = BASE64_STANDARD
        .decode(b64.as_bytes())
        .context("Failed to base64-decode certificate PEM")?;

    let digest = Sha256::digest(&der);
    let mut out = String::with_capacity(digest.len() * 3);
    for (i, byte) in digest.iter().enumerate() {
        if i > 0 {
            out.push(':');
        }
        let _ = write!(out, "{:02X}", byte);
    }

    Ok(out)
}

/// Serve `app` on `addr` according to `mode`. This is a long-running task:
/// it only returns once the server stops or fails fatally.
pub async fn serve(
    app: Router,
    addr: SocketAddr,
    mode: TlsMode,
    cert: Option<&Path>,
    key: Option<&Path>,
    state_dir: &Path,
) -> Result<()> {
    match mode {
        TlsMode::Off => {
            warn!("TLS is disabled; serving plain HTTP on {}", addr);
            axum_server::bind(addr)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .map_err(Into::into)
        }
        TlsMode::Custom => {
            let cert = cert.context("TLS mode 'custom' requires a certificate path")?;
            let key = key.context("TLS mode 'custom' requires a private key path")?;

            let cert_pem = std::fs::read(cert)
                .with_context(|| format!("Failed to read certificate file {}", cert.display()))?;
            let key_pem = std::fs::read(key)
                .with_context(|| format!("Failed to read private key file {}", key.display()))?;

            let config = RustlsConfig::from_pem(cert_pem, key_pem)
                .await
                .context("Failed to build TLS configuration")?;

            info!(
                "Serving HTTPS on {} using certificate {} and key {}",
                addr,
                cert.display(),
                key.display()
            );

            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .map_err(Into::into)
        }
        TlsMode::Auto => {
            let cert_path = state_dir.join(AUTO_CERT_FILENAME);
            let key_path = state_dir.join(AUTO_KEY_FILENAME);

            ensure_auto_cert(state_dir, &cert_path, &key_path)?;

            let cert_pem = std::fs::read(&cert_path).with_context(|| {
                format!("Failed to read certificate file {}", cert_path.display())
            })?;
            let key_pem = std::fs::read(&key_path).with_context(|| {
                format!("Failed to read private key file {}", key_path.display())
            })?;

            let fingerprint = fingerprint_from_pem(&cert_pem)?;
            println!("TLS certificate fingerprint (SHA-256): {}", fingerprint);
            info!("TLS certificate fingerprint (SHA-256): {}", fingerprint);

            let config = RustlsConfig::from_pem(cert_pem, key_pem)
                .await
                .context("Failed to build TLS configuration")?;

            info!(
                "Serving HTTPS on {} using certificate {} and key {}",
                addr,
                cert_path.display(),
                key_path.display()
            );

            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .map_err(Into::into)
        }
    }
}
