//! Deployable Sovereign relay (M1.5) — the product replacement for the M0
//! spike relay binary.
//!
//! Runs relay-v2 (circuit reservations for NATed peers), identify, ping,
//! and the persistent store-and-forward mailbox. It sees only opaque
//! traffic: relayed bytes are E2E-encrypted, and mailbox blobs are
//! sender-sealed ciphertext — the relay learns routing metadata, never
//! keys or plaintext. Deterministic identity from a seed file so its
//! PeerId (the seed-list entry) is stable across restarts.

mod deposit_token;
mod mailbox_store;

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use libp2p::{
    core::multiaddr::Protocol,
    futures::StreamExt,
    identify, identity, noise, ping, relay,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, StreamProtocol,
};
use sovereign_core::mailbox::{MailboxRequest, MailboxResponse, MAILBOX_PROTOCOL};

use mailbox_store::{MailboxConfig, MailboxStore, PutOutcome};

#[derive(Debug, Parser)]
#[command(name = "sovereign-relay", version)]
struct Opts {
    /// TCP + UDP/QUIC listen port.
    #[arg(long, default_value_t = 4001)]
    port: u16,
    /// Data dir for the identity seed + mailbox store.
    #[arg(long, default_value = "./sovereign-relay-data")]
    data_dir: PathBuf,
    /// Mailbox sweep interval (seconds).
    #[arg(long, default_value_t = 3600)]
    sweep_secs: u64,
    /// RELAY-002 inc-2: require a valid recipient-issued deposit token on every
    /// `Put`. Closes the rotating-keypair Sybil residual the per-sender caps
    /// leave open. Off by default until token issuance is wired into enrollment
    /// (inc-2c); turn on for the deployed seed node once senders carry tokens.
    #[arg(long, default_value_t = false)]
    require_deposit_token: bool,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    relay: relay::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    mailbox: request_response::cbor::Behaviour<MailboxRequest, MailboxResponse>,
}

/// Load a persisted ed25519 identity or create+persist one, so the relay
/// keeps the same PeerId (its seed-list entry) across restarts.
fn load_or_create_identity(data_dir: &PathBuf) -> std::io::Result<identity::Keypair> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join("relay_identity.key");
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(kp) = identity::Keypair::from_protobuf_encoding(&bytes) {
            return Ok(kp);
        }
    }
    let kp = identity::Keypair::generate_ed25519();
    let enc = kp
        .to_protobuf_encoding()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    // Best-effort private perms; on Windows the file inherits the data dir.
    std::fs::write(&path, enc)?;
    Ok(kp)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,libp2p_relay=info".into()),
        )
        .init();
    let opts = Opts::parse();

    let keypair = load_or_create_identity(&opts.data_dir)?;
    let mailbox_cfg = MailboxConfig {
        require_deposit_token: opts.require_deposit_token,
        ..MailboxConfig::default()
    };
    if opts.require_deposit_token {
        tracing::info!("deposit-token enforcement ON (RELAY-002): Put requires a valid recipient token");
    }
    let mut mailbox = MailboxStore::open(opts.data_dir.join("mailbox"), mailbox_cfg);

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| Behaviour {
            relay: relay::Behaviour::new(key.public().to_peer_id(), Default::default()),
            identify: identify::Behaviour::new(
                identify::Config::new("/sovereign/relay/1".into(), key.public())
                    .with_agent_version("sovereign-relay".into()),
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
            mailbox: request_response::cbor::Behaviour::new(
                [(StreamProtocol::new(MAILBOX_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default(),
            ),
        })?
        .build();

    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(opts.port)),
    )?;
    swarm.listen_on(
        Multiaddr::empty()
            .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(opts.port))
            .with(Protocol::QuicV1),
    )?;

    let peer_id = *swarm.local_peer_id();
    tracing::info!(%peer_id, "sovereign-relay up — seed-list entry is /ip4/<host>/udp/{}/quic-v1/p2p/{peer_id}", opts.port);

    let mut sweep = tokio::time::interval(Duration::from_secs(opts.sweep_secs.max(60)));

    loop {
        tokio::select! {
            _ = sweep.tick() => {
                let removed = mailbox.sweep(now_unix());
                if removed > 0 {
                    tracing::info!(removed, "mailbox sweep reclaimed expired items");
                }
            }
            event = swarm.next() => {
                let Some(event) = event else { break };
                handle_event(event, &mut swarm, &mut mailbox);
            }
        }
    }
    Ok(())
}

fn handle_event(
    event: SwarmEvent<BehaviourEvent>,
    swarm: &mut libp2p::Swarm<Behaviour>,
    mailbox: &mut MailboxStore,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            let p2p = address.with(Protocol::P2p(*swarm.local_peer_id()));
            tracing::info!(%p2p, "listening");
        }
        SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
            info, ..
        })) => {
            // Learn our public address so relayed reservations hand out
            // dialable addrs.
            swarm.add_external_address(info.observed_addr);
        }
        SwarmEvent::Behaviour(BehaviourEvent::Mailbox(request_response::Event::Message {
            peer,
            message: request_response::Message::Request { request, channel, .. },
            ..
        })) => {
            let response = match request {
                MailboxRequest::Put { to, blob, token } => {
                    let size = blob.len();
                    // RELAY-002 inc-2: gate the deposit on a valid recipient token
                    // BEFORE it can touch a queue. Only the mailbox owner can mint
                    // one, so a rotating-keypair Sybil can't manufacture permission
                    // for a victim's mailbox. Off unless the relay was started with
                    // --require-deposit-token.
                    let gate = if mailbox.require_deposit_token() {
                        match token.as_ref() {
                            Some(t) => {
                                deposit_token::verify_deposit_token(t, &to, &peer, now_unix())
                            }
                            None => Err("deposit token required"),
                        }
                    } else {
                        Ok(())
                    };
                    match gate {
                        Err(why) => {
                            tracing::warn!(%peer, why, "mailbox PUT denied (deposit token)");
                            MailboxResponse::Denied(why.into())
                        }
                        Ok(()) => match mailbox.put(&peer.to_string(), &to, blob, now_unix()) {
                            PutOutcome::Stored => {
                                tracing::info!(%peer, size, "mailbox PUT");
                                MailboxResponse::PutAck { dedup: false }
                            }
                            PutOutcome::Duplicate => MailboxResponse::PutAck { dedup: true },
                            PutOutcome::Denied(why) => {
                                tracing::warn!(%peer, why, "mailbox PUT denied");
                                MailboxResponse::Denied(why.into())
                            }
                        },
                    }
                }
                MailboxRequest::Pull => {
                    let items = mailbox.pull(&peer.to_bytes(), now_unix());
                    tracing::info!(%peer, count = items.len(), "mailbox PULL");
                    MailboxResponse::Items(items)
                }
            };
            let _ = swarm.behaviour_mut().mailbox.send_response(channel, response);
        }
        SwarmEvent::Behaviour(BehaviourEvent::Relay(ev)) => {
            tracing::debug!(?ev, "relay event");
        }
        _ => {}
    }
}
