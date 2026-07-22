use super::{decode_integer, VmBitString, VmBitStringEndian, VmBitStringError};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

#[test]
fn bitstring_canonicalizes_storage_and_discards_unrepresented_bytes() {
    let bits = VmBitString::from_bytes([0xff, 0xff], 3).expect("valid bit length");

    assert_eq!(bits.bit_len(), 3);
    assert_eq!(bits.byte_len(), 1);
    assert_eq!(bits.packed_bytes(), &[0xe0]);
    let canonical = VmBitString::from_bytes([0xe0], 3).expect("canonical bitstring");
    assert_eq!(bits, canonical);

    let mut original_hash = DefaultHasher::new();
    bits.hash(&mut original_hash);
    let mut canonical_hash = DefaultHasher::new();
    canonical.hash(&mut canonical_hash);
    assert_eq!(original_hash.finish(), canonical_hash.finish());
}

#[test]
fn bitstring_clones_preserve_values_across_derived_slices() {
    let original =
        VmBitString::from_bytes([0b1010_1010, 0b1100_0000], 10).expect("ten-bit bitstring");
    let retained = original.clone();
    let derived = retained.slice(1, 8).expect("derived slice");

    assert_eq!(original, retained);
    assert_eq!(original.bit_len(), 10);
    assert_eq!(original.packed_bytes(), &[0b1010_1010, 0b1100_0000]);
    assert_eq!(derived.bit_len(), 8);
    assert_eq!(derived.packed_bytes(), &[0b0101_0101]);
}

#[test]
fn bitstring_slices_aligned_and_unaligned_ranges_in_network_order() {
    let bits = VmBitString::from_bytes([0b1010_1010, 0b1100_0000], 10).expect("ten-bit bitstring");

    assert_eq!(bits.bit_at(0), Some(true));
    assert_eq!(bits.bit_at(1), Some(false));
    assert_eq!(bits.bit_at(9), Some(true));
    assert_eq!(bits.bit_at(10), None);

    let middle = bits.slice(2, 6).expect("unaligned slice");
    assert_eq!(middle.packed_bytes(), &[0b1010_1000]);
    assert_eq!(middle.bit_len(), 6);

    let aligned = bits.slice(0, 8).expect("aligned slice");
    assert_eq!(aligned.to_bytes().expect("aligned bytes").as_ref(), &[0xaa]);
}

#[test]
fn bitstring_match_tail_preserves_aligned_dynamic_and_zero_suffixes() {
    let two_bytes = VmBitString::from_bytes([1, 2], 16).expect("two-byte bitstring");
    assert_eq!(
        decode_integer(
            two_bytes.packed_bytes(),
            0,
            16,
            false,
            VmBitStringEndian::Big,
        ),
        Ok(258)
    );
    assert_eq!(
        two_bytes
            .slice(16, 0)
            .expect("zero tail")
            .to_bytes()
            .expect("zero tail is aligned")
            .as_ref(),
        &[] as &[u8]
    );

    let expected_tail = (1_u8..=127).collect::<Vec<_>>();
    let mut storage = vec![137, 19];
    storage.extend_from_slice(&expected_tail);
    let bits = VmBitString::from_bytes(&storage, storage.len() * 8).expect("aligned bitstring");
    assert_eq!(
        decode_integer(bits.packed_bytes(), 0, 16, false, VmBitStringEndian::Big,),
        Ok(35_091)
    );
    assert_eq!(
        bits.slice(16, expected_tail.len() * 8)
            .expect("aligned tail")
            .to_bytes()
            .expect("aligned tail bytes")
            .as_ref(),
        expected_tail
    );

    let dynamic = VmBitString::from_bytes([73, 0, 1, 2], 32).expect("dynamic bitstring");
    assert_eq!(
        decode_integer(dynamic.packed_bytes(), 0, 8, false, VmBitStringEndian::Big,),
        Ok(73)
    );
    assert_eq!(
        dynamic
            .slice(8, 24)
            .expect("dynamic tail")
            .to_bytes()
            .expect("dynamic tail bytes")
            .as_ref(),
        &[0, 1, 2]
    );
}

