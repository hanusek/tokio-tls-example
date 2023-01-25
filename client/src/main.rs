use std::io::{ErrorKind, Error as IoError};
use std::error::Error;
use std::result::Result;
use std::sync::Arc;
use std::time::SystemTime;
use std::convert::TryFrom;
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector};
use tokio_rustls::rustls::{Certificate, ClientConfig, ServerName, RootCertStore};
use tokio_rustls::rustls::client::{ServerCertVerified, ServerCertVerifier};
use tokio_rustls::rustls::Error as RustlsError;
use tokio::io::AsyncWriteExt;
use tokio::io::AsyncReadExt;

struct DummyVerifier { }

impl DummyVerifier {
    fn new() -> Self {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> 
{
    tracing_subscriber::fmt().init();

    let root_cert_store = RootCertStore::empty();

    let mut config = ClientConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    let dummy_verifier = Arc::new(DummyVerifier::new());
    config.dangerous().set_certificate_verifier(dummy_verifier);

    const HOST: &str = "127.0.0.1";
    const PORT: &str = "9900";
    let addr = format!("{}:{}", HOST, PORT);
    
    let connector = TlsConnector::from(Arc::new(config));
        
    let domain = ServerName::try_from("xxx")
        .map_err(|_| IoError::new(ErrorKind::InvalidInput, "invalid dnsname"))?;

    let stream = TcpStream::connect(&addr).await?;
    let mut tls_stream = connector.connect(domain, stream).await?;
    
    const BUFF_SIZE: usize = 512;

    tracing::info!("connected: '{:?}", tls_stream);
    let mut buf = vec![0; BUFF_SIZE];

    loop {
        tls_stream.write_all(b"HELLO").await.expect("failed to write data to socket");

        match tls_stream.read(&mut buf).await
        {
            Err(_e) => {},
            Ok(n) => {
                if n != 0 { 
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    tracing::info!("recv text: {} {:?} from server", n, text);
                }
            }
        };
    }//loop
    
}
