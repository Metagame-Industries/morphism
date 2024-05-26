#![feature(const_trait_impl)]
mod vrf;

use wasm_bindgen::prelude::*;

const U32: usize = 32;
const U64: usize = 64;
const MAX_FRAMES: u32 = 1200;

#[wasm_bindgen]
pub struct FrameIndexProof {
    frame_indices: Vec<u32>,
    proof: vrf::VrfProof,
    hash: [u8; 32],
}

#[wasm_bindgen]
impl FrameIndexProof {
    #[wasm_bindgen(getter)]
    pub fn frame_indices(&self) -> js_sys::Uint32Array {
        js_sys::Uint32Array::from(&self.frame_indices[..])
    }

    #[wasm_bindgen(getter)]
    pub fn proof(&self) -> String {
        format!("0x{}", hex::encode(&self.proof.to_bytes().to_vec()))
    }

    #[wasm_bindgen(getter)]
    pub fn hash(&self) -> String {
        format!("0x{}", hex::encode(&self.hash))
    }
}

#[wasm_bindgen]
pub fn prove_from_witness(seed: &str, sk: &str) -> FrameIndexProof {
    let seed = from_hex_str::<U32>(seed).unwrap();
    let sk = from_hex_str::<U32>(sk).unwrap();
    let sk = vrf::VrfSk::from_bytes(&sk).unwrap();
    let (hash, proof) = vrf::prove(&seed, &sk);
    let indices = vec![
        u32::from_be_bytes(hash[0..4].try_into().unwrap()) % MAX_FRAMES,
        u32::from_be_bytes(hash[4..8].try_into().unwrap()) % MAX_FRAMES,
        u32::from_be_bytes(hash[8..12].try_into().unwrap()) % MAX_FRAMES,
    ];
    FrameIndexProof {
        frame_indices: indices,
        proof,
        hash,
    }
}

fn from_hex_str<const T: usize>(s: &str) -> anyhow::Result<[u8; T]> {
    let hex = s.trim_start_matches("0x");
    let mut r = [0u8; T];
    hex::decode_to_slice(hex, &mut r)
        .map_err(|_| anyhow::anyhow!("invalid hex string"))
        .map(|_| r)
}