#[test]
fn bitstring_match_tail_rejects_unaligned_and_wrong_large_suffixes() {
    let one_byte = VmBitString::from_bytes([42], 8).expect("one-byte bitstring");
    let seven_bit_tail = one_byte.slice(1, 7).expect("seven-bit tail");
    assert_eq!(
        seven_bit_tail.to_bytes(),
        Err(VmBitStringError::NotByteAligned { bit_len: 7 })
    );

    let two_bytes = VmBitString::from_bytes([42, 33], 16).expect("two-byte bitstring");
    let one_bit_tail = two_bytes.slice(15, 1).expect("one-bit tail");
    assert_eq!(
        one_bit_tail.to_bytes(),
        Err(VmBitStringError::NotByteAligned { bit_len: 1 })
    );
    assert_eq!(
        two_bytes.require_exact_bit_len(8),
        Err(VmBitStringError::BitLengthMismatch {
            expected: 8,
            actual: 16,
        })
    );

    const LARGE_TAIL_BITS: usize = 0x1001;
    let total_bits = 8 + LARGE_TAIL_BITS;
    let mut storage = vec![0; total_bits.div_ceil(8)];
    storage[0] = 42;
    let large = VmBitString::from_bytes(storage, total_bits).expect("large-tail bitstring");
    assert_eq!(
        decode_integer(large.packed_bytes(), 0, 8, false, VmBitStringEndian::Big,),
        Ok(42)
    );
    assert_eq!(
        large
            .slice(8, LARGE_TAIL_BITS)
            .expect("large tail")
            .bit_len(),
        LARGE_TAIL_BITS
    );

    let short = VmBitString::from_bytes([0; 14], 108).expect("short bitstring");
    assert_eq!(
        short.require_exact_bit_len(total_bits),
        Err(VmBitStringError::BitLengthMismatch {
            expected: total_bits,
            actual: 108,
        })
    );
}

#[test]
fn bitstring_match_binary_splits_every_aligned_and_offset_byte_boundary() {
    let bytes = (0_u8..=57).collect::<Vec<_>>();
    let aligned = VmBitString::from_bytes(&bytes, bytes.len() * 8).expect("aligned value");
    let offset_prefix = VmBitString::from_bytes([0b1010_0000], 3).expect("offset prefix");
    let offset = offset_prefix.concat(&aligned).expect("offset value");

    for split in 0..=bytes.len() {
        let prefix_bits = split * 8;
        let suffix_bits = (bytes.len() - split) * 8;
        assert_eq!(
            aligned
                .slice(0, prefix_bits)
                .expect("aligned prefix")
                .to_bytes()
                .expect("prefix bytes")
                .as_ref(),
            &bytes[..split]
        );
        assert_eq!(
            aligned
                .slice(prefix_bits, suffix_bits)
                .expect("aligned suffix")
                .to_bytes()
                .expect("suffix bytes")
                .as_ref(),
            &bytes[split..]
        );
        assert_eq!(
            offset
                .slice(3, prefix_bits)
                .expect("offset prefix")
                .to_bytes()
                .expect("offset prefix bytes")
                .as_ref(),
            &bytes[..split]
        );
        assert_eq!(
            offset
                .slice(3 + prefix_bits, suffix_bits)
                .expect("offset suffix")
                .to_bytes()
                .expect("offset suffix bytes")
                .as_ref(),
            &bytes[split..]
        );
    }
}

