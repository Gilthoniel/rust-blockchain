use serde::{Serialize, Deserialize};
use ring::digest;

const ID_SHORT_LEN: usize = 4;

#[derive(Serialize, Deserialize, Clone)]
pub struct BlockID([u8; 32]);

impl std::fmt::Display for BlockID {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for i in 0..ID_SHORT_LEN {
      write!(f, "{:02x?}", self.0[i])?; 
    }
    Ok(())
  }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Block {
    data: Vec<u8>,
}

impl Block {
    pub fn new(data: Vec<u8>) -> Self {
        Block {
            data,
        }
    }

    pub fn hash(&self) -> BlockID {
        let d = digest::digest(&digest::SHA256, self.data.as_slice());
        let mut id: [u8; 32] = Default::default();
        id[..].clone_from_slice(d.as_ref());

        BlockID(id)
    }
}
