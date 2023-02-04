use certs::{load_keys, load_certs};
use std::io::{ErrorKind, Error as IoError};
use std::net::SocketAddr;
use std::sync::Arc;
use std::path::Path;
use std::error::Error;
use std::collections::HashMap;

use bytes::Bytes;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::{Framed, BytesCodec};

use tokio_rustls::{TlsAcceptor};
use tokio_rustls::server::TlsStream;
use tokio_rustls::rustls::{self};

use futures::SinkExt;

use tokio_tungstenite::WebSocketStream;
//use tungstenite::protocol::Message;

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<Bytes>;

/// Shorthand for the receive half of the message channel.
type Rx = mpsc::UnboundedReceiver<Bytes>;

struct Shared {
    peers: HashMap<SocketAddr, Tx>,
}

/// The state for each connected client.
struct Peer {
    /// The TCP socket wrapped with the `Lines` codec, defined below.
    ///
    /// This handles sending and receiving data on the socket. When using
    /// `Lines`, we can work at the line level instead of having to manage the
    /// raw byte operations.
    lines: Framed<TlsStream<TcpStream>, BytesCodec>,

    /// Receive half of the message channel.
    ///
    /// This is used to receive messages from peers. When a message is received
    /// off of this `Rx`, it will be written to the socket.
    rx: Rx,
}

impl Peer {
    /// Create a new instance of `Peer`.
    async fn new(
        state: Arc<Mutex<Shared>>,
        lines: Framed<TlsStream<TcpStream>, BytesCodec>,
    ) -> std::io::Result<Self> {
        // Get the client socket address
        let addr = lines.get_ref().get_ref().0.peer_addr()?;

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded_channel();

        // Add an entry for this `Peer` in the shared state map.
        state.lock().await.peers.insert(addr, tx);

        Ok(Peer { lines, rx })
    }
}

struct PeerWs {
    ws: WebSocketStream<TlsStream<TcpStream>>,
    rx: Rx,
}

impl PeerWs {
    /// Create a new instance of `Peer`.
    async fn new(
        state: Arc<Mutex<Shared>>,
        ws: WebSocketStream<TlsStream<TcpStream>>,
    ) -> std::io::Result<Self> {
        // Get the client socket address
        let addr = ws.get_ref().get_ref().0.peer_addr()?;

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded_channel();

        // Add an entry for this `Peer` in the shared state map.
        state.lock().await.peers.insert(addr, tx);

        Ok(PeerWs { ws, rx })
    }
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }

    fn get_clients_number(&self) -> usize
    {
        self.peers.len()
    }

    async fn send_to_peer(&mut self, target: SocketAddr, bytes: &Bytes) 
    {
        for peer in self.peers.iter_mut() {
            if *peer.0 == target {
                tracing::warn!("send_to_peer to: {:?}, msg: {:?}", peer.0, bytes);
                let _ = peer.1.send(bytes.clone());
            }
        }
    }

    /// Send a `LineCodec` encoded message to every peer, except
    /// for the sender.
    async fn broadcast(&mut self, sender: SocketAddr, bytes: &Bytes) {
        for peer in self.peers.iter_mut() {
            if *peer.0 != sender {
                tracing::warn!("send brodcast to: {:?}, msg: {:?}", peer.0, bytes);
                let _ = peer.1.send(bytes.clone());
            }
        }
    }
}


async fn process_ws_stream(state: Arc<Mutex<Shared>>, mut peer: PeerWs, addr: SocketAddr) -> Result<(), Box<dyn Error>> 
{
    loop 
    {
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            Some(msg) = peer.rx.recv() => {
                //tracing::warn!("send to: {:?}, msg: {:?}", peer.ws, msg);
                let ws_msg = tungstenite::protocol::Message::Binary(msg.to_vec());
                peer.ws.send(ws_msg).await?;
            }
            result = peer.ws.next() => 
            {
                match result {
                    // A message was received from the current user, we should
                    // broadcast this message to the other users.
                    Some(Ok(msg)) => {
                        //tracing::warn!("send to: {:?}, Send msg: {:?}", peer.ws.get_ref().get_ref().0.peer_addr().unwrap(), msg);
                        //peer.ws.send(msg).await?;

                        tracing::warn!("msg: {:?}", msg);

                        match msg
                        {
                            tungstenite::protocol::Message::Binary(v) => {
                                {
                                    state.lock().await.broadcast(addr, &Bytes::from(v)).await;
                                }
                            },
                            _ => {}
                        };
                    }
                    // An error occurred.
                    Some(Err(e)) => {
                        tracing::error!("error = {:?}", e);
                    }
                    // The stream has been exhausted.
                    None => { 
                        {
                            let mut state = state.lock().await;
                            state.peers.remove(&addr);
                    
                            let msg = format!("{} has left", addr);
                            tracing::info!("{}", msg);
                            state.broadcast(addr, &Bytes::from(msg)).await;
                        }
                        break
                    },
                };
            }
        }
    }
    return Ok(());
}

