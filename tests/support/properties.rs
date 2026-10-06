//! Reproducible, bounded in-process generation; no canonical corpus side effects.
pub fn config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: 64,
        max_shrink_iters: 256,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x4f_42_52_49_44_47_45),
        failure_persistence: None,
        ..Default::default()
    }
}
