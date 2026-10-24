use std::sync::OnceLock;

use hmac::{Hmac, Mac};
use num_bigint::BigUint;
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::{Digest, Sha256};

const N_HEX: &str = concat!(
    "AC6BDB41324A9A9BF166DE5E1389582FAF72B6651987EE07FC3192943DB56050",
    "A37329CBB4A099ED8193E0757767A13DD52312AB4B03310DCD7F48A9DA04FD50",
    "E8083969EDB767B0CF6095179A163AB3661A05FBD5FAAAE82918A9962F0B93B8",
    "55F97993EC975EEAA80D740ADBF4FF747359D041D5C33EA71D281E446B14773B",
    "CA97B43A23FB801676BD207A436C6481F1D2B9078717461A5B9D32E688F87748",
    "544523B524B0D57D5EA77A2775D2ECFA032CFBDBF52FB3786160279004E57AE6",
    "AF874E7303CE53299CCC041C7BC308D82A5698F3A8D0C38271AE35F8E9DBFBB6",
    "94B5C803D89F7AE435DE236D525F54759B65E372FCD68EF20FA7111F9E4AFF73",
);

// Length of the SRP-6a 2048-bit modulus.
pub const N_BYTE_LENGTH: usize = 256;

pub const G: u32 = 2;

static N: OnceLock<BigUint> = OnceLock::new();
static N_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
static K: OnceLock<BigUint> = OnceLock::new();
static H_N_XOR_H_G: OnceLock<Vec<u8>> = OnceLock::new();

pub fn n() -> &'static BigUint {
    N.get_or_init(|| BigUint::parse_bytes(N_HEX.as_bytes(), 16).expect("invalid N_HEX"))
}

pub fn n_bytes() -> &'static [u8] {
    N_BYTES.get_or_init(|| fixed_length_bytes(n(), N_BYTE_LENGTH))
}

pub fn k() -> &'static BigUint {
    K.get_or_init(|| {
        let g_padded = fixed_length_bytes(&BigUint::from(G), N_BYTE_LENGTH);
        BigUint::from_bytes_be(&sha256_concat(&[n_bytes(), &g_padded]))
    })
}

pub fn h_n_xor_h_g() -> &'static [u8] {
    H_N_XOR_H_G.get_or_init(|| {
        let g_padded = fixed_length_bytes(&BigUint::from(G), N_BYTE_LENGTH);
        xor(&sha256_concat(&[n_bytes()]), &sha256_concat(&[&g_padded]))
    })
}

pub fn int_to_bytes(value: &BigUint) -> Vec<u8> {
    value.to_bytes_be()
}

pub fn bytes_to_int(bytes: &[u8]) -> BigUint {
    BigUint::from_bytes_be(bytes)
}

fn fixed_length_bytes(value: &BigUint, length: usize) -> Vec<u8> {
    let raw = value.to_bytes_be();
    if raw.len() >= length {
        return raw[raw.len() - length..].to_vec();
    }
    let mut out = vec![0u8; length];
    out[length - raw.len()..].copy_from_slice(&raw);
    out
}

pub fn pad_to_n(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() >= N_BYTE_LENGTH {
        return bytes.to_vec();
    }
    let mut out = vec![0u8; N_BYTE_LENGTH];
    out[N_BYTE_LENGTH - bytes.len()..].copy_from_slice(bytes);
    out
}

pub fn random_private_key() -> BigUint {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    let mut value = BigUint::from_bytes_be(&buf);
    value.set_bit(255, true);
    value
}

