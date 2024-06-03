use anyhow::Result;
use tokio::net::TcpStream;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{fmt::Layer, layer::SubscriberExt, util::SubscriberInitExt, Layer as _};

#[tokio::main]
async fn main() -> Result<()> {
    let layer = Layer::new().with_filter(LevelFilter::INFO);
    tracing_subscriber::registry().with(layer).init();

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on: {}", addr);

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Accepted connection from: {}", addr);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                error!("Error to handle connection {}: {:?}", addr, e);
            }
        });
    }
}

async fn handle_connection(stream: TcpStream) -> Result<()> {
    info!("Handling connection: {:?}", stream);
    Ok(())
}
