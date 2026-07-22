use super::super::actor::{VmActorReceive, VmActorRuntime};
use super::super::bitstring::{VmBitString, VmBitStringEndian};
use super::super::process::VmProcessSource;
use super::ReplValue;

#[test]
fn bs_bit_binaries_preserve_exact_small_large_and_asymmetric_lengths() {
    for bit_len in [0_usize, 1, 7, 9, 80, 100, 101, 800, 1_001, 1_000_001] {
        let storage = vec![0xff; bit_len.div_ceil(8)];
        let bits = VmBitString::from_bytes(&storage, bit_len).expect("valid exact bit length");
        assert_eq!(bits.bit_len(), bit_len);
        assert_eq!(bits.byte_len(), bit_len.div_ceil(8));
        assert_eq!(bits.slice(0, bit_len), Ok(bits.clone()));
        assert_eq!(
            bits.slice(bit_len, 0)
                .expect("empty terminal slice")
                .bit_len(),
            0
        );
        if bit_len % 8 != 0 && bit_len != 0 {
            let unused_mask = u8::MAX >> (bit_len % 8);
            assert_eq!(
                bits.packed_bytes().last().copied().unwrap() & unused_mask,
                0
            );
        }
    }

    let twelve =
        VmBitString::from_integer(1, 12, false, VmBitStringEndian::Big).expect("twelve-bit value");
    assert_eq!(twelve.packed_bytes(), [0, 16]);

    let asymmetric = VmBitString::from_bytes([128, 255, 0, 0], 26)
        .expect("one leading bit and twenty-five trailing bits");
    let head = asymmetric.slice(0, 1).expect("leading bit");
    let tail = asymmetric.slice(1, 25).expect("asymmetric tail");
    assert_eq!(head.packed_bytes(), [128]);
    assert_eq!(tail.packed_bytes(), [1, 254, 0, 0]);
    assert_eq!(head.concat(&tail), Ok(asymmetric));
}

#[test]
fn bs_bit_binaries_split_reconstruct_and_append_without_alignment_padding() {
    let trailing =
        VmBitString::from_bytes([1, 2, 3, 4, 128], 33).expect("four bytes plus one trailing bit");
    let byte_prefix = trailing.slice(0, 32).expect("aligned prefix");
    let last_bit = trailing.slice(32, 1).expect("trailing bit");
    assert_eq!(
        byte_prefix.to_bytes().expect("aligned bytes").as_ref(),
        [1, 2, 3, 4]
    );
    assert_eq!(last_bit.packed_bytes(), [128]);
    assert_eq!(byte_prefix.concat(&last_bit), Ok(trailing));

    let leading = VmBitString::from_bytes([128, 129, 1, 130, 0], 33)
        .expect("one leading bit followed by four logical bytes");
    let shifted_bytes = (0..4)
        .map(|index| {
            leading
                .slice(index * 8, 8)
                .and_then(|chunk| chunk.to_integer(false, VmBitStringEndian::Big))
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("decode shifted byte chunks");
    assert_eq!(shifted_bytes, [128, 129, 1, 130]);
    assert_eq!(
        leading
            .slice(32, 1)
            .expect("final remainder")
            .packed_bytes(),
        [0]
    );

    let one = VmBitString::from_bytes([128], 1).expect("one bit");
    let all_ones = (0..2_048)
        .try_fold(
            VmBitString::from_bytes([], 0).expect("empty bitstring"),
            |prefix, _| prefix.concat(&one),
        )
        .expect("append 2,048 individual bits");
    assert_eq!(all_ones.bit_len(), 2_048);
    assert_eq!(
        all_ones.to_bytes().expect("aligned ones").as_ref(),
        &[0xff; 256]
    );
}

#[test]
fn bs_bit_binaries_round_trip_repeated_actor_messages_without_mutation() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(VmProcessSource::new("app.BitBinaryParity", "sender", 0));
    let receiver = runtime.spawn_root(VmProcessSource::new("app.BitBinaryParity", "receiver", 0));
    let payload = VmBitString::from_bytes(vec![0xa5; 125_001], 1_000_001)
        .expect("large unaligned message payload");
    let snapshot = payload.clone();

    for _ in 0..100 {
        runtime
            .send(sender, receiver, ReplValue::BitString(payload.clone()))
            .expect("send bitstring payload");
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(receiver)
            .expect("receive bitstring payload")
        else {
            panic!("queued bitstring message must be receivable");
        };
        assert_eq!(message.payload, ReplValue::BitString(payload.clone()));
    }

    assert_eq!(payload, snapshot);
    assert_eq!(payload.bit_len(), 1_000_001);
    assert_eq!(
        payload.packed_bytes().last().copied().unwrap() & 0b0111_1111,
        0,
        "unused trailing storage bits remain canonical after every send"
    );
}