#[test]
fn bitstring_match_binary_preserves_all_bit_splits_and_dynamic_widths() {
    let bytes = (0_u8..16)
        .map(|value| value.wrapping_mul(29).wrapping_add(7))
        .collect::<Vec<_>>();
    let bits = VmBitString::from_bytes(&bytes, bytes.len() * 8).expect("bit split value");

    for start in 0..=bits.bit_len() {
        for byte_len in 0..=((bits.bit_len() - start) / 8) {
            let bit_len = byte_len * 8;
            let extracted = bits.slice(start, bit_len).expect("whole-byte bit slice");
            assert_eq!(extracted.bit_len(), bit_len);
            for offset in 0..bit_len {
                assert_eq!(extracted.bit_at(offset), bits.bit_at(start + offset));
            }
        }
    }

    let large_bytes = (0..10_000)
        .map(|index| ((index * 37 + 11) % 256) as u8)
        .collect::<Vec<_>>();
    let large = VmBitString::from_bytes(&large_bytes, large_bytes.len() * 8)
        .expect("large deterministic value");
    for start in (0..=(large.bit_len() - 130)).step_by(61) {
        for (fixed, dynamic) in [
            (3, 7 - 4),
            (7, 7),
            (15, 7 + 7 + 1),
            (63, 64 - 1),
            (65, 64 + 1),
        ] {
            assert_eq!(
                large.slice(start, fixed).expect("fixed-width slice"),
                large.slice(start, dynamic).expect("dynamic-width slice")
            );
        }
    }
}

#[test]
fn bitstring_match_binary_covers_empty_units_large_prefix_and_overflow() {
    let empty = VmBitString::from_bytes([], 0).expect("empty bitstring");
    for _ in 0..10_000 {
        assert_eq!(empty.slice(0, 0), Ok(empty.clone()));
    }

    for bit_len in [0_usize, 13, 26, 39] {
        let value = VmBitString::from_bytes(vec![0xff; bit_len.div_ceil(8)], bit_len)
            .expect("13-bit unit value");
        assert_eq!(value.bit_len() % 13, 0);
    }
    for bit_len in [0_usize, 16, 32, 48] {
        let value = VmBitString::from_bytes(vec![0xff; bit_len.div_ceil(8)], bit_len)
            .expect("16-bit unit value");
        assert_eq!(value.bit_len() % 16, 0);
    }
    for bit_len in [1_usize, 8, 12, 15, 17, 25] {
        assert_ne!(bit_len % 13, 0);
        assert_ne!(bit_len % 16, 0);
    }

    let mut positioned = vec![42];
    positioned.extend_from_slice(b"abcdefghij");
    let positioned =
        VmBitString::from_bytes(&positioned, positioned.len() * 8).expect("known-position value");
    assert_eq!(
        decode_integer(
            positioned.packed_bytes(),
            0,
            8,
            false,
            VmBitStringEndian::Big,
        ),
        Ok(42)
    );
    assert_eq!(
        positioned
            .slice(8, 9 * 8)
            .expect("known-position body")
            .to_bytes()
            .expect("known-position bytes")
            .as_ref(),
        b"abcdefghi"
    );
    assert_eq!(
        positioned
            .slice(10 * 8, 8)
            .expect("known-position tail")
            .to_bytes()
            .expect("known-position tail byte")
            .as_ref(),
        b"j"
    );

    let prefix_bytes = (0..512)
        .map(|index| ((index * 17 + 3) % 256) as u8)
        .collect::<Vec<_>>();
    let mut large_match = prefix_bytes.clone();
    large_match.extend_from_slice(b" tail");
    let large_match = VmBitString::from_bytes(&large_match, large_match.len() * 8)
        .expect("greater-than-4095-bit match value");
    assert_eq!(
        large_match
            .slice(0, 4096)
            .expect("4096-bit prefix")
            .to_bytes()
            .expect("4096-bit prefix bytes")
            .as_ref(),
        prefix_bytes
    );
    assert_eq!(
        large_match
            .slice(4096, 5 * 8)
            .expect("large-prefix tail")
            .to_bytes()
            .expect("large-prefix tail bytes")
            .as_ref(),
        b" tail"
    );
    assert_eq!(
        large_match.slice(usize::MAX, 1),
        Err(VmBitStringError::RangeOverflow)
    );
}

