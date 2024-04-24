// include!(concat!(env!("OUT_DIR"), "/distributedkv.rs"));

use tokio::sync::oneshot;

use crate::rpc::GetRequest;
use crate::rpc::GetResponse;
use crate::rpc::PutRequest;
use crate::rpc::PutResponse;

pub(crate) enum NetworkEvent {
    GetRequestOutbound(GetResponse),
    PutRequestOutbound(PutResponse),
}

pub(crate) enum NetworkMessage {
    Get(GetRequest, oneshot::Sender<NetworkEvent>),
    Put(PutRequest, oneshot::Sender<NetworkEvent>),
}
