#[cfg(test)]
use std::cmp::min;

#[cfg(test)]
const ADLER_BASE: u32 = 65_521;
#[cfg(test)]
const ADLER_NMAX: usize = 5_552;
const CRC32_POLY: u32 = 0xedb8_8320;
#[cfg(test)]
const GF2_DIM: usize = 32;

/// Returns the initial Adler-32 checksum value.
#[cfg(test)]
pub(crate) const fn adler32_init() -> u32 {
    1
}

/// Updates an Adler-32 checksum with a byte slice.
#[cfg(test)]
pub(crate) fn adler32_update(sum: u32, bytes: &[u8]) -> u32 {
    let mut s1 = sum & 0xffff;
    let mut s2 = (sum >> 16) & 0xffff;
    let mut remaining = bytes;

    while !remaining.is_empty() {
        let chunk_len = min(remaining.len(), ADLER_NMAX);
        let (chunk, rest) = remaining.split_at(chunk_len);
        remaining = rest;

        for byte in chunk {
            s1 += u32::from(*byte);
            s2 += s1;
        }

        s1 %= ADLER_BASE;
        s2 %= ADLER_BASE;
    }

    (s2 << 16) | s1
}

/// Combines two Adler-32 sums as if their inputs were concatenated.
#[cfg(test)]
pub(crate) fn adler32_combine(sum1: u32, sum2: u32, len2: u32) -> u32 {
    if len2 == 0 {
        return sum1;
    }

    let rem = len2 % ADLER_BASE;
    let s1_1 = sum1 & 0xffff;
    let s2_1 = (sum1 >> 16) & 0xffff;
    let s1_2 = sum2 & 0xffff;
    let s2_2 = (sum2 >> 16) & 0xffff;

    let s1 = (s1_1 + s1_2 + ADLER_BASE - 1) % ADLER_BASE;
    let s2 = (s2_1 + s2_2 + rem * s1_1 + ADLER_BASE - rem) % ADLER_BASE;

    (s2 << 16) | s1
}

/// Returns the initial CRC-32 checksum value.
pub(crate) const fn crc32_init() -> u32 {
    0
}

/// Updates a CRC-32 checksum with a byte slice.
pub(crate) fn crc32_update(sum: u32, bytes: &[u8]) -> u32 {
    let mut crc = !sum;

    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ CRC32_POLY
            } else {
                crc >> 1
            };
        }
    }

    !crc
}

/// Combines two CRC-32 sums as if their inputs were concatenated.
#[cfg(test)]
pub(crate) fn crc32_combine(mut sum1: u32, sum2: u32, len2: u32) -> u32 {
    if len2 == 0 {
        return sum1;
    }

    let mut odd = [0u32; GF2_DIM];
    let mut even = [0u32; GF2_DIM];
    let mut row = 1u32;
    let mut len2 = len2;

    odd[0] = CRC32_POLY;
    for item in odd.iter_mut().skip(1) {
        *item = row;
        row <<= 1;
    }

    gf2_matrix_square(&mut even, &odd);
    gf2_matrix_square(&mut odd, &even);

    loop {
        gf2_matrix_square(&mut even, &odd);
        if len2 & 1 != 0 {
            sum1 = gf2_matrix_times(&even, sum1);
        }
        len2 >>= 1;
        if len2 == 0 {
            break;
        }

        gf2_matrix_square(&mut odd, &even);
        if len2 & 1 != 0 {
            sum1 = gf2_matrix_times(&odd, sum1);
        }
        len2 >>= 1;
        if len2 == 0 {
            break;
        }
    }

    sum1 ^ sum2
}

#[cfg(test)]
fn gf2_matrix_times(matrix: &[u32; GF2_DIM], mut vector: u32) -> u32 {
    let mut sum = 0;
    let mut index = 0;

    while vector != 0 {
        if vector & 1 != 0 {
            sum ^= matrix[index];
        }
        vector >>= 1;
        index += 1;
    }

    sum
}

#[cfg(test)]
fn gf2_matrix_square(square: &mut [u32; GF2_DIM], matrix: &[u32; GF2_DIM]) {
    for index in 0..GF2_DIM {
        square[index] = gf2_matrix_times(matrix, matrix[index]);
    }
}

#[cfg(test)]
#[path = "checksum_test.rs"]
#[cfg(test)]
mod checksum_test;
