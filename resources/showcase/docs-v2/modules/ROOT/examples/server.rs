use std::net::SocketAddr;

// tag::imports[]
use anyhow::Result;
use axum::Router;
// end::imports[]

// tag::main[]
#[tokio::main]
async fn main() -> Result<()> {
    let app = Router::new();
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;

    Ok(())
}
// end::main[]
