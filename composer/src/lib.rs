mod store;
pub mod tac;

pub use tac::Album;

use thiserror::Error;

pub type ComposerResult<T> = Result<T, ComposerError>;

#[derive(Debug, Error)]
pub enum ComposerError {
    #[error("media codec error: {0}")]
    Media(#[from] ffmpeg::Error),
    #[error("sparse merkle tree error: {0}")]
    MerkleTree(#[from] xmt::error::Error),
    #[error("store error: {0}")]
    Store(#[from] sled::Error),
    #[error("serde bincode error: {0}")]
    Serde(#[from] bincode::Error),
    #[error("media file error: {0}")]
    File(#[from] std::io::Error),
    #[error("media already appended")]
    MediaExists,
    #[error("album already exists")]
    AlbumExists,
    #[error("album not found")]
    AlbumNotFound,
    #[error("resource not found")]
    ResourceNotFound,
}
