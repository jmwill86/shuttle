use std::net::SocketAddr;

use clap::Parser;
use shuttle_provisioner::{Args, MyProvisioner, ProvisionerServer};
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let Args {
        ip,
        port,
        shared_pg_uri,
        shared_mongodb_uri,
        fqdn,
        internal_pg_address,
        internal_mongodb_address,
    } = Args::parse();
    let addr = SocketAddr::new(ip, port);

    let provisioner = MyProvisioner::new(
        &shared_pg_uri,
        &shared_mongodb_uri,
        fqdn.to_string(),
        internal_pg_address,
        internal_mongodb_address,
    )
    .await
    .unwrap();

    println!("starting provisioner on {}", addr);
    Server::builder()
        .add_service(ProvisionerServer::new(provisioner))
        .serve(addr)
        .await?;

    Ok(())
}
