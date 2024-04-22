use libp2p::request_response;

use super::codec::APICodec;
use super::message::ApiRequest;
use super::message::ApiResponse;

struct APIBehaviour {
    innet: request_response::Behaviour<APICodec<&'static str, ApiRequest, ApiResponse>>,
}

pub struct DistributedKVBehaviour {
    pub api: APIBehaviour,
}
