use libp2p::request_response;

use super::codec::APICodec;

struct APIBehaviour {
    innet: request_response::Behaviour<APICodec<&'static str, APIRequest, APIResponse>>,
}

pub struct DistributedKVBehaviour {
    pub api: APIBehaviour,
}
