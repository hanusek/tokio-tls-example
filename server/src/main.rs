use std::sync::Arc;
use std::path::Path;
use std::error::Error;
use std::io::{ErrorKind, Error as IoError};
use std::net::SocketAddr;
use std::result::Result;
use std::collections::HashMap;

use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsStream};
use tokio_rustls::rustls::ServerConfig;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use futures_util::StreamExt;
use futures_util::SinkExt;

use certs::{load_keys, load_certs};

async fn handle_ws_connection(peer_addr: SocketAddr, 
                              tls_stream: TlsStream<TcpStream>,
                              clients_map: Arc<Mutex<HashMap<SocketAddr, WebSocketStream<TlsStream<TcpStream>>>>>)
{
    let mut ws_stream = accept_async(tls_stream).await.expect("Failed to accept");

    tracing::info!("New WebSocket connection: {}", peer_addr);

    if let Some(msg) = ws_stream.next().await
    {
        tracing::info!("recv: {:?} from peer: {:?}", msg.unwrap(), peer_addr)
    }

    {
        let mut clients_mutex = clients_map.lock().await;
        tracing::info!("Clients number: {:?}", clients_mutex.len() + 1);

        // Send to all clients
        for (peer_addr, stream) in clients_mutex.iter_mut() 
        {
            let msg = Message::Text(format!("New client: {:?}", peer_addr.clone()));
            match stream.send(msg.into()).await
            {
                Ok(_) => { tracing::info!("send msg to peer: {:?}", peer_addr); },
                Err(e) =>  { tracing::info!("send error: {:?} to peer: {:?}", e, peer_addr); }
            };
        }

        clients_mutex.insert(peer_addr, ws_stream);
    }

}

// async fn handle_connection(peer_addr: SocketAddr, 
//                            mut tls_stream: TlsStream<TcpStream>, 
//                            clients_map: Arc<Mutex<HashMap<SocketAddr, TlsStream<TcpStream>>>>) 
// {
//     tracing::info!("New connection: {}", peer_addr);
//     
//     const BUFF_SIZE: usize = 512;
//     let mut buf = vec![0; BUFF_SIZE];

//     let n = tls_stream
//     .read(&mut buf)
//     .await
//     .expect("failed to read data from socket");

//     let text_msg = String::from_utf8_lossy(&buf[..n]).to_string();
//     tracing::info!("recv bytes: {} {:?} from peer: {:?}", n, text_msg, peer_addr);
            
//     {
//         let mut clients_mutex = clients_map.lock().await;
//         tracing::info!("Clients number: {:?}", clients_mutex.len() + 1);

//         // Send to all clients
//         for (peer_addr, stream) in clients_mutex.iter_mut() 
//         {
//         stream.write_all(format!("New client: {:?}", peer_addr.clone()).as_str().as_bytes()).await.expect("failed to write data to socket");
//         tracing::info!("send msg to peer: {:?}", peer_addr);
//         }

//         clients_mutex.insert(peer_addr, tls_stream);
//     }
// }

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
        let tls_stream = acceptor.accept(stream).await?;

        let clients_map_cloned = clients_map.clone();

        tokio::spawn(async move {
            handle_ws_connection(peer_addr, tokio_rustls::TlsStream::Server(tls_stream), clients_map_cloned).await;
        });
    }
}