//! Shared nonnegative timestamp admission without changing the authoritative JSON number.
use serde_json::Number;

pub(crate) fn valid_timestamp(value: &Number) -> bool {
    // Negative nonzero significands can underflow to -0.0 in f64. Inspect the
    // exact significand; exponent digits do not make a mathematical zero nonzero.
    let nonnegative = value.as_str().strip_prefix('-').is_none_or(|digits| {
        digits
            .bytes()
            .take_while(|b| !matches!(b, b'e' | b'E'))
            .all(|b| matches!(b, b'0' | b'.'))
    });
    // Preserve the existing finite-f64 range without rounding the stored value.
    nonnegative && value.as_f64().is_some_and(f64::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_significand_distinguishes_zero_from_underflow_without_expanding_exponents() {
        for (source, accepted) in [
            ("0", true),
            ("-0.0", true),
            ("-0.000e-9999", true),
            ("-0.000E-9999", true),
            ("1e-9999", true),
            ("9007199254740993.125", true),
            ("-0.0001e-9999", false),
            ("-0.0001E-9999", false),
            ("-5e-324", false),
            ("-0.1", false),
            ("1e9999", false),
            ("-1e9999", false),
        ] {
            let value: Number = source.parse().unwrap();
            assert_eq!(valid_timestamp(&value), accepted, "{source}");
        }
    }
}
