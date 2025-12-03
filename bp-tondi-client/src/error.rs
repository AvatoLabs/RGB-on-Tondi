//! Error types for the Tondi RPC client

use std::num::TryFromIntError;

use amplify::hex;
use bp::Txid;

/// Errors that can occur during Tondi RPC operations
#[derive(Debug, Display, Error, From)]
#[display(inner)]
pub enum Error {
    /// Connection error: {0}
    #[display(doc_comments)]
    Connection(String),

    /// RPC call failed: {0}
    #[display(doc_comments)]
    RpcCall(String),

    /// Invalid response from server
    #[display(doc_comments)]
    InvalidServerResponse,

    /// Transaction {0} not found
    #[display(doc_comments)]
    TransactionNotFound(Txid),

    /// Block not found: {0}
    #[display(doc_comments)]
    BlockNotFound(String),

    /// Address error: {0}
    #[display(doc_comments)]
    Address(String),

    /// Serialization error: {0}
    #[display(doc_comments)]
    Serialization(String),

    /// Invalid number conversion
    #[from]
    IntConversion(TryFromIntError),

    /// Hex encoding/decoding error
    #[from]
    Hex(hex::Error),

    /// JSON serialization error
    #[from]
    Json(serde_json::Error),

    /// Node not synced
    #[display(doc_comments)]
    NodeNotSynced,

    /// UTXO index not enabled on node
    #[display(doc_comments)]
    UtxoIndexNotEnabled,

    /// Timeout waiting for response
    #[display(doc_comments)]
    Timeout,

    /// I/O error: {0}
    #[display(doc_comments)]
    Io(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

/// Result type for Tondi RPC operations
pub type Result<T> = std::result::Result<T, Error>;

