use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::x25519;

#[test]
fn rfc7748_x25519_vector_is_preserved() {
    let scalar = decode_array("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4");
    let point = decode_array("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c");
    let expected = decode_array("c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");

    assert_eq!(x25519(scalar, point), expected);
}

#[test]
fn rfc5869_hkdf_sha256_case_one_is_preserved() {
    let input_key_material = [0x0b_u8; 22];
    let salt = decode_array::<13>("000102030405060708090a0b0c");
    let info = decode_array::<10>("f0f1f2f3f4f5f6f7f8f9");
    let expected_pseudorandom_key =
        decode_array::<32>("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
    let expected_output = decode_array::<42>(
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
    );
    let (pseudorandom_key, hkdf) = Hkdf::<Sha256>::extract(Some(&salt), &input_key_material);
    let mut output = [0_u8; 42];
    hkdf.expand(&info, &mut output)
        .expect("RFC 5869 output length is valid");

    assert_eq!(pseudorandom_key.as_slice(), expected_pseudorandom_key);
    assert_eq!(output, expected_output);
}

#[test]
fn rfc8439_chacha20_poly1305_vector_is_preserved() {
    let key =
        decode_array::<32>("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
    let nonce = decode_array::<12>("070000004041424344454647");
    let aad = decode_array::<12>("50515253c0c1c2c3c4c5c6c7");
    let plaintext = b"Ladies and Gentlemen of the class of '99: \
        If I could offer you only one tip for the future, sunscreen would be it.";
    let expected = decode_array::<130>(concat!(
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6",
        "3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36",
        "92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc",
        "3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691",
    ));
    let cipher = ChaCha20Poly1305::new_from_slice(&key).expect("RFC 8439 key length is valid");
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .expect("RFC 8439 encryption succeeds");

    assert_eq!(ciphertext.as_slice(), expected);
}

fn decode_array<const LENGTH: usize>(encoded: &str) -> [u8; LENGTH] {
    assert_eq!(encoded.len(), LENGTH * 2);
    let mut decoded = [0_u8; LENGTH];
    for (index, digits) in encoded.as_bytes().chunks_exact(2).enumerate() {
        let digits = std::str::from_utf8(digits).expect("test vector is ASCII");
        decoded[index] = u8::from_str_radix(digits, 16).expect("test vector is hexadecimal");
    }
    decoded
}
