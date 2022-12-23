// Copyright 2021 Protocol Labs.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! [`NetworkBehaviour`] to act as a direct connection upgrade through relay node.

use crate::handler;
use crate::handler::{direct, relayed, Role};
use crate::protocol;
use either::Either;
use libp2p_core::connection::{ConnectedPoint, ConnectionId};
use libp2p_core::multiaddr::Protocol;
use libp2p_core::{Endpoint, Multiaddr, PeerId};
use libp2p_swarm::behaviour::{ConnectionClosed, ConnectionEstablished, DialFailure, FromSwarm};
use libp2p_swarm::dial_opts::{self, DialOpts};
use libp2p_swarm::{
    dummy, ConnectionHandlerUpgrErr, ExternalAddresses, NetworkBehaviour, NetworkBehaviourAction,
    NotifyHandler, PollParameters, THandler, THandlerInEvent, THandlerOutEvent,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll};
use thiserror::Error;

const MAX_NUMBER_OF_UPGRADE_ATTEMPTS: u8 = 3;

/// The events produced by the [`Behaviour`].
#[derive(Debug)]
pub enum Event {
    InitiatedDirectConnectionUpgrade {
        remote_peer_id: PeerId,
        local_relayed_addr: Multiaddr,
    },
    RemoteInitiatedDirectConnectionUpgrade {
        remote_peer_id: PeerId,
        remote_relayed_addr: Multiaddr,
    },
    DirectConnectionUpgradeSucceeded {
        remote_peer_id: PeerId,
    },
    DirectConnectionUpgradeFailed {
        remote_peer_id: PeerId,
        error: Error,
    },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to dial peer.")]
    Dial,
    #[error("Failed to establish substream: {0}.")]
    Handler(ConnectionHandlerUpgrErr<void::Void>),
}

pub struct Behaviour {
    /// Queue of actions to return when polled.
    queued_actions: VecDeque<ActionBuilder>,

    direct_outgoing_connection_attempts: HashMap<ConnectionId, (ConnectionId, handler::Role)>,

    /// All direct (non-relayed) connections.
    direct_connections: HashMap<PeerId, HashSet<ConnectionId>>,

    external_addresses: ExternalAddresses,

    local_peer_id: PeerId,
}

impl Behaviour {
    pub fn new(local_peer_id: PeerId) -> Self {
        Behaviour {
            queued_actions: Default::default(),
            direct_outgoing_connection_attempts: Default::default(),
            direct_connections: Default::default(),
            external_addresses: Default::default(),
            local_peer_id,
        }
    }

