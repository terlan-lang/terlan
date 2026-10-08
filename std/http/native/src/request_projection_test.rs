use super::RequestFieldProjection as Projection;

#[test]
fn bitmask_operations_fail_closed_on_overflow() {
    for field in 0..16 {
        let mut projection = Projection::empty();
        projection.include(field);
        assert_eq!(projection, Projection::Fields(1 << field));
        assert!(projection.requires(field));
        projection.include(field);
        assert_eq!(projection, Projection::Fields(1 << field));
    }
    for field in [16, 32, 64, u32::MAX as usize, usize::MAX] {
        let mut projection = Projection::empty();
        assert!(!projection.requires(field));
        projection.include(field);
        assert_eq!(projection, Projection::Complete);
        assert!(projection.requires(field));
        projection.include(1);
        assert_eq!(projection, Projection::Complete);
    }
}

#[test]
fn request_contract_admits_only_declared_positions() {
    for mask in 0_u16..512 {
        let fields: Vec<_> = (0..9).filter(|field| mask & (1 << field) != 0).collect();
        let projection = Projection::from_observed_fields(fields);
        assert_eq!(projection, Projection::Fields(mask << 1));
        for field in 0..=10 {
            let observed = mask << 1 & (1 << field) != 0;
            assert_eq!(projection.requires(field), observed);
            assert_eq!(
                projection.admits_scalar_string(field),
                matches!(field, 1 | 2 | 4 | 5 | 9) && mask << 1 == 1 << field
            );
        }
    }
    for invalid in [9, 10, 16, 32, 64, usize::MAX] {
        for fields in [vec![invalid], vec![1, invalid], vec![invalid, 1]] {
            assert_eq!(
                Projection::from_observed_fields(fields),
                Projection::Complete
            );
        }
        assert!(!Projection::Complete.admits_scalar_string(invalid));
        assert!(!Projection::Fields(u16::MAX).admits_scalar_string(invalid));
    }
    assert_eq!(
        Projection::from_observed_fields(vec![0, 0, 0]),
        Projection::Fields(2)
    );
    for field in 1..=9 {
        assert!(!Projection::Complete.admits_scalar_string(field));
    }
}
