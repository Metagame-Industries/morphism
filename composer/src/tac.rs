use crate::{store::SledStore, ComposerError, ComposerResult};
use ffmpeg::{
    codec::context::Context, format::input, media::Type, util::frame::video::Video as VideoFrame,
};
use std::path::{Path, PathBuf};
use xmt::{blake2b::Blake2bHasher, SparseMerkleTree, H256};

type MerkleTree = SparseMerkleTree<Blake2bHasher, H256, SledStore<H256>>;

#[derive(Debug, Clone)]
pub struct Album {
    pub name: String,
    pub path: PathBuf,
    db: sled::Db,
}

impl Album {
    pub fn new(home: &Path, name: &str) -> Result<Self, ComposerError> {
        let path = home.join(sha256::digest(name));
        (!path.exists())
            .then(|| ())
            .ok_or(ComposerError::AlbumExists)?;
        std::fs::create_dir(&path)?;
        let db = sled::open(&path)?;
        Ok(Self {
            name: name.to_string(),
            path,
            db,
        })
    }

    pub fn find(home: &Path, name: &str) -> Result<Self, ComposerError> {
        let path = home.join(sha256::digest(name));
        path.exists()
            .then(|| Self {
                name: name.to_string(),
                path: path.clone(),
                db: sled::open(&path).unwrap(),
            })
            .ok_or(ComposerError::AlbumNotFound)
    }

    pub fn find_episode(&self, id: [u8; 32]) -> Option<MerkleTree> {
        let store = SledStore::open(&self.db, id).ok()?;
        let root = store.read_root().ok()??;
        Some(MerkleTree::new(root, store))
    }

    pub fn get_proof_of_frames(&self, id: [u8; 32], frames: &[u64]) -> ComposerResult<Vec<u8>> {
        let store = SledStore::open(&self.db, id)?;
        let smt = MerkleTree::new(
            store.read_root()?.ok_or(ComposerError::ResourceNotFound)?,
            store,
        );
        let keys = frames
            .iter()
            .map(|f| H256::from(Self::digest(&f.to_be_bytes())))
            .collect::<Vec<_>>();
        let proof = smt.merkle_proof(keys.clone())?.compile(keys)?;
        log::debug!(
            "get proof of frames {:?} => \n0x{}",
            frames,
            hex::encode(&proof.0)
        );
        Ok(proof.0)
    }

    pub fn append<P>(&self, media: &P, overwrite: bool) -> Result<H256, ComposerError>
    where
        P: AsRef<Path>,
    {
        ffmpeg::init()?;
        let id = sha256::try_digest(media.as_ref())
            .map(|s| hex::decode(s).expect("qed"))
            .map_err(|e| ComposerError::File(e))?
            .try_into()
            .expect("qed");
        let mut smt = self.new_empty_tree(id, overwrite)?;
        let frames = Self::dump_frames(media, |f| Ok(Self::digest(f)))?;
        Self::save_frames(frames, &mut smt)
    }

    fn new_empty_tree(&self, id: [u8; 32], force: bool) -> Result<MerkleTree, ComposerError> {
        let store = SledStore::open(&self.db, id)?;
        match store.read_root()? {
            None => Ok(MerkleTree::new(Default::default(), store)),
            Some(_) if force => {
                store.clear()?;
                Ok(MerkleTree::new(Default::default(), store))
            }
            _ => Err(ComposerError::MediaExists),
        }
    }

    fn save_frames(frames: Vec<[u8; 32]>, smt: &mut MerkleTree) -> Result<H256, ComposerError> {
        let v = frames
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let i = (i as u64).to_be_bytes();
                (H256::from(Self::digest(&i)), H256::from(f))
            })
            .collect::<Vec<_>>();
        let root = smt.update_all(v)?.clone();
        log::debug!(
            "frames saved, root: 0x{}",
            hex::encode(<[u8; 32]>::from(root.clone()))
        );
        smt.store_mut().save_root(&root)?;
        Ok(root)
    }

    fn dump_frames<F, R, P>(file: &P, f: F) -> Result<Vec<R>, ComposerError>
    where
        P: AsRef<std::path::Path>,
        F: Fn(&[u8]) -> Result<R, ffmpeg::Error>,
    {
        let mut frames = vec![];
        if let Ok(mut ictx) = input(file) {
            let input = ictx
                .streams()
                .best(Type::Video)
                .ok_or(ffmpeg::Error::StreamNotFound)?;
            let video_stream_index = input.index();
            log::debug!(
                "duration = {}, frames = {}",
                input.duration(),
                input.frames()
            );
            let context_decoder = Context::from_parameters(input.parameters())?;
            if let Ok(mut decoder) = context_decoder.decoder().video() {
                for (stream, packet) in ictx.packets() {
                    if stream.index() == video_stream_index {
                        decoder.send_packet(&packet)?;
                        let mut decoded = VideoFrame::empty();
                        while decoder.receive_frame(&mut decoded).is_ok() {
                            let r = f(decoded.data(0))?;
                            frames.push(r);
                        }
                    }
                }
                decoder.send_eof()?;
            }
        }
        if frames.is_empty() {
            return Err(ComposerError::Media(ffmpeg::Error::StreamNotFound));
        }
        Ok(frames)
    }

    pub(crate) fn digest(c: &[u8]) -> [u8; 32] {
        use blake2::digest::{consts::U32, Digest};
        let mut hasher = blake2::Blake2b::<U32>::new();
        hasher.update(c);
        hasher.finalize().into()
    }
}
