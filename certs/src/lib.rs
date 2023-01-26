use std::fs::File;
use std::path::{Path};
use std::io::{BufReader, ErrorKind, Error as IoError, Result as IoResult};
use std::result::Result;
use std::time::SystemTime;
use std::sync::Arc;

use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio_rustls::rustls::{Certificate, ClientConfig, ServerName, PrivateKey, RootCertStore};
use tokio_rustls::rustls::client::{ServerCertVerified, ServerCertVerifier};
use tokio_rustls::rustls::Error as RustlsError;

pub struct DummyVerifier { }

impl DummyVerifier {
    pub fn new() -> Self {
        DummyVerifier { }
    }
}

impl ServerCertVerifier for DummyVerifier {
    fn verify_server_cert(
        &self, 
        _end_entity: &Certificate, 
        _intermediates: &[Certificate], 
        _server_name: &ServerName, 
        _scts: &mut dyn Iterator<Item = &[u8]>, 
        _ocsp_response: &[u8], 
        _now: SystemTime
    ) -> Result<ServerCertVerified, RustlsError>
    {
        return Ok(ServerCertVerified::assertion()); 
    }
}

pub fn create_client_config() -> ClientConfig
{
    let mut config = ClientConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();

    config.dangerous().set_certificate_verifier(Arc::new(DummyVerifier::new()));
    return config;
}

pub fn load_certs(path: &Path) -> IoResult<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| IoError::new(ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).map(Certificate).collect())
}

pub fn load_keys(path: &Path) -> IoResult<Vec<PrivateKey>> {
    pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| IoError::new(ErrorKind::InvalidInput, "invalid key"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}
