use std::fs::File;
use std::net::{SocketAddr};
use std::path::{Path};
use std::io::{BufReader, ErrorKind, Error as IoError, Result as IoResult};
use std::error::Error;
use std::result::Result;
use std::sync::Arc;
use std::collections::HashMap;

use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio_rustls::{TlsAcceptor, TlsStream};
use tokio_rustls::rustls::{self, Certificate, PrivateKey};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

fn load_certs(path: &Path) -> IoResult<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| IoError::new(ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).map(Certificate).collect())
}

fn load_keys(path: &Path) -> IoResult<Vec<PrivateKey>> {
    pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| IoError::new(ErrorKind::InvalidInput, "invalid key"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> 
{
    tracing_subscriber::fmt().init();

    let certs = load_certs(&Path::new("../keys/eddsa-ca.cert"))?;
    let keys = load_keys(&Path::new("../keys/Ed25519_private_key.pem"))?;
    
    let private_key = keys.get(0).unwrap().clone();

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .map_err(|err| IoError::new(ErrorKind::InvalidInput, err))?;

    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("0.0.0.0:9900").await?;
    tracing::info!("listening on port {:?}", listener.local_addr()?);

    let clients_map = Arc::new(Mutex::new(HashMap::<SocketAddr, TlsStream<_>>::new()));

    const BUFF_SIZE: usize = 512;

    loop {

        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let mut tls_stream = acceptor.accept(stream).await?;

        let clients_map_cloned = clients_map.clone();

        tokio::spawn(async move {
            tracing::info!("connected: ('{}', {})", peer_addr.ip(), peer_addr.port());
            let mut buf = vec![0; BUFF_SIZE];
            
            let n = tls_stream
                        .read(&mut buf)
                        .await
                        .expect("failed to read data from socket");
                
            let text_msg = String::from_utf8_lossy(&buf[..n]).to_string();
            tracing::info!("recv bytes: {} {:?} from peer: {:?}", n, text_msg, peer_addr);
                            
            {
                let mut clients_mutex = clients_map_cloned.lock().await;
                tracing::info!("Clients number: {:?}", clients_mutex.len() + 1);

                // Send to all clients
                for (peer_addr, stream) in clients_mutex.iter_mut() 
                {
                    stream.write_all(format!("New client: {:?}", peer_addr.clone()).as_str().as_bytes()).await.expect("failed to write data to socket");
                    tracing::info!("send msg to peer: {:?}", peer_addr);
                }

                clients_mutex.insert(peer_addr, tokio_rustls::TlsStream::Server(tls_stream));
            }

        });
    }
}