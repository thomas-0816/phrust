use crate::server::ServerError;
use rustls_pki_types::pem::PemObject;
use std::{path::Path, sync::Arc};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        ServerConfig as RustlsServerConfig,
        pki_types::{CertificateDer, PrivateKeyDer},
    },
};

pub(crate) fn build_tls_acceptor(
    cert_path: Option<&Path>,
    key_path: Option<&Path>,
) -> Result<Option<TlsAcceptor>, ServerError> {
    let (Some(cert_path), Some(key_path)) = (cert_path, key_path) else {
        return Ok(None);
    };
    let certs = load_tls_certs(cert_path)?;
    let key = load_tls_private_key(key_path)?;
    let mut config = RustlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| {
            ServerError::Tls(format!(
                "TLS certificate/key configuration is invalid: {error}"
            ))
        })?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Some(TlsAcceptor::from(Arc::new(config))))
}

pub(crate) fn load_tls_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, ServerError> {
    let certs = CertificateDer::pem_file_iter(path)
        .map_err(|error| {
            ServerError::Tls(format!(
                "TLS certificate `{}` cannot be parsed: {error}",
                path.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            ServerError::Tls(format!(
                "TLS certificate `{}` cannot be parsed: {error}",
                path.display()
            ))
        })?;
    if certs.is_empty() {
        return Err(ServerError::Tls(format!(
            "TLS certificate `{}` does not contain any certificates",
            path.display()
        )));
    }
    Ok(certs)
}

pub(crate) fn load_tls_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, ServerError> {
    PrivateKeyDer::from_pem_file(path).map_err(|error| {
        ServerError::Tls(format!(
            "TLS private key `{}` cannot be parsed: {error}",
            path.display()
        ))
    })
}
