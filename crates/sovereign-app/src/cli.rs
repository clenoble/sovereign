use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sovereign", about = "Sovereign GE — your data, your rules")]
pub struct Cli {
    /// Path to config file
    #[arg(long)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch the GUI (Phase 1b)
    Run,

    /// Create a new document
    CreateDoc {
        #[arg(long)]
        title: String,
        #[arg(long)]
        thread_id: String,
        #[arg(long, default_value_t = true)]
        is_owned: bool,
    },

    /// Get a document by ID
    GetDoc {
        #[arg(long)]
        id: String,
    },

    /// List documents, optionally filtered by thread
    ListDocs {
        #[arg(long)]
        thread_id: Option<String>,
    },

    /// Update a document
    UpdateDoc {
        #[arg(long)]
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        content: Option<String>,
    },

    /// Delete a document
    DeleteDoc {
        #[arg(long)]
        id: String,
    },

    /// Create a new thread
    CreateThread {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },

    /// List all threads
    ListThreads,

    /// Add a relationship between two documents
    AddRelationship {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        relation_type: String,
        #[arg(long, default_value_t = 0.5)]
        strength: f32,
    },

    /// List relationships for a document
    ListRelationships {
        #[arg(long)]
        doc_id: String,
    },

    /// Commit a document snapshot
    Commit {
        #[arg(long)]
        doc_id: String,
        #[arg(long)]
        message: String,
    },

    /// List commits for a document
    ListCommits {
        #[arg(long)]
        doc_id: String,
    },

    /// Encrypt all existing plaintext documents (idempotent)
    #[cfg(feature = "encryption")]
    EncryptData,

    /// Pair with another device on the local network
    #[cfg(feature = "p2p")]
    PairDevice {
        #[arg(long)]
        peer_id: String,
    },

    /// List paired devices
    #[cfg(feature = "p2p")]
    ListDevices,

    /// Enroll a guardian for key recovery
    #[cfg(feature = "p2p")]
    EnrollGuardian {
        #[arg(long)]
        name: String,
        #[arg(long)]
        peer_id: String,
    },

    /// List enrolled guardians
    #[cfg(feature = "encryption")]
    ListGuardians,

    /// Initiate key recovery from guardians
    #[cfg(feature = "encryption")]
    InitiateRecovery,

    /// List all contacts
    ListContacts,

    /// List all conversations, optionally filtered by channel
    ListConversations {
        #[arg(long)]
        channel: Option<String>,
    },
}
