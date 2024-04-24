use crate::behaviour::DistributedKVBehaviour;
use crate::behaviour::DistributedKVBehaviourEvent;
use crate::message::NetworkEvent;
use crate::message::NetworkMessage;
use crate::rpc::GetResponse;
use crate::rpc::PutResponse;
use flume::Receiver;
use futures::StreamExt;
use libp2p::kad;
use libp2p::kad::QueryId;
use libp2p::noise;
use libp2p::tcp;
use libp2p::yamux;
use libp2p::Swarm;
use libp2p::{identity::Keypair, SwarmBuilder};
use libp2p::{mdns, swarm::SwarmEvent};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::oneshot;

fn handle_mdns_event(swarm: &mut Swarm<DistributedKVBehaviour>, ev: mdns::Event) {
    match ev {
        mdns::Event::Discovered(list) => swarm.behaviour_mut().add_address(list),
        mdns::Event::Expired(list) => swarm.behaviour_mut().remove_address(list),
    }
}

fn handle_kad_event(
    _: &mut Swarm<DistributedKVBehaviour>,
    ev: kad::Event,
    pending_response_senders: &mut HashMap<QueryId, oneshot::Sender<NetworkEvent>>,
) {
    match ev {
        kad::Event::InboundRequest { request } => match request {
            kad::InboundRequest::GetRecord {
                num_closer_peers,
                present_locally,
            } => {
                println!(
                    "InboundRequest::GetRecord {}, {}",
                    num_closer_peers, present_locally
                );
            }

            _ => {}
        },
        kad::Event::OutboundQueryProgressed { id, result, .. } => match result {
            kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                key,
                providers,
                ..
            })) => {
                for peer in providers {
                    println!(
                        "Peer {peer:?} provides key {:?}",
                        std::str::from_utf8(key.as_ref()).unwrap()
                    );
                }
            }
            kad::QueryResult::GetProviders(Err(err)) => {
                eprintln!("Failed to get providers: {err:?}");
            }
            kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
                record: kad::Record { key, value, .. },
                ..
            }))) => {
                println!(
                    "Successfully get record {:?}",
                    std::str::from_utf8(key.as_ref()).unwrap()
                );

                let response_sender = match pending_response_senders.remove(&id) {
                    None => return,
                    Some(response_sender) => response_sender,
                };

                if let Err(_) =
                    response_sender.send(NetworkEvent::GetRequestOutbound(GetResponse { value }))
                {
                    tracing::error!("Send GetRequestOutbound failed")
                }
            }
            kad::QueryResult::GetRecord(Ok(_)) => {}
            kad::QueryResult::GetRecord(Err(err)) => {
                tracing::error!("Failed to get record: {err:?}");

                let response_sender = match pending_response_senders.remove(&id) {
                    None => return,
                    Some(response_sender) => response_sender,
                };

                if let Err(_) =
                    response_sender.send(NetworkEvent::GetRequestOutbound(GetResponse {
                        value: Vec::new(),
                    }))
                {
                    tracing::error!("Send GetRequestOutbound failed")
                }
            }
            kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                println!(
                    "Successfully put record {:?}",
                    std::str::from_utf8(key.as_ref()).unwrap()
                );

                let response_sender = match pending_response_senders.remove(&id) {
                    None => return,
                    Some(response_sender) => response_sender,
                };

                if let Err(_) =
                    response_sender.send(NetworkEvent::PutRequestOutbound(PutResponse {}))
                {
                    tracing::error!("Send GetRequestOutbound failed")
                }
            }
            kad::QueryResult::PutRecord(Err(err)) => {
                eprintln!("Failed to put record: {err:?}");
            }
            kad::QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                println!(
                    "Successfully put provider record {:?}",
                    std::str::from_utf8(key.as_ref()).unwrap()
                );
            }
            kad::QueryResult::StartProviding(Err(err)) => {
                eprintln!("Failed to put provider record: {err:?}");
            }
            _ => {}
        },
        _ => {}
    }
}

fn handle_distributed_kv_behaviour_event(
    swarm: &mut Swarm<DistributedKVBehaviour>,
    event: DistributedKVBehaviourEvent,
    pending_response_senders: &mut HashMap<QueryId, oneshot::Sender<NetworkEvent>>,
) {
    match event {
        DistributedKVBehaviourEvent::Kad(ev) => {
            handle_kad_event(swarm, ev, pending_response_senders)
        }
        DistributedKVBehaviourEvent::Mdns(ev) => handle_mdns_event(swarm, ev),
    }
}

fn handle_network_msg(
    swarm: &mut Swarm<DistributedKVBehaviour>,
    msg: NetworkMessage,
    pending_response_senders: &mut HashMap<QueryId, oneshot::Sender<NetworkEvent>>,
) {
    match msg {
        NetworkMessage::Get(request, response_sender) => {
            let query_id = swarm.behaviour_mut().get_record(&request.key);
            pending_response_senders.insert(query_id, response_sender);
        }
        NetworkMessage::Put(request, response_sender) => {
            let query_id = match swarm
                .behaviour_mut()
                .put_record(&request.key, &request.value)
            {
                Ok(query_id) => query_id,
                Err(err) => {
                    tracing::error!("Put request error: {}", err);
                    return;
                }
            };
            pending_response_senders.insert(query_id, response_sender);
        }
    }
}

pub(crate) struct DistributedKVService {
    swarm: Swarm<DistributedKVBehaviour>,
    network_msg_receiver: Receiver<NetworkMessage>,
    pending_response_senders: HashMap<QueryId, oneshot::Sender<NetworkEvent>>,
}

impl DistributedKVService {
    pub(crate) fn new(
        key_pair: Keypair,
        network_msg_receiver: Receiver<NetworkMessage>,
    ) -> anyhow::Result<Self> {
        let behaviour = DistributedKVBehaviour::new(key_pair.clone())?;
        let mut swarm = SwarmBuilder::with_existing_identity(key_pair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_behaviour(|_| behaviour)?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
        Ok(Self {
            swarm,
            network_msg_receiver,
            pending_response_senders: HashMap::new(),
        })
    }

    pub(crate) async fn run(mut self) {
        let mut network_stream = self.network_msg_receiver.stream();
        loop {
            tokio::select! {
                rpc_msg = network_stream.next() => match rpc_msg {
                    Some(rpc_msg) => handle_network_msg(&mut self.swarm, rpc_msg, &mut self.pending_response_senders),
                    None => break,
                },

                swarm_event = self.swarm.next() => match swarm_event {
                    Some(swarm_event) => match swarm_event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            tracing::info!("Listening in {address:?}");
                        }
                        SwarmEvent::Behaviour(ev) => {
                            handle_distributed_kv_behaviour_event(&mut self.swarm, ev, &mut self.pending_response_senders)
                        }
                        _ => {}
                    },
                    None => {break}
                }
            }
        }
    }
}
