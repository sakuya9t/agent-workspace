//! TLS for the relay hop.
//!
//! Both ends of that hop are our own code, so both configs live here: the
//! **client** side the node agent dials `wss://` with, and the **server** side
//! the relay binary listens with.
//!
//! The relay hop carries the daemon device token and the full terminal stream,
//! so it must be encrypted whenever it leaves the host. Note the asymmetry that
//! makes this a code change rather than an ops change: the daemon dials the
//! relay *outbound* from behind NAT, so the daemon is itself a TLS **client**.
//! Terminating TLS at a reverse proxy does not help unless the agent can speak
//! `wss://` — which is what [`client_config`] exists for.
//!
//! The daemon stays loopback-bound and needs no certificate of its own; only
//! the relay presents one.

use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, RootCertStore, ServerConfig};

/// Install the process-wide rustls crypto provider exactly once.
///
/// rustls 0.23 refuses to build any config without one. It can infer a provider
/// from crate features, but only when exactly one is enabled anywhere in the
/// dependency graph — a property another crate can silently break. Installing
/// `ring` explicitly makes that independent of the rest of the graph.
fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Err means someone already installed a provider, which is equally fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// The TLS config the node agent dials the relay with.
///
/// Trusts the public web PKI, so a relay holding an ordinary ACME certificate
/// works with no client-side configuration and no browser warning — the
/// recommended deployment. `extra_ca` adds PEM trust anchors on top, for a
/// self-hosted relay behind a private CA or a self-signed certificate; without
/// it such a relay is unreachable, since the agent has no way to be told
/// "trust this one".
pub fn client_config(extra_ca: Option<&[u8]>) -> Result<Arc<ClientConfig>> {
    install_crypto_provider();

    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if let Some(pem) = extra_ca {
        let certs = read_certs(pem).context("parsing the relay CA bundle")?;
        if certs.is_empty() {
            bail!("the relay CA bundle contains no certificates");
        }
        for cert in certs {
            roots
                .add(cert)
                .context("adding a relay CA certificate to the trust store")?;
        }
    }

    Ok(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

/// The relay listener's TLS config, from a PEM certificate chain and key file.
pub fn server_config(cert_path: &Path, key_path: &Path) -> Result<ServerConfig> {
    let cert_pem = std::fs::read(cert_path)
        .with_context(|| format!("reading TLS certificate {}", cert_path.display()))?;
    let key_pem = std::fs::read(key_path)
        .with_context(|| format!("reading TLS key {}", key_path.display()))?;
    server_config_pem(&cert_pem, &key_pem).with_context(|| {
        format!(
            "building the relay TLS config from {} and {}",
            cert_path.display(),
            key_path.display()
        )
    })
}

/// The relay listener's TLS config, from PEM bytes already in hand.
///
/// ALPN advertises **http/1.1 only, deliberately**. A browser offered `h2` will
/// take it, and HTTP/2 has no plain WebSocket upgrade — every terminal stream
/// through the relay would fail. Pinning h1 keeps the upgrade path intact.
pub fn server_config_pem(cert_pem: &[u8], key_pem: &[u8]) -> Result<ServerConfig> {
    install_crypto_provider();

    let certs = read_certs(cert_pem).context("parsing the TLS certificate")?;
    if certs.is_empty() {
        bail!("no certificates found in the TLS certificate PEM");
    }
    let key = rustls_pemfile::private_key(&mut BufReader::new(key_pem))
        .context("parsing the TLS key")?
        .ok_or_else(|| anyhow::anyhow!("no private key found in the TLS key PEM"))?;

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("the TLS key does not match the certificate")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

fn read_certs(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    rustls_pemfile::certs(&mut BufReader::new(pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}
