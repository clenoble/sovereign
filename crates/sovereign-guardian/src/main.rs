//! sovereign-guardian CLI — the G1 scaffold surface.
//!
//! The shipping guardian app is mobile-first (Tauri Android, D2/D3 UI);
//! this binary is the engine's dev/test harness and a desktop fallback:
//! `init` mints the identity, `enroll` runs the in-person handshake,
//! `status` lists duties. The long-running `run` mode (relay
//! reservation + mailbox heartbeat loop) lands with G3.

use clap::{Parser, Subcommand};
use sovereign_guardian::{custody::CustodyStore, enroll, state::GuardianIdentity};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sovereign-guardian",
    about = "Hold a friend's recovery key-shard. 3 of 5 shards recover; yours alone reveals nothing."
)]
struct Cli {
    /// Data directory (identity, duties, sealed shards).
    #[arg(long, default_value = "guardian-data")]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create (or show) this install's guardian identity.
    Init,
    /// Enroll as a guardian: scan the owner's QR (offer) and type the
    /// spoken code.
    Enroll {
        /// The offer payload from the owner's QR (base64).
        #[arg(long)]
        offer: String,
        /// The short code the owner reads out.
        #[arg(long)]
        code: String,
        /// How the owner's roster should label you.
        #[arg(long, default_value = "A friend")]
        label: String,
    },
    /// List guarding duties.
    Status,
    /// G3: serve held shards — reserve a relay circuit and answer recovery
    /// requests until stopped. Friends leave this running (mobile: a
    /// background service).
    Run {
        /// Seed relay multiaddr(s) to reserve a circuit on (repeatable), so a
        /// recovering device can reach this guardian across NATs. Each must
        /// end `/p2p/<relay-peer-id>`.
        #[arg(long = "relay")]
        relays: Vec<String>,
        /// DEV/E2E ONLY: auto-approve incoming recovery requests. In the
        /// shipping app the human approves after verifying out of band.
        #[arg(long, default_value_t = false)]
        auto_approve: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,libp2p_swarm=warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let identity = GuardianIdentity::open(&cli.data_dir)?;

    match cli.command {
        Command::Init => {
            println!("guardian identity: {}", identity.peer_id()?);
            println!("data dir: {}", cli.data_dir.display());
        }
        Command::Enroll { offer, code, label } => {
            let mut custody = CustodyStore::open(&cli.data_dir)?;
            let duty = enroll::enroll(
                &offer,
                &code,
                &label,
                &identity,
                &mut custody,
                enroll::ENROLL_TIMEOUT,
            )
            .await?;
            println!(
                "enrolled: guarding for {} ({}) — shard {} (epoch {}, {}-of-{})",
                duty.owner_label, duty.owner_tag, duty.shard_id, duty.epoch, duty.threshold, duty.total
            );
        }
        Command::Run { relays, auto_approve } => {
            sovereign_guardian::serve::run(&cli.data_dir, &identity, relays, auto_approve).await?;
        }
        Command::Status => {
            let custody = CustodyStore::open(&cli.data_dir)?;
            if custody.duties().is_empty() {
                println!("no guarding duties yet");
            } else {
                println!("guarding for {} person(s):", custody.duties().len());
                for d in custody.duties() {
                    println!(
                        "  {} ({}) — shard {} epoch {} ({}-of-{}), enrolled {}, last heartbeat {}",
                        d.owner_label,
                        d.owner_tag,
                        d.shard_id,
                        d.epoch,
                        d.threshold,
                        d.total,
                        d.enrolled_at,
                        d.last_heartbeat_at.as_deref().unwrap_or("never"),
                    );
                }
            }
        }
    }
    Ok(())
}