#[test]
fn bitstring_rejects_invalid_lengths_ranges_and_alignment() {
    assert_eq!(
        VmBitString::from_bytes([0xff], 9),
        Err(VmBitStringError::BitLengthExceedsStorage {
            bit_len: 9,
            available_bits: 8,
        })
    );

    let bits = VmBitString::from_bytes([0xff], 7).expect("seven-bit bitstring");
    assert_eq!(
        bits.slice(6, 2),
        Err(VmBitStringError::RangeOutOfBounds {
            start: 6,
            end: 8,
            bit_len: 7,
        })
    );
    assert_eq!(
        bits.to_bytes(),
        Err(VmBitStringError::NotByteAligned { bit_len: 7 })
    );
}

#[test]
fn bitstring_exact_bytes_rejects_short_and_long_storage() {
    assert_eq!(
        VmBitString::from_exact_bytes([1], 2),
        Err(VmBitStringError::ByteLengthMismatch {
            expected: 2,
            actual: 1,
        })
    );
    assert_eq!(
        VmBitString::from_exact_bytes([1, 2, 3], 2),
        Err(VmBitStringError::ByteLengthMismatch {
            expected: 2,
            actual: 3,
        })
    );
    assert_eq!(
        VmBitString::from_exact_bytes([1, 2], 2)
            .expect("exact byte storage")
            .to_bytes()
            .expect("aligned bytes")
            .as_ref(),
        [1, 2]
    );
}

#[test]
fn bitstring_exact_bit_length_rejects_short_and_long_values() {
    let bits = VmBitString::from_bytes([0b1010_0000], 3).expect("three-bit value");
    assert_eq!(
        bits.require_exact_bit_len(2),
        Err(VmBitStringError::BitLengthMismatch {
            expected: 2,
            actual: 3,
        })
    );
    assert_eq!(
        bits.require_exact_bit_len(4),
        Err(VmBitStringError::BitLengthMismatch {
            expected: 4,
            actual: 3,
        })
    );
    assert_eq!(
        bits.require_exact_bit_len(3)
            .expect("exact bit length")
            .packed_bytes(),
        [0b1010_0000]
    );
}

#[test]
fn bitstring_encodes_utf8_scalar_boundaries_and_rejects_non_scalars() {
    for (value, expected) in [
        (0x00, &[0x00][..]),
        (0x7f, &[0x7f][..]),
        (0x80, &[0xc2, 0x80][..]),
        (0x7ff, &[0xdf, 0xbf][..]),
        (0x800, &[0xe0, 0xa0, 0x80][..]),
        (0x10000, &[0xf0, 0x90, 0x80, 0x80][..]),
        (0x10ffff, &[0xf4, 0x8f, 0xbf, 0xbf][..]),
    ] {
        let bits = VmBitString::from_utf8_scalar(value).expect("valid scalar");
        assert!(bits.is_byte_aligned());
        assert_eq!(bits.packed_bytes(), expected);
    }

    for value in [-1, 0xd800, 0xdfff, 0x110000] {
        assert_eq!(
            VmBitString::from_utf8_scalar(value),
            Err(VmBitStringError::InvalidUtf8Scalar { value })
        );
    }
}

#[test]
fn bitstring_decodes_exact_utf8_scalars_and_rejects_invalid_encodings() {
    for value in [0x00, 0x7f, 0x80, 0x7ff, 0x800, 0x10000, 0x10ffff] {
        let bits = VmBitString::from_utf8_scalar(value).expect("valid scalar");
        assert_eq!(bits.to_utf8_scalar(), Ok(value));
    }

    for bytes in [&[][..], &[0x80][..], &[0xc0, 0x80][..], &[b'a', b'b'][..]] {
        let bits = VmBitString::from_bytes(bytes, bytes.len() * 8).expect("byte-aligned value");
        assert_eq!(
            bits.to_utf8_scalar(),
            Err(VmBitStringError::InvalidUtf8ScalarEncoding)
        );
    }

    let unaligned = VmBitString::from_bytes([0x80], 1).expect("one-bit value");
    assert_eq!(
        unaligned.to_utf8_scalar(),
        Err(VmBitStringError::NotByteAligned { bit_len: 1 })
    );
}

