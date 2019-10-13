use ring::digest;
use rand::prelude::{StdRng, SeedableRng};
use serde::{Deserialize, Serialize};

const ID_SHORT_LEN: usize = 4;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockID([u8; 32]);

impl std::fmt::Display for BlockID {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for i in 0..ID_SHORT_LEN {
      write!(f, "{:02x?}", self.0[i])?;
    }
    Ok(())
  }
}

impl PartialEq for BlockID {
  fn eq(&self, other: &Self) -> bool {
    self.0 == other.0
  }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Seed([u8; 32]);

impl PartialEq for &Seed {
  fn eq(&self, other: &Self) -> bool {
    self.0 == other.0
  }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
  seed: Seed,
  ack: Vec<bool>,
  data: Vec<u8>,
}

impl Block {
  pub fn new(data: Vec<u8>) -> Self {
    Block {
      seed: Seed(Default::default()), // TODO: random
      ack: Vec::new(),
      data,
    }
  }

  pub fn get_seed(&self) -> &Seed {
    &self.seed
  }

  pub fn get_rng(&self) -> StdRng {
    StdRng::from_seed(self.seed.0)
  }

  pub fn get_ack(&self) -> usize {
    self.ack.iter().filter(|&v| *v).count()
  }

  pub fn incr_ack(&mut self, index: usize) {
    if self.ack.len() < index + 1 {
      self.ack.resize(index + 1, false);
    }

    self.ack[index] = true;
  }

  pub fn verify(&self) -> bool {
    return true;
  }

  pub fn hash(&self) -> BlockID {
    let mut c = digest::Context::new(&digest::SHA256);
    c.update(&self.data);
    c.update(&self.seed.0);

    let mut id: [u8; digest::SHA256_OUTPUT_LEN] = Default::default();
    id[..].clone_from_slice(c.finish().as_ref());

    BlockID(id)
  }

  pub fn next(&self, data: Vec<u8>) -> Self {
    let d = digest::digest(&digest::SHA256, &self.seed.0[..]);
    let mut id: [u8; digest::SHA256_OUTPUT_LEN] = Default::default();
    id[..].clone_from_slice(d.as_ref());

    Block {
      seed: Seed(id),
      ack: Vec::new(),
      data,
    }
  }
}
