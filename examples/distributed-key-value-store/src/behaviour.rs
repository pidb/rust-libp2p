use anyhow::anyhow;
use libp2p::identity::Keypair;
use libp2p::kad;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::Mode;
use libp2p::kad::QueryId;
use libp2p::kad::Quorum;
use libp2p::kad::Record;
use libp2p::kad::RecordKey;
use libp2p::mdns;
use libp2p::swarm::NetworkBehaviour;
use libp2p::Multiaddr;
use libp2p::PeerId;

#[derive(NetworkBehaviour)]
pub(crate) struct DistributedKVBehaviour {
    mdns: mdns::tokio::Behaviour,
    kad: kad::Behaviour<MemoryStore>,
}

impl DistributedKVBehaviour {
    pub(crate) fn new(key_pair: Keypair) -> anyhow::Result<Self> {
        let peer_id = key_pair.public().to_peer_id();
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
        let mut kad = kad::Behaviour::new(peer_id, MemoryStore::new(peer_id));
        kad.set_mode(Some(Mode::Server));
        Ok(Self { mdns, kad })
    }

    pub(crate) fn add_address(&mut self, list: Vec<(PeerId, Multiaddr)>) {
        for (peer, address) in list {
            tracing::info!("Mdns add {}, {}", peer, address);
            self.kad.add_address(&peer, address);
        }
    }
    pub(crate) fn remove_address(&mut self, list: Vec<(PeerId, Multiaddr)>) {
        for (peer, address) in list {
            tracing::info!("Mdns remove {}, {}", peer, address);
            self.kad.remove_address(&peer, &address);
        }
    }

    #[inline]
    pub(crate) fn put_record(&mut self, key: &[u8], value: &[u8]) -> anyhow::Result<QueryId> {
        self.kad
            .put_record(
                Record {
                    key: RecordKey::new(&key),
                    value: value.to_vec(),
                    publisher: None,
                    expires: None,
                },
                Quorum::One,
            )
            .map_err(|err| anyhow!("{}", err))
    }

    #[inline]
    pub(crate) fn get_record(&mut self, key: &[u8]) -> QueryId {
        self.kad.get_record(RecordKey::new(&key))
    }
}