#[test]
fn bitstring_utf_encodings_round_trip_every_unicode_scalar() {
    for scalar in (0_u32..=0x10ffff).filter_map(char::from_u32) {
        let value = i64::from(u32::from(scalar));
        let utf8 = VmBitString::from_utf8_scalar(value).expect("valid UTF-8 scalar");
        assert_eq!(utf8.to_utf8_scalar(), Ok(value));

        for endian in [VmBitStringEndian::Big, VmBitStringEndian::Little] {
            let utf16 = VmBitString::from_utf16_scalar(value, endian).expect("valid UTF-16 scalar");
            assert_eq!(utf16.to_utf16_scalar(endian), Ok(value));
            let utf32 = VmBitString::from_utf32_scalar(value, endian).expect("valid UTF-32 scalar");
            assert_eq!(utf32.to_utf32_scalar(endian), Ok(value));
        }
    }
}

#[test]
fn bitstring_utf16_and_utf32_preserve_wire_order_and_offset_values() {
    for (value, utf16_be, utf16_le, utf32_be, utf32_le) in [
        (
            0x0041,
            &[0x00, 0x41][..],
            &[0x41, 0x00][..],
            &[0x00, 0x00, 0x00, 0x41][..],
            &[0x41, 0x00, 0x00, 0x00][..],
        ),
        (
            0x20ac,
            &[0x20, 0xac][..],
            &[0xac, 0x20][..],
            &[0x00, 0x00, 0x20, 0xac][..],
            &[0xac, 0x20, 0x00, 0x00][..],
        ),
        (
            0x1f600,
            &[0xd8, 0x3d, 0xde, 0x00][..],
            &[0x3d, 0xd8, 0x00, 0xde][..],
            &[0x00, 0x01, 0xf6, 0x00][..],
            &[0x00, 0xf6, 0x01, 0x00][..],
        ),
    ] {
        for (endian, expected16, expected32) in [
            (VmBitStringEndian::Big, utf16_be, utf32_be),
            (VmBitStringEndian::Little, utf16_le, utf32_le),
        ] {
            let utf16 = VmBitString::from_utf16_scalar(value, endian).expect("UTF-16 scalar");
            let utf32 = VmBitString::from_utf32_scalar(value, endian).expect("UTF-32 scalar");
            assert_eq!(utf16.packed_bytes(), expected16);
            assert_eq!(utf32.packed_bytes(), expected32);

            let prefix = VmBitString::from_bytes([0b1010_0000], 3).expect("offset prefix");
            let offset16 = prefix.concat(&utf16).expect("offset UTF-16");
            let offset32 = prefix.concat(&utf32).expect("offset UTF-32");
            assert_eq!(
                offset16
                    .slice(3, utf16.bit_len())
                    .expect("offset UTF-16 scalar")
                    .to_utf16_scalar(endian),
                Ok(value)
            );
            assert_eq!(
                offset32
                    .slice(3, utf32.bit_len())
                    .expect("offset UTF-32 scalar")
                    .to_utf32_scalar(endian),
                Ok(value)
            );
        }
    }
}

