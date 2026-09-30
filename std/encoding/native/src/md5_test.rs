use md5::{Digest, Md5};

use super::{digest, digest_bytes};

fn lcg_payload(len: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_u32;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

fn incremental_digest(chunks: impl IntoIterator<Item = Vec<u8>>) -> String {
    let mut state = Md5::new();
    for chunk in chunks {
        state.update(chunk);
    }
    let bytes = state.finalize();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Preserves the canonical RFC 1321 MD5 vectors through RustCrypto.
#[test]
fn md5_digest_matches_rfc_1321_vectors() {
    let cases = [
        ("", "d41d8cd98f00b204e9800998ecf8427e"),
        ("a", "0cc175b9c0f1b6a831c399e269772661"),
        ("abc", "900150983cd24fb0d6963f7d28e17f72"),
        ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
        (
            "abcdefghijklmnopqrstuvwxyz",
            "c3fcd3d76192e4007dfb496cca67e13b",
        ),
        (
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            "d174ab98d277d9f5a5611c2c9f419d9f",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(digest(input), expected, "input={input:?}");
    }
}

/// Locks every padding transition around the 64-byte compression boundary.
#[test]
fn md5_digest_preserves_padding_boundaries() {
    let cases = [
        (0, "d41d8cd98f00b204e9800998ecf8427e"),
        (1, "0cc175b9c0f1b6a831c399e269772661"),
        (55, "ef1772b6dff9a122358552954ad0df65"),
        (56, "3b0c8ac703f828b04c6c197006d17218"),
        (57, "652b906d60af96844ebd21b674f35e93"),
        (63, "b06521f39153d618550606be297466d5"),
        (64, "014842d480b571495a4a0363793f7367"),
        (65, "c743a45e0d2e6a95cb859adae0248435"),
        (127, "020406e1d05cdc2aa287641f7ae2cc39"),
        (128, "e510683b3f5ffe4093d021808bc6ff70"),
        (129, "b325dc1c6f5e7a2b7cf465b9feab7948"),
        (1_000, "cabe45dcc9ae5b66ba86600cca6b8ba8"),
    ];

    for (len, expected) in cases {
        assert_eq!(digest_bytes(&vec![b'a'; len]), expected, "len={len}");
    }
}

/// Proves every two-chunk split and safe cloned state match one-shot hashing.
#[test]
fn md5_incremental_splits_and_clones_match_one_shot_digest() {
    let data = lcg_payload(257);
    let expected = digest_bytes(&data);

    for split in 0..=data.len() {
        assert_eq!(
            incremental_digest([data[..split].to_vec(), data[split..].to_vec()]),
            expected,
            "split={split}",
        );
    }

    let mut original = Md5::new();
    original.update(b"abc");
    let copied = original.clone().finalize();
    original.update(b"def");
    assert_eq!(copied.as_slice(), Md5::digest(b"abc").as_slice());
    assert_eq!(
        original.finalize().as_slice(),
        Md5::digest(b"abcdef").as_slice()
    );
}

/// Exercises one-byte and prime-sized chunks over a larger deterministic body.
#[test]
fn md5_adversarial_chunk_sizes_match_one_shot_digest() {
    let data = lcg_payload(4_096);
    let expected = digest_bytes(&data);

    for chunk_size in [1, 3, 17, 55, 56, 63, 64, 65, 127, 257, 4_096] {
        assert_eq!(
            incremental_digest(data.chunks(chunk_size).map(<[u8]>::to_vec)),
            expected,
            "chunk_size={chunk_size}",
        );
    }
}