    fn on_connection_established(
        &mut self,
        ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint: connected_point,
            ..
        }: ConnectionEstablished,
    ) {
        if connected_point.is_relayed() {
            if connected_point.is_listener() && !self.direct_connections.contains_key(&peer_id) {
                // TODO: Try dialing the remote peer directly. Specification:
                //
                // > The protocol starts with the completion of a relay connection from A to B. Upon
                // observing the new connection, the inbound peer (here B) checks the addresses
                // advertised by A via identify. If that set includes public addresses, then A may
                // be reachable by a direct connection, in which case B attempts a unilateral
                // connection upgrade by initiating a direct connection to A.
                //
                // https://github.com/libp2p/specs/blob/master/relay/DCUtR.md#the-protocol
                self.queued_actions.extend([
                    ActionBuilder::Connect {
                        peer_id,
                        attempt: 1,
                        handler: NotifyHandler::One(connection_id),
                    },
                    NetworkBehaviourAction::GenerateEvent(
                        Event::InitiatedDirectConnectionUpgrade {
                            remote_peer_id: peer_id,
                            local_relayed_addr: match connected_point {
                                ConnectedPoint::Listener { local_addr, .. } => local_addr.clone(),
                                ConnectedPoint::Dialer { .. } => unreachable!("Due to outer if."),
                            },
                        },
                    )
                    .into(),
                ]);
            }
        } else {
            self.direct_connections
                .entry(peer_id)
                .or_default()
                .insert(connection_id);
        }
    }

    fn on_dial_failure(&mut self, DialFailure { peer_id, id, .. }: DialFailure) {
        let (relayed_connection_id, role) =
            match self.direct_outgoing_connection_attempts.remove(&id) {
                None => return,
                Some((relayed_connection_id, role)) => (relayed_connection_id, role),
            };
        let attempt = match role {
            Role::Listener => return,
            Role::Initiator { attempt } => attempt,
        };

        let peer_id = peer_id.expect("Peer of `Prototype::DirectConnection` is always known.");
        if attempt < MAX_NUMBER_OF_UPGRADE_ATTEMPTS {
            self.queued_actions.push_back(ActionBuilder::Connect {
                peer_id,
                handler: NotifyHandler::One(relayed_connection_id),
                attempt: attempt + 1,
            });
        } else {
            self.queued_actions.extend([
                NetworkBehaviourAction::NotifyHandler {
                    peer_id,
                    handler: NotifyHandler::One(relayed_connection_id),
                    event: Either::Left(handler::relayed::Command::UpgradeFinishedDontKeepAlive),
                }
                .into(),
                NetworkBehaviourAction::GenerateEvent(Event::DirectConnectionUpgradeFailed {
                    remote_peer_id: peer_id,
                    error: Error::Dial,
                })
                .into(),
            ]);
        }
    }

    fn on_connection_closed(
        &mut self,
        ConnectionClosed {
            peer_id,
            connection_id,
            endpoint: connected_point,
            ..
        }: ConnectionClosed<<Self as NetworkBehaviour>::ConnectionHandler>,
    ) {
        if !connected_point.is_relayed() {
            let connections = self
                .direct_connections
                .get_mut(&peer_id)
                .expect("Peer of direct connection to be tracked.");
            connections
                .remove(&connection_id)
                .then_some(())
                .expect("Direct connection to be tracked.");
            if connections.is_empty() {
                self.direct_connections.remove(&peer_id);
            }
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler =
        Either<relayed::Handler, Either<direct::Handler, dummy::ConnectionHandler>>;
    type OutEvent = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _peer: PeerId,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, Box<dyn std::error::Error + Send + 'static>> {
        let is_relayed = local_addr.iter().any(|p| p == Protocol::P2pCircuit); // TODO: Make this an extension on `Multiaddr`.

        let relayed_connection_id = match self
            .direct_outgoing_connection_attempts
            .remove(&connection_id)
        {
            None => {
                let handler = if is_relayed {
                    Either::Left(relayed::Handler::new(ConnectedPoint::Listener {
                        local_addr: local_addr.clone(),
                        send_back_addr: remote_addr.clone(),
                    })) // TODO: We could make two `relayed::Handler` here, one inbound one outbound.
                } else {
                    Either::Right(Either::Right(dummy::ConnectionHandler))
                };

                return Ok(handler);
            }
            // TODO: Why are we ignoring `Role`?
            Some((relayed_connection_id, _)) => relayed_connection_id,
        };

        assert!(
            !is_relayed,
            "`Prototype::DirectConnection` is never created for relayed connection."
        );

        Ok(Either::Right(Either::Left(direct::Handler::new(
            relayed_connection_id,
        ))))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        connection_id: ConnectionId,
    ) -> Result<THandler<Self>, Box<dyn std::error::Error + Send + 'static>> {
        let is_relayed = addr.iter().any(|p| p == Protocol::P2pCircuit); // TODO: Make this an extension on `Multiaddr`.

        let relayed_connection_id = match self
            .direct_outgoing_connection_attempts
            .remove(&connection_id)
        {
            None => {
                let handler = if is_relayed {
                    Either::Left(relayed::Handler::new(ConnectedPoint::Dialer {
                        address: addr.clone(),
                        role_override,
                    })) // TODO: We could make two `relayed::Handler` here, one inbound one outbound.
                } else {
                    Either::Right(Either::Right(dummy::ConnectionHandler))
                };

                return Ok(handler);
            }
            // TODO: Why are we ignoring `Role`?
            Some((relayed_connection_id, _)) => relayed_connection_id,
        };

        assert!(
            !is_relayed,
            "`Prototype::DirectConnection` is never created for relayed connection."
        );

        Ok(Either::Right(Either::Left(direct::Handler::new(
            relayed_connection_id,
        ))))
    }

    fn on_connection_handler_event(
        &mut self,
        event_source: PeerId,
        relayed_connection_id: ConnectionId,
        handler_event: THandlerOutEvent<Self>,
    ) {
        match handler_event {
            Either::Left(handler::relayed::Event::InboundConnectRequest {
                inbound_connect,
                remote_addr,
            }) => {
                self.queued_actions.extend([
                    ActionBuilder::AcceptInboundConnect {
                        peer_id: event_source,
                        handler: NotifyHandler::One(relayed_connection_id),
                        inbound_connect,
                    },
                    NetworkBehaviourAction::GenerateEvent(
                        Event::RemoteInitiatedDirectConnectionUpgrade {
                            remote_peer_id: event_source,
                            remote_relayed_addr: remote_addr,
                        },
                    )
                    .into(),
                ]);
            }
            Either::Left(handler::relayed::Event::InboundNegotiationFailed { error }) => {
                self.queued_actions.push_back(
                    NetworkBehaviourAction::GenerateEvent(Event::DirectConnectionUpgradeFailed {
                        remote_peer_id: event_source,
                        error: Error::Handler(error),
                    })
                    .into(),
                );
            }
            Either::Left(handler::relayed::Event::InboundConnectNegotiated(remote_addrs)) => {
                let (action, connection_id) = NetworkBehaviourAction::dial(
                    DialOpts::peer_id(event_source)
                        .addresses(remote_addrs)
                        .condition(dial_opts::PeerCondition::Always)
                        .build(),
                );

                self.queued_actions.push_back(action.into());
                self.direct_outgoing_connection_attempts.insert(
                    connection_id,
                    (relayed_connection_id, handler::Role::Listener),
                );
            }
            Either::Left(handler::relayed::Event::OutboundNegotiationFailed { error }) => {
                self.queued_actions.push_back(
                    NetworkBehaviourAction::GenerateEvent(Event::DirectConnectionUpgradeFailed {
                        remote_peer_id: event_source,
                        error: Error::Handler(error),
                    })
                    .into(),
                );
            }
            Either::Left(handler::relayed::Event::OutboundConnectNegotiated {
                remote_addrs,
                attempt,
            }) => {
                let (action, connection_id) = NetworkBehaviourAction::dial(
                    DialOpts::peer_id(event_source)
                        .addresses(remote_addrs)
                        .condition(dial_opts::PeerCondition::Always)
                        .override_role()
                        .build(),
                );

                self.queued_actions.push_back(action.into());
                self.direct_outgoing_connection_attempts.insert(
                    connection_id,
                    (relayed_connection_id, handler::Role::Initiator { attempt }),
                );
            }
            Either::Right(Either::Left(
                handler::direct::Event::DirectConnectionUpgradeSucceeded {
                    relayed_connection_id,
                },
            )) => {
                self.queued_actions.extend([
                    NetworkBehaviourAction::NotifyHandler {
                        peer_id: event_source,
                        handler: NotifyHandler::One(relayed_connection_id),
                        event: Either::Left(
                            handler::relayed::Command::UpgradeFinishedDontKeepAlive,
                        ),
                    }
                    .into(),
                    NetworkBehaviourAction::GenerateEvent(
                        Event::DirectConnectionUpgradeSucceeded {
                            remote_peer_id: event_source,
                        },
                    )
                    .into(),
                ]);
            }
            Either::Right(Either::Right(event)) => void::unreachable(event),
        };
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, THandlerInEvent<Self>>> {
        if let Some(action) = self.queued_actions.pop_front() {
            return Poll::Ready(action.build(self.local_peer_id, &self.external_addresses));
        }

        Poll::Pending
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        self.external_addresses.on_swarn_event(&event);

        match event {
            FromSwarm::ConnectionEstablished(connection_established) => {
                self.on_connection_established(connection_established)
            }
            FromSwarm::ConnectionClosed(connection_closed) => {
                self.on_connection_closed(connection_closed)
            }
            FromSwarm::DialFailure(dial_failure) => self.on_dial_failure(dial_failure),
            FromSwarm::AddressChange(_)
            | FromSwarm::ListenFailure(_)
            | FromSwarm::NewListener(_)
            | FromSwarm::NewListenAddr(_)
            | FromSwarm::ExpiredListenAddr(_)
            | FromSwarm::ListenerError(_)
            | FromSwarm::ListenerClosed(_)
            | FromSwarm::NewExternalAddr(_)
            | FromSwarm::ExpiredExternalAddr(_) => {}
        }
    }
}