async fn process_tls_stream(state: Arc<Mutex<Shared>>, mut peer: Peer, addr: SocketAddr) -> Result<(), Box<dyn Error>> 
{
    loop 
    {
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            Some(msg) = peer.rx.recv() => {
                //tracing::warn!("send to: {:?}, msg: {:?}", peer.lines, msg);
                peer.lines.send(msg).await?; //NOTE: send to peer
            }
            result = peer.lines.next() => 
            {
                match result {
                    // A message was received from the current user, we should
                    // broadcast this message to the other users.
                    Some(Ok(msg)) => {
                        {
                            state.lock().await.broadcast(addr, &Bytes::from(msg)).await;
                        }
                        
                        //tracing::warn!("send to: {:?}, msg: {:?}", peer.lines, msg);
                        //peer.lines.send(msg).await?;
                    }
                    // An error occurred.
                    Some(Err(e)) => {
                        tracing::error!("error = {:?}", e);
                    }
                    // The stream has been exhausted.
                    None => { 
                        {
                            let mut state = state.lock().await;
                            state.peers.remove(&addr);
                    
                            let msg = format!("{} has left", addr);
                            tracing::info!("{}", msg);
                            state.broadcast(addr, &Bytes::from(msg)).await;
                        }                        
                        break 
                    },
                };
            }
        }
    }
    return Ok(());
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> 
{
    tracing_subscriber::fmt().init();

    let certs = load_certs(&Path::new("../keys/eddsa-ca.cert"))?;
    let keys = load_keys(&Path::new("../keys/Ed25519_private_key.pem"))?;

    let private_key = keys.get(0).unwrap().clone();
    let _cert = certs.get(0).unwrap().clone();

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .map_err(|err| IoError::new(ErrorKind::InvalidInput, err))?;

    let cfg = Arc::new(config);

    let plain_listener = TcpListener::bind("0.0.0.0:9900").await?;
    let websocket_listener = TcpListener::bind("0.0.0.0:9999").await?;

    let state = Arc::new(Mutex::new(Shared::new()));

    loop {

        tokio::select! {
            res1 = plain_listener.accept() =>  {
                let (stream, addr) = res1.unwrap();
                let acceptor = TlsAcceptor::from(cfg.clone());
                let tls_stream = acceptor.accept(stream).await.unwrap();
                let peer = Peer::new(state.clone(), Framed::new(tls_stream, BytesCodec::new())).await?;

                let state = Arc::clone(&state);

                {
                    let mut state = state.lock().await;
                    state.broadcast(addr, &Bytes::from("new tls client")).await;
                    tracing::debug!("clients: {}", state.get_clients_number());
                }

                tokio::spawn(async move {
                    tracing::debug!("accepted connection");
                    if let Err(e) = process_tls_stream(state, peer, addr).await {
                        tracing::info!("an error occurred; error = {:?}", e);
                    }
                });
            }
            res2 = websocket_listener.accept() =>  { 
                let (stream, addr) = res2.unwrap();
                let acceptor = TlsAcceptor::from(cfg.clone());
                let tls_stream = acceptor.accept(stream).await.unwrap();
                let ws_stream = tokio_tungstenite::accept_async(tls_stream).await.unwrap();        
                let peer = PeerWs::new(state.clone(), ws_stream).await?;
                
                let state = Arc::clone(&state);

                {
                    let mut state = state.lock().await;
                    state.broadcast(addr, &Bytes::from("new websocket client")).await;
                    tracing::debug!("clients: {}", state.get_clients_number());
                }

                tokio::spawn(async move {
                    tracing::debug!("accepted connection");
                    if let Err(e) = process_ws_stream(state, peer, addr).await {
                        tracing::info!("an error occurred; error = {:?}", e);
                    }
                });
            },
        };

    }
}

