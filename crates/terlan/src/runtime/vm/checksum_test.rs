use super::{
    adler32_combine, adler32_init, adler32_update, crc32_combine, crc32_init, crc32_update,
};

fn adler32_digest(data: &[u8]) -> u32 {
    adler32_update(adler32_init(), data)
}

fn crc32_digest(data: &[u8]) -> u32 {
    crc32_update(crc32_init(), data)
}

fn repeated(byte: u8, len: usize) -> Vec<u8> {
    vec![byte; len]
}

fn byte_range(len: usize) -> Vec<u8> {
    (0..len).map(|value| value as u8).collect()
}

fn assert_every_split_matches(
    limit: usize,
    digest: fn(&[u8]) -> u32,
    combine: fn(u32, u32, u32) -> u32,
) {
    let data = byte_range(limit);
    let expected = digest(&data);

    for split in 0..=data.len() {
        let left = digest(&data[..split]);
        let right = digest(&data[split..]);
        assert_eq!(combine(left, right, (data.len() - split) as u32), expected);
    }
}

fn assert_three_way_combine_matches(
    limit: usize,
    digest: fn(&[u8]) -> u32,
    combine: fn(u32, u32, u32) -> u32,
) {
    let data = byte_range(limit);
    let expected = digest(&data);

    for first_split in 0..=data.len() {
        for second_split in first_split..=data.len() {
            let first = digest(&data[..first_split]);
            let second = digest(&data[first_split..second_split]);
            let third = digest(&data[second_split..]);
            let second_and_third = combine(second, third, (data.len() - second_split) as u32);

            assert_eq!(
                combine(first, second_and_third, (data.len() - first_split) as u32,),
                expected,
                "three-way combine failed at splits {first_split} and {second_split}",
            );
        }
    }
}

#[test]
fn adler32_reference_vectors_match_zlib() {
    let cases = [
        (b"".as_slice(), 0x0000_0001),
        (b"a".as_slice(), 0x0062_0062),
        (b"abc".as_slice(), 0x024d_0127),
        (b"abcdefghijklmnopqrstuvwxyz".as_slice(), 0x9086_0b20),
    ];

    for (input, expected) in cases {
        assert_eq!(adler32_digest(input), expected);
    }
    assert_eq!(adler32_digest(&repeated(0, 5_552)), 0x15b0_0001);
    assert_eq!(adler32_digest(&byte_range(256)), 0xadf6_7f81);
    assert_eq!(adler32_digest(&repeated(b'a', 1_000)), 0xf9d8_7af8);
}

#[test]
fn adler32_incremental_updates_match_one_shot() {
    let data = byte_range(16_384);
    let mut sum = adler32_init();

    for chunk in data.chunks(17) {
        sum = adler32_update(sum, chunk);
    }

    assert_eq!(sum, adler32_digest(&data));
}

#[test]
fn adler32_combined_updates_match_every_split() {
    assert_every_split_matches(512, adler32_digest, adler32_combine);
}

#[test]
fn adler32_three_way_combines_match_one_shot() {
    assert_three_way_combine_matches(64, adler32_digest, adler32_combine);
}

#[test]
fn adler32_combine_reference_vectors_match_zlib() {
    let a55 = repeated(b'a', 55);
    let a57 = repeated(b'a', 57);
    let range256 = byte_range(256);
    let a1000 = repeated(b'a', 1_000);

    let cases = [
        (
            b"a".as_slice(),
            b"bc".as_slice(),
            0x0062_0062,
            0x0129_00c6,
            0x024d_0127,
        ),
        (
            a55.as_slice(),
            a57.as_slice(),
            0x47d9_14d8,
            0x72ac_159a,
            0x5eaf_2a71,
        ),
        (
            range256.as_slice(),
            a1000.as_slice(),
            0xadf6_7f81,
            0xf9d8_7af8,
            0xd10b_fa78,
        ),
    ];

    for (left, right, left_sum, right_sum, combined) in cases {
        assert_eq!(adler32_digest(left), left_sum);
        assert_eq!(adler32_digest(right), right_sum);
        assert_eq!(
            adler32_combine(left_sum, right_sum, right.len() as u32),
            combined
        );
    }
}

#[test]
fn crc32_reference_vectors_match_zlib() {
    let cases = [
        (b"".as_slice(), 0x0000_0000),
        (b"a".as_slice(), 0xe8b7_be43),
        (b"abc".as_slice(), 0x3524_41c2),
        (b"abcdefghijklmnopqrstuvwxyz".as_slice(), 0x4c27_50bd),
    ];

    for (input, expected) in cases {
        assert_eq!(crc32_digest(input), expected);
    }
    assert_eq!(crc32_digest(&repeated(0, 5_552)), 0x2c4b_3908);
    assert_eq!(crc32_digest(&byte_range(256)), 0x2905_8c73);
    assert_eq!(crc32_digest(&repeated(b'a', 1_000)), 0x9a38_da03);
}

#[test]
fn crc32_incremental_updates_match_one_shot() {
    let data = byte_range(16_384);
    let mut sum = crc32_init();

    for chunk in data.chunks(17) {
        sum = crc32_update(sum, chunk);
    }

    assert_eq!(sum, crc32_digest(&data));
}

#[test]
fn crc32_combined_updates_match_every_split() {
    assert_every_split_matches(512, crc32_digest, crc32_combine);
}

#[test]
fn crc32_three_way_combines_match_one_shot() {
    assert_three_way_combine_matches(64, crc32_digest, crc32_combine);
}

#[test]
fn crc32_combine_reference_vectors_match_zlib() {
    let a55 = repeated(b'a', 55);
    let a57 = repeated(b'a', 57);
    let range256 = byte_range(256);
    let a1000 = repeated(b'a', 1_000);

    let cases = [
        (
            b"a".as_slice(),
            b"bc".as_slice(),
            0xe8b7_be43,
            0xc2a9_2b38,
            0x3524_41c2,
        ),
        (
            a55.as_slice(),
            a57.as_slice(),
            0xaadf_e34e,
            0x5073_6241,
            0x0c6e_9fd3,
        ),
        (
            range256.as_slice(),
            a1000.as_slice(),
            0x2905_8c73,
            0x9a38_da03,
            0xb4bd_c9e9,
        ),
    ];

    for (left, right, left_sum, right_sum, combined) in cases {
        assert_eq!(crc32_digest(left), left_sum);
        assert_eq!(crc32_digest(right), right_sum);
        assert_eq!(
            crc32_combine(left_sum, right_sum, right.len() as u32),
            combined
        );
    }
}

#[test]
fn zero_length_combines_preserve_left_sum() {
    assert_eq!(
        adler32_combine(0x1234_5678, adler32_digest(b"abc"), 0),
        0x1234_5678
    );
    assert_eq!(
        crc32_combine(0x1234_5678, crc32_digest(b"abc"), 0),
        0x1234_5678
    );
}
