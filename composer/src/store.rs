use crate::ComposerResult;
use serde::{Deserialize, Serialize};
use xmt::{
    error::Error,
    merge::MergeValue,
    traits::{StoreReadOps, StoreWriteOps},
    BranchKey, BranchNode, H256,
};

#[derive(Debug)]
pub struct SledStore<V> {
    db: sled::Tree,
    _v: std::marker::PhantomData<V>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
enum SerializableMergeValue {
    Value([u8; 32]),
    MergeWithZero {
        base_node: [u8; 32],
        zero_bits: [u8; 32],
        zero_count: u8,
    },
}

impl From<&MergeValue> for SerializableMergeValue {
    fn from(v: &MergeValue) -> Self {
        match v {
            MergeValue::Value(v) => Self::Value(<[u8; 32]>::from(*v)),
            MergeValue::MergeWithZero {
                base_node,
                zero_bits,
                zero_count,
            } => Self::MergeWithZero {
                base_node: <[u8; 32]>::from(*base_node),
                zero_bits: <[u8; 32]>::from(*zero_bits),
                zero_count: *zero_count,
            },
        }
    }
}

impl Into<MergeValue> for SerializableMergeValue {
    fn into(self) -> MergeValue {
        match self {
            Self::Value(v) => MergeValue::Value(H256::from(v)),
            Self::MergeWithZero {
                base_node,
                zero_bits,
                zero_count,
            } => MergeValue::MergeWithZero {
                base_node: H256::from(base_node),
                zero_bits: H256::from(zero_bits),
                zero_count,
            },
        }
    }
}

impl<V> SledStore<V> {
    pub fn open(album: &sled::Db, id: [u8; 32]) -> ComposerResult<Self> {
        Ok(Self {
            db: album.open_tree(id)?,
            _v: std::marker::PhantomData,
        })
    }

    pub fn save_root(&mut self, root: &H256) -> ComposerResult<()> {
        let v: [u8; 32] = (*root).into();
        self.db.insert(b"root", v.to_vec())?;
        Ok(())
    }

    pub fn read_root(&self) -> ComposerResult<Option<H256>> {
        Ok(self.db.get(b"root")?.map(|v| {
            let v: [u8; 32] = v.as_ref().try_into().expect("invalid root value");
            H256::from(v)
        }))
    }

    pub fn clear(&self) -> ComposerResult<()> {
        self.db.clear()?;
        Ok(())
    }

    fn leaf_key(key: &H256) -> Result<Vec<u8>, Error> {
        let b = <[u8; 32]>::from(*key);
        bincode::serialize(&(b"leaf", b)).map_err(|e| Error::Store(e.to_string()))
    }

    fn branch_key(key: &BranchKey) -> Result<Vec<u8>, Error> {
        let BranchKey { height, node_key } = *key;
        let b = <[u8; 32]>::from(node_key);
        bincode::serialize(&(b"bran", height, b)).map_err(|e| Error::Store(e.to_string()))
    }

    fn serialize_branch(branch: &BranchNode) -> Result<Vec<u8>, Error> {
        let BranchNode { left, right } = branch;
        let (left, right): (SerializableMergeValue, SerializableMergeValue) =
            (left.into(), right.into());
        bincode::serialize(&(left, right)).map_err(|e| Error::Store(e.to_string()))
    }

    fn deserialize_branch(v: &[u8]) -> Result<BranchNode, Error> {
        let (left, right): (SerializableMergeValue, SerializableMergeValue) =
            bincode::deserialize(v).map_err(|e| Error::Store(e.to_string()))?;
        Ok(BranchNode {
            left: left.into(),
            right: right.into(),
        })
    }
}

impl<V: From<[u8; 32]>> SledStore<V> {
    fn deserialize_val(v: &[u8]) -> Result<V, Error> {
        let v: [u8; 32] = v
            .try_into()
            .map_err(|_| Error::Store("invalid value".to_string()))?;
        Ok(V::from(v))
    }
}

impl<V: From<[u8; 32]>> StoreReadOps<V> for SledStore<V> {
    fn get_branch(&self, key: &BranchKey) -> Result<Option<BranchNode>, Error> {
        let v = self
            .db
            .get(Self::branch_key(key)?)
            .map_err(|e| Error::Store(e.to_string()))?;
        v.map(|v| Self::deserialize_branch(&v))
            .transpose()
            .map_err(|e| Error::Store(e.to_string()))
    }

    fn get_leaf(&self, key: &H256) -> Result<Option<V>, Error> {
        let v = self
            .db
            .get(Self::leaf_key(key)?)
            .map_err(|e| Error::Store(e.to_string()))?;
        v.map(|v| Self::deserialize_val(&v))
            .transpose()
            .map_err(|e| Error::Store(e.to_string()))
    }
}

impl<V: Into<[u8; 32]>> SledStore<V> {
    fn serialize_val(v: V) -> Result<Vec<u8>, Error> {
        let v = v.into();
        Ok(v.to_vec())
    }
}

impl<V: Into<[u8; 32]>> StoreWriteOps<V> for SledStore<V> {
    fn insert_branch(&mut self, key: BranchKey, branch: BranchNode) -> Result<(), Error> {
        self.db
            .insert(Self::branch_key(&key)?, Self::serialize_branch(&branch)?)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    fn insert_leaf(&mut self, key: H256, leaf: V) -> Result<(), Error> {
        self.db
            .insert(Self::leaf_key(&key)?, Self::serialize_val(leaf)?)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    fn remove_branch(&mut self, key: &BranchKey) -> Result<(), Error> {
        self.db
            .remove(Self::branch_key(key)?)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    fn remove_leaf(&mut self, key: &H256) -> Result<(), Error> {
        self.db
            .remove(Self::leaf_key(key)?)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}
