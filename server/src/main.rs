use std::sync::Arc;
use std::path::Path;
use std::error::Error;
use std::io::{ErrorKind, Error as IoError};
use std::net::SocketAddr;
use std::result::Result;
use std::collections::HashMap;

use tokio::sync::Mutex;
use tokio::net::{TcpListener};
use tokio_rustls::{TlsAcceptor};
use tokio_rustls::rustls::ServerConfig;

use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

use certs::{load_keys, load_certs};

type DataStream = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;

async fn handle_connection(peer_addr: SocketAddr, 
                           tls_stream: DataStream,
                           clients_map: Arc<Mutex<HashMap<SocketAddr, DataStream>>>) 
{
    tracing::info!("New connection: {}", peer_addr);

    let mut clients_mutex = clients_map.lock().await; 
    clients_mutex.insert(peer_addr, tls_stream);

    tracing::info!("Clients number: {:?}", clients_mutex.len() + 1);

    const BUFF_SIZE: usize = 512;
    let mut buf = vec![0; BUFF_SIZE];

    let tls_stream = clients_mutex.get_mut(&peer_addr).unwrap();

    while let Ok(n) = tls_stream.read(&mut buf).await
    {
        if n == 0 { 
            tracing::error!("No message from: {}, disconnect!", peer_addr); 
            return;
        }

        let text_msg = String::from_utf8_lossy(&buf[..n]).to_string();
        tracing::info!("recv bytes: {} {:?} from peer: {:?}", n, text_msg, peer_addr);
        
        // Send to all clients
         for (peer_addr, stream) in clients_mutex.iter_mut() 
         {
             stream.write_all(format!("New client: {:?}", peer_addr.clone()).as_str().as_bytes()).await.expect("failed to write data to socket");
             tracing::info!("send msg to peer: {:?}", peer_addr);
         }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> 
{
    tracing_subscriber::fmt().init();

    let certs = load_certs(&Path::new("../keys/eddsa-ca.cert"))?;
    let keys = load_keys(&Path::new("../keys/Ed25519_private_key.pem"))?;
    
    let private_key = keys.get(0).unwrap().clone();

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .map_err(|err| IoError::new(ErrorKind::InvalidInput, err))?;

    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("0.0.0.0:9900").await?;
    tracing::info!("listening on port {:?}", listener.local_addr()?);

    let clients_map  = Arc::new(Mutex::new(HashMap::<SocketAddr, _>::new()));

    loop {

        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let mut tls_stream = acceptor.accept(stream).await?;
        
        let clients_map_cloned = clients_map.clone();

        tokio::spawn(async move {
            handle_connection(peer_addr, tls_stream, clients_map_cloned).await;
        });
    }
}