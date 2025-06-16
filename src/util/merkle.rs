use sha2::{Digest, Sha256};

pub fn compute_merkle_root(mut hashes: Vec<[u8; 32]>) -> ([u8; 32], bool) {
    let mut mutation = false;
    while hashes.len() > 1 {
        for i in 0..hashes.len() - 1 {
            if hashes[i] == hashes[i + 1] {
                mutation = true;
            }
        }
        if hashes.len() & 1 == 1 {
            hashes.push(*hashes.last().unwrap());
        }
        let p = hashes.as_mut_ptr();
        sha256_d64(p, (hashes.len() / 2) as isize);
        hashes.truncate(hashes.len() / 2);
    }

    if hashes.is_empty() {
        ([0u8; 32], false)
    } else {
        (hashes[0], mutation)
    }
}

fn sha256_d64(data: *mut [u8; 32], blocks: isize) {
    unsafe {
        for i in 0..blocks {
            let block: *mut [u8; 64] = std::mem::transmute(data.offset(i * 2));
            let shad: [u8; 32] = Sha256::digest(Sha256::digest(*block)).into();
            *data.offset(i) = shad;
        }
    }
}
