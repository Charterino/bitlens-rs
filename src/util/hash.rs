use sha2::Digest;

// Bitcoin's 160 bit hash: sha256+ripemd-160
pub fn hash160(input: &[u8]) -> [u8; 20] {
    let shad = sha2::Sha256::digest(input);
    let riped = ripemd::Ripemd160::digest(shad);
    riped.into()
}