pub fn concat(parts: &[&[u8]]) -> Vec<u8> {
    let total: usize = parts.iter().map(|p| p.len()).sum();
    let mut out = Vec::with_capacity(total);
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

pub fn xor(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
}

pub fn sha256_concat(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

pub fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, dk_len: usize) -> Vec<u8> {
    let mut out = vec![0u8; dk_len];

    let iterations = iterations.max(1);
    pbkdf2_hmac::<Sha256>(password, salt, iterations, &mut out);
    out
}

pub fn derive_s2k(password: &str, salt: &[u8], iterations: u32) -> Vec<u8> {
    let pass_hash = sha256_concat(&[password.as_bytes()]);
    pbkdf2_hmac_sha256(&pass_hash, salt, iterations, 32)
}

pub fn derive_s2k_fo(password: &str, salt: &[u8], iterations: u32) -> Vec<u8> {
    let pass_hash = sha256_concat(&[password.as_bytes()]);
    let pass_hash_hex = hex_lower(&pass_hash);
    pbkdf2_hmac_sha256(pass_hash_hex.as_bytes(), salt, iterations, 32)
}

pub fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn compute_x(salt: &[u8], derived_password: &[u8]) -> BigUint {
    let inner = sha256_concat(&[b":", derived_password]);
    bytes_to_int(&sha256_concat(&[salt, &inner]))
}

pub fn compute_u(a_bytes: &[u8], b_bytes: &[u8]) -> BigUint {
    let a_padded = pad_to_n(a_bytes);
    let b_padded = pad_to_n(b_bytes);
    bytes_to_int(&sha256_concat(&[&a_padded, &b_padded]))
}

fn mod_sub(a: &BigUint, b: &BigUint, m: &BigUint) -> BigUint {
    ((a % m) + m - (b % m)) % m
}

pub fn compute_s(private_key_a: &BigUint, _a_bytes: &[u8], b_bytes: &[u8], x: &BigUint, u: &BigUint) -> Vec<u8> {
    let n = n();
    let b = bytes_to_int(b_bytes);
    let gx = BigUint::from(G).modpow(x, n);
    let base = mod_sub(&b, &(k() * gx), n);
    let exponent = (private_key_a + u * x) % n;
    let s = base.modpow(&exponent, n);
    sha256_concat(&[&int_to_bytes(&s)]).to_vec()
}

// Client proof M1 binds the user, salt, public keys, and session key.
pub fn compute_m1(username: &str, salt: &[u8], a_bytes: &[u8], b_bytes: &[u8], k_bytes: &[u8]) -> Vec<u8> {
    let hashed_username = sha256_concat(&[username.as_bytes()]);
    sha256_concat(&[h_n_xor_h_g(), &hashed_username, salt, a_bytes, b_bytes, k_bytes]).to_vec()
}

pub fn compute_m2(a_bytes: &[u8], m1_bytes: &[u8], k_bytes: &[u8]) -> Vec<u8> {
    sha256_concat(&[a_bytes, m1_bytes, k_bytes]).to_vec()
}

pub fn public_key_a(private_key_a: &BigUint) -> Vec<u8> {
    int_to_bytes(&BigUint::from(G).modpow(private_key_a, n()))
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n_has_expected_length_and_value() {
        assert_eq!(n_bytes().len(), 256);
        assert_eq!(n_bytes()[0], 0xAC);
        assert_eq!(*n(), BigUint::parse_bytes(N_HEX.as_bytes(), 16).unwrap());
    }

    #[test]
    fn k_and_hnxorhg_match_rfc5054_derivation() {
        // k = SHA256(N || g_padded)
        let g_padded = fixed_length_bytes(&BigUint::from(2u32), 256);
        let expected_k = BigUint::from_bytes_be(&sha256_concat(&[n_bytes(), &g_padded]));
        assert_eq!(*k(), expected_k);

        let expected_xor = xor(&sha256_concat(&[n_bytes()]), &sha256_concat(&[&g_padded]));
        assert_eq!(h_n_xor_h_g(), expected_xor.as_slice());
    }

    #[test]
    fn int_to_bytes_zero_is_single_zero_byte() {
        assert_eq!(int_to_bytes(&BigUint::from(0u32)), vec![0u8]);
    }

    #[test]
    fn pad_to_n_right_aligns() {
        let padded = pad_to_n(&[1, 2, 3]);
        assert_eq!(padded.len(), 256);
        assert_eq!(&padded[253..], &[1, 2, 3]);
    }

    #[test]
    fn password_derivation_is_deterministic_and_differs_by_protocol() {
        let salt = b"0123456789abcdef";
        let s2k = derive_s2k("Password123", salt, 1000);
        let s2k_fo = derive_s2k_fo("Password123", salt, 1000);
        assert_eq!(s2k.len(), 32);
        assert_eq!(s2k_fo.len(), 32);
        assert_ne!(s2k, s2k_fo);
        assert_eq!(s2k, derive_s2k("Password123", salt, 1000));
    }

    #[test]
    fn pbkdf2_matches_rfc6070_vector() {
        let out = pbkdf2_hmac_sha256(b"password", b"salt", 1, 32);
        assert_eq!(hex_lower(&out), "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b");
    }

    #[test]
    fn private_key_has_high_bit_set() {
        let a = random_private_key();
        assert!(a.bit(255));
        assert!(a < (BigUint::from(1u32) << 256));
    }

    #[test]
    fn public_key_is_in_range() {
        let a = random_private_key();
        let a_pub = public_key_a(&a);
        assert!(bytes_to_int(&a_pub) < *n());
    }
}
