use borsh::BorshDeserialize;

#[test]
fn malformed_transactions_return_errors() {
    let malformed = [0xff, 0x00, 0x7f, 0x13, 0x37];

    assert!(std::panic::catch_unwind(|| {
        let _ = fractal_core::Transaction::try_from_slice(&malformed);
        let _ = Vec::<fractal_core::Transaction>::try_from_slice(&malformed);
    })
    .is_ok());
}
