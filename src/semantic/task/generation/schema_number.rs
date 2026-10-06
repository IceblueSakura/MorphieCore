//! Exact, bounded Schema comparisons; never expand an exponent into a huge integer.
use super::GenerationError;
use serde_json::Number;
use std::cmp::Ordering;

pub(super) struct Decimal {
    negative: bool,
    digits: String,
    exponent: i64,
}
impl Decimal {
    pub(super) fn new(number: &Number) -> Result<Self, GenerationError> {
        let source = number.as_str();
        let negative = source.starts_with('-');
        let source = source.trim_start_matches('-');
        let (mantissa, exponent) = source.split_once(['e', 'E']).unwrap_or((source, "0"));
        let fraction = mantissa.split_once('.').map_or(0, |(_, part)| part.len());
        let mut digits = mantissa.replace('.', "");
        let leading = digits.len() - digits.trim_start_matches('0').len();
        digits.drain(..leading);
        if digits.is_empty() {
            return Ok(Self {
                negative: false,
                digits: "0".into(),
                exponent: 0,
            });
        }
        let trailing = digits.len() - digits.trim_end_matches('0').len();
        digits.truncate(digits.len() - trailing);
        // The normalization domain is bounded; an unrepresentable exponent fails
        // explicitly instead of rounding or allocating exponent-sized output.
        let exponent = exponent
            .parse::<i64>()
            .map_err(|_| GenerationError::Limit)?
            .checked_sub(i64::try_from(fraction).map_err(|_| GenerationError::Limit)?)
            .and_then(|value| value.checked_add(trailing as i64))
            .ok_or(GenerationError::Limit)?;
        Ok(Self {
            negative,
            digits,
            exponent,
        })
    }
    pub(super) fn positive(&self) -> bool {
        !self.negative && self.digits != "0"
    }
    pub(super) fn canonical(&self) -> String {
        if self.digits == "0" {
            return "0".into();
        }
        format!(
            "{}{}e{}",
            if self.negative { "-" } else { "" },
            self.digits,
            self.exponent
        )
    }
    pub(super) fn natural(self) -> Result<Natural, GenerationError> {
        if self.negative || self.exponent < 0 {
            return Err(GenerationError::InvalidSchema);
        }
        let order = self
            .exponent
            .checked_add(self.digits.len() as i64)
            .ok_or(GenerationError::Limit)?;
        Ok(Natural {
            digits: self.digits,
            order,
        })
    }
}
pub(super) struct Natural {
    digits: String,
    order: i64,
}
impl Natural {
    pub(super) fn cmp(&self, other: &Self) -> Ordering {
        match (self.digits == "0", other.digits == "0") {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        self.order.cmp(&other.order).then_with(|| {
            let width = self.digits.len().max(other.digits.len());
            self.digits
                .bytes()
                .chain(std::iter::repeat(b'0'))
                .take(width)
                .cmp(
                    other
                        .digits
                        .bytes()
                        .chain(std::iter::repeat(b'0'))
                        .take(width),
                )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equivalent_numbers_have_one_key_without_float_or_exponent_expansion() {
        for (left, right) in [
            ("123.4500", "1.2345e2"),
            ("-0.000", "0"),
            ("1000", "1e3"),
            ("0.00120", "12e-4"),
        ] {
            let key = |source: &str| Decimal::new(&source.parse().unwrap()).unwrap().canonical();
            assert_eq!(key(left), key(right));
        }
        let value = Decimal::new(&"1e9999".parse().unwrap()).unwrap();
        assert_eq!(value.canonical(), "1e9999");
        assert!(Decimal::new(&"1e9223372036854775808".parse().unwrap()).is_err());
    }
    #[test]
    fn natural_comparison_matches_small_integer_order() {
        for a in [0u64, 1, 10, 99, 100, 901, 1000, u64::MAX] {
            for b in [0u64, 1, 10, 99, 100, 901, 1000, u64::MAX] {
                let value = |n| Decimal::new(&Number::from(n)).unwrap().natural().unwrap();
                assert_eq!(value(a).cmp(&value(b)), a.cmp(&b));
            }
        }
    }
}