#[test]
fn bitstring_utf16_and_utf32_reject_invalid_scalars_and_encodings() {
    for value in (-100..0).chain(0xd800..=0xdfff).chain(0x110000..=0x1101ff) {
        for endian in [VmBitStringEndian::Big, VmBitStringEndian::Little] {
            assert_eq!(
                VmBitString::from_utf16_scalar(value, endian),
                Err(VmBitStringError::InvalidUtf16Scalar { value })
            );
            assert_eq!(
                VmBitString::from_utf32_scalar(value, endian),
                Err(VmBitStringError::InvalidUtf32Scalar { value })
            );
        }
    }

    for surrogate in 0xd800_u16..=0xdfff {
        for endian in [VmBitStringEndian::Big, VmBitStringEndian::Little] {
            let bytes = match endian {
                VmBitStringEndian::Big => surrogate.to_be_bytes(),
                VmBitStringEndian::Little => surrogate.to_le_bytes(),
            };
            let invalid = VmBitString::from_bytes(bytes, 16).expect("surrogate code unit");
            assert_eq!(
                invalid.to_utf16_scalar(endian),
                Err(VmBitStringError::InvalidUtf16ScalarEncoding { endian })
            );
        }
    }

    for endian in [VmBitStringEndian::Big, VmBitStringEndian::Little] {
        for bytes in [&[][..], &[0][..], &[0, 65, 0][..], &[0, 65, 0, 66][..]] {
            let invalid =
                VmBitString::from_bytes(bytes, bytes.len() * 8).expect("invalid UTF-16 shape");
            assert_eq!(
                invalid.to_utf16_scalar(endian),
                Err(VmBitStringError::InvalidUtf16ScalarEncoding { endian })
            );
        }
        for value in [0x0000_d800_u32, 0x0011_0000, u32::MAX] {
            let bytes = match endian {
                VmBitStringEndian::Big => value.to_be_bytes(),
                VmBitStringEndian::Little => value.to_le_bytes(),
            };
            let invalid = VmBitString::from_bytes(bytes, 32).expect("invalid UTF-32 scalar");
            assert_eq!(
                invalid.to_utf32_scalar(endian),
                Err(VmBitStringError::InvalidUtf32ScalarEncoding { endian })
            );
        }
        for bytes in [&[][..], &[0][..], &[0, 0, 65][..], &[0; 5][..]] {
            let invalid =
                VmBitString::from_bytes(bytes, bytes.len() * 8).expect("invalid UTF-32 shape");
            assert_eq!(
                invalid.to_utf32_scalar(endian),
                Err(VmBitStringError::InvalidUtf32ScalarEncoding { endian })
            );
        }
    }

    let unaligned = VmBitString::from_bytes([0], 1).expect("unaligned scalar");
    assert_eq!(
        unaligned.to_utf16_scalar(VmBitStringEndian::Big),
        Err(VmBitStringError::NotByteAligned { bit_len: 1 })
    );
    assert_eq!(
        unaligned.to_utf32_scalar(VmBitStringEndian::Little),
        Err(VmBitStringError::NotByteAligned { bit_len: 1 })
    );
}

#[test]
fn bitstring_concatenates_exact_logical_bits_without_padding() {
    let prefix = VmBitString::from_bytes([0b1010_0000], 3).expect("three-bit prefix");
    let suffix = VmBitString::from_bytes([0b1101_0000], 5).expect("five-bit suffix");
    let combined = prefix.concat(&suffix).expect("combined bitstring");

    assert_eq!(combined.bit_len(), 8);
    assert_eq!(combined.packed_bytes(), &[0b1011_1010]);

    let tail = VmBitString::from_bytes([0b0110_0000], 4).expect("four-bit tail");
    let unaligned = combined
        .slice(1, 6)
        .expect("six-bit prefix")
        .concat(&tail)
        .expect("ten-bit combined value");
    assert_eq!(unaligned.bit_len(), 10);
    assert_eq!(unaligned.packed_bytes(), &[0b0111_0101, 0b1000_0000]);
}

#[test]
fn bitstring_concat_preserves_empty_identity_and_rejects_length_overflow() {
    let empty = VmBitString::from_bytes([], 0).expect("empty bitstring");
    let bits = VmBitString::from_bytes([0b1010_0000], 4).expect("four-bit value");
    assert_eq!(empty.concat(&bits), Ok(bits.clone()));
    assert_eq!(bits.concat(&empty), Ok(bits));

    let impossible = VmBitString {
        bytes: Arc::from([]),
        bit_len: usize::MAX,
    };
    let one = VmBitString::from_bytes([0x80], 1).expect("one bit");
    assert_eq!(
        impossible.concat(&one),
        Err(VmBitStringError::BitLengthOverflow)
    );
}

