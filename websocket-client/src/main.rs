use std::sync::Arc;
use tokio_rustls::TlsConnector;
use async_tungstenite::tokio::connect_async_with_tls_connector;
use async_tungstenite::tungstenite::Message;
use futures_util::SinkExt;
use futures_util::StreamExt;

use certs::create_client_config;

#[tokio::main]
async fn main()
{
    tracing_subscriber::fmt().init();

    let config = create_client_config();
    let connector = TlsConnector::from(Arc::new(config));

    const CONNECT_ADDR: &str = "wss://127.0.0.1:9900";

    let url = url::Url::parse(&CONNECT_ADDR).unwrap();

    let (mut ws_stream, _) = connect_async_with_tls_connector(url, Some(connector)).await.expect("Failed to connect");

    loop {
        ws_stream.send(Message::Text("HELLO".into())).await.unwrap();
        
        while let Some(msg) = ws_stream.next().await {
            tracing::info!("recv text: {:?} from server", msg.unwrap());
        }
    }//loop

}
