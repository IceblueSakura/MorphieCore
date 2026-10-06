//! Bounded ECMAScript Unicode syntax, never regex compilation or matching.
use super::GenerationError;
use oxc_allocator::Allocator;
use oxc_regular_expression::{
    LiteralParser, Options,
    ast::{BoundaryAssertionKind, Term},
};

const MAX_BYTES: usize = 65_536;
const MAX_DEPTH: usize = 64;
const MAX_GROUPS: usize = 1_024;

pub(super) fn validate(pattern: &str) -> Result<(), GenerationError> {
    preflight(pattern)?;
    let allocator = Allocator::default();
    // Pattern text is already JSON-decoded; ConstructorParser would unescape it twice.
    // https://docs.rs/oxc_regular_expression/0.152.0/oxc_regular_expression/struct.LiteralParser.html
    let ast = LiteralParser::new(&allocator, pattern, Some("u"), Options::default())
        .parse()
        .map_err(|_| GenerationError::InvalidSchema)?;
    let mut pending: Vec<_> = ast.body.body.iter().flat_map(|a| a.body.iter()).collect();
    while let Some(term) = pending.pop() {
        let child = match term {
            Term::IgnoreGroup(group) => {
                if group.modifiers.is_some() {
                    return Err(GenerationError::InvalidSchema);
                }
                Some(&group.body)
            }
            Term::CapturingGroup(group) => Some(&group.body),
            Term::LookAroundAssertion(assertion) => Some(&assertion.body),
            Term::Quantifier(quantifier) => {
                pending.push(&quantifier.body);
                None
            }
            Term::BoundaryAssertion(assertion) => {
                if !matches!(
                    assertion.kind,
                    BoundaryAssertionKind::Start
                        | BoundaryAssertionKind::End
                        | BoundaryAssertionKind::Boundary
                        | BoundaryAssertionKind::NegativeBoundary
                ) {
                    return Err(GenerationError::InvalidSchema);
                }
                None
            }
            _ => None,
        };
        if let Some(child) = child {
            pending.extend(child.body.iter().flat_map(|a| a.body.iter()));
        }
    }
    Ok(())
}

/// Bound recursion and allocation before handing untrusted input to the parser.
/// Escaped delimiters and class contents do not introduce group nesting in u mode.
fn preflight(pattern: &str) -> Result<(), GenerationError> {
    if pattern.len() > MAX_BYTES {
        return Err(GenerationError::Limit);
    }
    let (mut depth, mut groups, mut class, mut escaped) = (0usize, 0usize, false, false);
    for byte in pattern.bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' => escaped = true,
            b']' if class => class = false,
            _ if class => (),
            b'[' => class = true,
            b'(' => {
                depth += 1;
                groups += 1;
                if depth > MAX_DEPTH || groups > MAX_GROUPS {
                    return Err(GenerationError::Limit);
                }
            }
            b')' => depth = depth.saturating_sub(1),
            _ => (),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    proptest! {
        #![proptest_config(crate::test_properties::config())]
        #[test]
        fn property_regex_group_bounds(depth in 0usize..129, open in prop::sample::select(vec!["(", "(?:", "(?=", "(?<="])) {
            let pattern = format!("{}a{}", open.repeat(depth), ")".repeat(depth));
            prop_assert_eq!(validate(&pattern), if depth <= MAX_DEPTH { Ok(()) } else { Err(GenerationError::Limit) });
        }
        #[test]
        fn property_regex_delimiters_in_classes_and_escapes_are_not_groups(n in 1025usize..2048) {
            let class = format!("[{}]", "(".repeat(n));
            prop_assert!(validate(&class).is_ok());
            prop_assert!(validate(&r"\(".repeat(n)).is_ok());
        }
        #[test]
        fn property_regex_arbitrary_bounded_utf8_never_panics(chars in prop::collection::vec(any::<char>(), 0..2048)) {
            let _ = validate(&chars.into_iter().collect::<String>());
        }
    }
    #[test]
    fn extreme_nesting_and_flat_work_are_rejected_on_a_small_worker_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                assert_eq!(
                    validate(&format!("{}a{}", "(".repeat(50_000), ")".repeat(50_000))),
                    Err(GenerationError::Limit)
                );
                assert_eq!(
                    validate(&"()".repeat(MAX_GROUPS + 1)),
                    Err(GenerationError::Limit)
                );
                assert!(
                    validate(&format!(
                        "{}a{}",
                        "(".repeat(MAX_DEPTH),
                        ")".repeat(MAX_DEPTH)
                    ))
                    .is_ok()
                );
                assert!(validate(&"a".repeat(MAX_BYTES)).is_ok());
                assert!(validate(&r"\(\)[()]".repeat(64)).is_ok());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
