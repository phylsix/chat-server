use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use dashmap::DashMap;
use futures::{stream::SplitStream, SinkExt, StreamExt};
use tokio::{net::TcpStream, sync::mpsc::Sender};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{fmt::Layer, layer::SubscriberExt, util::SubscriberInitExt, Layer as _};

const BUFFER_SIZE: usize = 100;

#[derive(Debug, Default)]
struct State {
    peers: DashMap<SocketAddr, Sender<Arc<Message>>>,
}

enum Message {
    UserJoined(String),
    UserLeft(String),
    Chat { from: String, message: String },
}

struct Peer {
    username: String,
    stream: SplitStream<Framed<TcpStream, LinesCodec>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let layer = Layer::new().with_filter(LevelFilter::INFO);
    tracing_subscriber::registry().with(layer).init();

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on: {}", addr);

    let state = Arc::new(State::default());

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Accepted connection from: {}", addr);

        let _state = Arc::clone(&state);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, addr, _state).await {
                error!("Error to handle connection {}: {:?}", addr, e);
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, addr: SocketAddr, state: Arc<State>) -> Result<()> {
    info!("Handling connection on addr: {}", addr.port());

    let mut stream = Framed::new(stream, LinesCodec::new());

    // ask user to enter username
    stream.send("Enter your username: ").await?;

    let username = match stream.next().await {
        Some(Ok(username)) => username,
        Some(Err(e)) => return Err(e.into()),
        _ => return Ok(()),
    };

    // user joined chat
    let mut peer = state.add(addr, username, stream).await;
    let message = Arc::new(Message::user_joined(&peer.username));
    info!("{}", message);
    state.broadcast(addr, message).await;

    // handle incoming messages
    while let Some(line) = peer.stream.next().await {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                error!("Error to read line from {}: {:?}", addr, e);
                break;
            }
        };

        if line.is_empty() {
            continue;
        }

        let message = Arc::new(Message::chat(&peer.username, &line));
        state.broadcast(addr, message).await;
    }

    // user left chat
    state.peers.remove(&addr);
    let message = Arc::new(Message::user_left(&peer.username));
    info!("{}", message);
    state.broadcast(addr, message).await;

    Ok(())
}

impl State {
    async fn add(
        &self,
        addr: SocketAddr,
        username: String,
        stream: Framed<TcpStream, LinesCodec>,
    ) -> Peer {
        let (tx, mut rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        self.peers.insert(addr, tx);

        let (mut stream_sender, stream_receiver) = stream.split();

        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if let Err(e) = stream_sender.send(line.to_string()).await {
                    error!("Error to send message to {}: {:?}", addr, e);
                    break;
                }
            }
        });

        Peer {
            username,
            stream: stream_receiver,
        }
    }

    async fn broadcast(&self, from_addr: SocketAddr, message: Arc<Message>) {
        for peer in self.peers.iter() {
            if peer.key() == &from_addr {
                continue;
            }
            if let Err(e) = peer.value().send(message.clone()).await {
                error!("Error to send message to {}: {:?}", peer.key(), e);
            }
        }
    }
}

impl Message {
    fn user_joined(username: &str) -> Self {
        let content = format!("+++ {} has joined the chat +++", username);
        Self::UserJoined(content)
    }

    fn user_left(username: &str) -> Self {
        let content = format!("--- {} has left the chat ---", username);
        Self::UserLeft(content)
    }

    fn chat(from: &str, message: &str) -> Self {
        Self::Chat {
            from: from.to_string(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserJoined(content) => write!(f, "{}", content),
            Self::UserLeft(content) => write!(f, "{}", content),
            Self::Chat { from, message } => write!(f, "[{}]: {}", from, message),
        }
    }
}
