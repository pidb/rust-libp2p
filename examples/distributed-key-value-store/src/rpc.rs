use std::str::FromStr;

use anyhow::anyhow;
use flume::Sender;
use tokio::sync::oneshot;
use tonic::transport;
use tonic::Request;
use tonic::Response;
use tonic::Status;

use crate::message::NetworkEvent;
use crate::message::NetworkMessage;

include!(concat!(env!("OUT_DIR"), "/distributedkv.rs"));

pub(crate) struct KVServiceImpl {
    network_msg_sender: Sender<NetworkMessage>,
}

impl KVServiceImpl {
    pub(crate) fn new(network_msg_sender: Sender<NetworkMessage>) -> anyhow::Result<Self> {
        Ok(Self { network_msg_sender })
    }
}

#[async_trait::async_trait]
impl kv_service_server::KvService for KVServiceImpl {
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        let request = request.into_inner();
        let (tx, rx) = oneshot::channel();
        self.network_msg_sender
            .send(NetworkMessage::Get(request, tx))
            .map_err(|_| Status::internal("send to p2p faield"))?;

        let network_event = rx
            .await
            .map_err(|_| Status::internal("receive p2p response failed"))?;

        match network_event {
            NetworkEvent::GetRequestOutbound(response) => Ok(Response::new(response)),
            _ => Err(Status::internal("unknow response")),
        }
    }

    async fn put(&self, request: Request<PutRequest>) -> Result<Response<PutResponse>, Status> {
        let request = request.into_inner();
        let (tx, rx) = oneshot::channel();
        self.network_msg_sender
            .send(NetworkMessage::Put(request, tx))
            .map_err(|_| Status::internal("send to p2p faield"))?;

        let network_event = rx
            .await
            .map_err(|_| Status::internal("receive p2p response failed"))?;

        match network_event {
            NetworkEvent::PutRequestOutbound(response) => Ok(Response::new(response)),
            _ => Err(Status::internal("unknow response")),
        }
    }
}

pub(crate) struct KVServiceClientImpl {
    #[allow(unused)]
    channel: transport::Channel,
    client: kv_service_client::KvServiceClient<transport::Channel>,
}

impl KVServiceClientImpl {
    pub(crate) async fn new(addr: &str) -> anyhow::Result<Self> {
        let channel = transport::Endpoint::from_str(addr)
            .map_err(|err| anyhow!("{}", err))?
            .connect()
            .await
            .map_err(|err| anyhow!("{}", err))?;

        let client = kv_service_client::KvServiceClient::new(channel.clone());
        Ok(Self { channel, client })
    }

    pub(crate) async fn get(&mut self, key: &[u8]) -> anyhow::Result<Vec<u8>> {
        let request = Request::new(GetRequest { key: key.to_vec() });
        let response = self
            .client
            .get(request)
            .await
            .map(|response| response.into_inner())
            .map_err(|err| anyhow!("{}", err))?;

        Ok(response.value)
    }

    pub(crate) async fn put(&mut self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        let request = Request::new(PutRequest {
            key: key.to_vec(),
            value: value.to_vec(),
        });

        let _ = self
            .client
            .put(request)
            .await
            .map(|response| response.into_inner())
            .map_err(|err| anyhow!("{}", err))?;

        Ok(())
    }
}

pub(crate) async fn run(
    addr: String,
    network_msg_sender: Sender<NetworkMessage>,
) -> anyhow::Result<()> {
    tracing::info!("RPC server listen on: {}", addr);
    let kv_service = KVServiceImpl::new(network_msg_sender)?;
    transport::Server::builder()
        .concurrency_limit_per_connection(128)
        .add_service(kv_service_server::KvServiceServer::new(kv_service))
        .serve(addr.parse()?)
        .await
        .map_err(|err| anyhow!("{}", err))
}
