use std::io::{ErrorKind, Error as IoError};
use std::sync::Arc;
use std::error::Error;
use std::result::Result;
use std::convert::TryFrom;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use tokio::io::AsyncReadExt;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ServerName;

use certs::create_client_config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> 
{
    tracing_subscriber::fmt().init();

    let config = create_client_config();

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