#[test]
fn bitstring_constructs_signed_and_unsigned_integer_fields_in_both_orders() {
    for (value, width, signed, endian, expected) in [
        (0x1234, 16, false, VmBitStringEndian::Big, &[0x12, 0x34][..]),
        (
            0x1234,
            16,
            false,
            VmBitStringEndian::Little,
            &[0x34, 0x12][..],
        ),
        (0x0abc, 12, false, VmBitStringEndian::Big, &[0xab, 0xc0][..]),
        (
            0x0abc,
            12,
            false,
            VmBitStringEndian::Little,
            &[0xbc, 0xa0][..],
        ),
        (-2, 4, true, VmBitStringEndian::Big, &[0xe0][..]),
        (-257, 12, true, VmBitStringEndian::Little, &[0xff, 0xe0][..]),
    ] {
        let bits = VmBitString::from_integer(value, width, signed, endian)
            .expect("representable integer field");
        assert_eq!(bits.bit_len(), width);
        assert_eq!(bits.packed_bytes(), expected);
    }
}

#[test]
fn bitstring_integer_construction_enforces_width_and_signedness_boundaries() {
    for (value, width, signed) in [
        (0, 0, false),
        (0, 64, false),
        (-1, 1, false),
        (2, 1, false),
        (-2, 1, true),
        (1, 1, true),
    ] {
        assert!(
            VmBitString::from_integer(value, width, signed, VmBitStringEndian::Big).is_err(),
            "value={value}, width={width}, signed={signed}"
        );
    }

    assert!(VmBitString::from_integer(i64::MAX, 63, false, VmBitStringEndian::Big).is_ok());
    assert!(VmBitString::from_integer(-(1_i64 << 62), 63, true, VmBitStringEndian::Big).is_ok());
    assert!(
        VmBitString::from_integer((1_i64 << 62) - 1, 63, true, VmBitStringEndian::Little).is_ok()
    );
}

#[test]
fn bitstring_decodes_signed_and_unsigned_integer_fields_in_both_orders() {
    for (value, width, signed, endian) in [
        (0x1234, 16, false, VmBitStringEndian::Big),
        (0x1234, 16, false, VmBitStringEndian::Little),
        (0x0abc, 12, false, VmBitStringEndian::Big),
        (0x0abc, 12, false, VmBitStringEndian::Little),
        (-2, 4, true, VmBitStringEndian::Big),
        (-257, 12, true, VmBitStringEndian::Big),
        (-257, 12, true, VmBitStringEndian::Little),
        (i64::MAX, 63, false, VmBitStringEndian::Big),
        (-(1_i64 << 62), 63, true, VmBitStringEndian::Little),
    ] {
        let bits = VmBitString::from_integer(value, width, signed, endian)
            .expect("representable integer field");
        assert_eq!(bits.to_integer(signed, endian), Ok(value));
    }
}

#[test]
fn shared_integer_decoder_handles_offsets_and_rejects_invalid_ranges() {
    let bytes = [0b1111_1010, 0b1011_1100, 0b0000_0000];
    assert_eq!(
        decode_integer(&bytes, 4, 12, false, VmBitStringEndian::Big),
        Ok(0x0abc)
    );
    assert_eq!(
        decode_integer(&bytes, 4, 12, false, VmBitStringEndian::Little),
        Ok(0x0cab)
    );

    for width in [0, 64] {
        assert_eq!(
            decode_integer(&bytes, 0, width, false, VmBitStringEndian::Big),
            Err(VmBitStringError::InvalidIntegerWidth { bit_width: width })
        );
    }
    assert_eq!(
        decode_integer(&bytes, 20, 8, false, VmBitStringEndian::Big),
        Err(VmBitStringError::RangeOutOfBounds {
            start: 20,
            end: 28,
            bit_len: 24,
        })
    );
    assert_eq!(
        decode_integer(&bytes, usize::MAX, 1, false, VmBitStringEndian::Big,),
        Err(VmBitStringError::RangeOverflow)
    );

    let empty = VmBitString::from_bytes([], 0).expect("empty bitstring");
    assert_eq!(
        empty.to_integer(false, VmBitStringEndian::Big),
        Err(VmBitStringError::InvalidIntegerWidth { bit_width: 0 })
    );
    let too_wide = VmBitString::from_bytes([0; 8], 64).expect("64-bit bitstring");
    assert_eq!(
        too_wide.to_integer(true, VmBitStringEndian::Little),
        Err(VmBitStringError::InvalidIntegerWidth { bit_width: 64 })
    );
}
