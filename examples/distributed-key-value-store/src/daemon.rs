use libp2p::identity::Keypair;
use tokio::signal::{
    self,
    unix::{signal, SignalKind},
};

use crate::{
    message::NetworkMessage,
    rpc::{self},
    service::DistributedKVService,
};

pub async fn run(rpc_addr: &str) -> anyhow::Result<()> {
    let new_key_pair = Keypair::generate_secp256k1();
    tracing::info!("Peer {} generate", new_key_pair.public().to_peer_id());

    let addr = rpc_addr.to_string();
    let (network_msg_tx, network_msg_rx) = flume::unbounded::<NetworkMessage>();

    let service = DistributedKVService::new(new_key_pair.clone(), network_msg_rx)?;
    tokio::spawn(async move { service.run().await });
    tokio::spawn(async move { rpc::run(addr, network_msg_tx).await.unwrap() });

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = signal::ctrl_c() => {
            tracing::info!("Interrupt by keyboard enter");
            Ok(())
        },
        _ = terminate.recv() => {
            tracing::info!("Received SIGTERM");
            Ok(())
        },
    }
}