/// A [`NetworkBehaviourAction`], either complete, or still requiring data from [`PollParameters`]
/// before being returned in [`Behaviour::poll`].
enum ActionBuilder {
    Done(NetworkBehaviourAction<Event, THandlerInEvent<Behaviour>>),
    Connect {
        attempt: u8,
        handler: NotifyHandler,
        peer_id: PeerId,
    },
    AcceptInboundConnect {
        inbound_connect: Box<protocol::inbound::PendingConnect>,
        handler: NotifyHandler,
        peer_id: PeerId,
    },
}

impl From<NetworkBehaviourAction<Event, THandlerInEvent<Behaviour>>> for ActionBuilder {
    fn from(action: NetworkBehaviourAction<Event, THandlerInEvent<Behaviour>>) -> Self {
        Self::Done(action)
    }
}

impl ActionBuilder {
    fn build(
        self,
        local_peer_id: PeerId,
        external_addresses: &ExternalAddresses,
    ) -> NetworkBehaviourAction<Event, THandlerInEvent<Behaviour>> {
        let obs_addrs = || {
            external_addresses
                .iter()
                .cloned()
                .filter(|a| !a.iter().any(|p| p == Protocol::P2pCircuit))
                .map(|a| a.with(Protocol::P2p(local_peer_id.into())))
                .collect()
        };

        match self {
            ActionBuilder::Done(action) => action,
            ActionBuilder::AcceptInboundConnect {
                inbound_connect,
                handler,
                peer_id,
            } => NetworkBehaviourAction::NotifyHandler {
                handler,
                peer_id,
                event: Either::Left(handler::relayed::Command::AcceptInboundConnect {
                    inbound_connect,
                    obs_addrs: obs_addrs(),
                }),
            },
            ActionBuilder::Connect {
                attempt,
                handler,
                peer_id,
            } => NetworkBehaviourAction::NotifyHandler {
                handler,
                peer_id,
                event: Either::Left(handler::relayed::Command::Connect {
                    attempt,
                    obs_addrs: obs_addrs(),
                }),
            },
        }
    }
}
