# Generation schema admission

This is the local schema admission contract, not a general JSON Schema evaluator or proof of model output adherence. Semantic ownership stays with `OutputConstraint::JsonSchema` and `FunctionTool`; protocol codecs do not maintain a second schema. The fixed [Structured Outputs baseline](../references/responses-standard.md#4-tools-与-schema) supplies the strict rules and dated quotas. No live Provider validation is implied.

## Modes and defaults

| Owner / control | Mode |
|---|---|
| text.format strict=true | Explicit strict |
| text.format strict absent/null/false | General structural |
| function parameters, Explicit(true) | Explicit strict |
| function parameters, Omitted(NormalizeSchema) | Responses normalization intent |
| function parameters, Explicit(false) or Omitted(NonStrict) | General structural |
| function output_schema | General structural, independent of parameter strict |

Absent function schemas remain absent. General structural mode permits `{}` and does not require an object instance type. All schema nodes in this profile are JSON objects; boolean values are allowed as additional/unevaluated property/item policies, not as standalone schema nodes.

Normalization intent shares strict vocabulary, root and quota admission but does not demand already-complete required lists or explicit additionalProperties=false. Those omissions retain their declared source default; the gateway neither adds constraints nor claims that normalization has executed. Explicit strict requires those invariants before accepting the schema. Existing cross-profile strict-default rejection remains in lowering.

## Structural vocabulary

| Family | Shape / validation |
|---|---|
| type | Known JSON type name or nonempty, unique array of names |
| properties, patternProperties, $defs, dependentSchemas | Object of schema objects; names are data, not keyword names |
| required, dependentRequired | Unique string arrays; dependentRequired is a name-to-array map |
| items, contains, propertyNames, not, if, then, else | Schema object |
| anyOf, allOf, oneOf, prefixItems | Nonempty array of schema objects |
| additionalProperties, unevaluatedProperties, unevaluatedItems | Boolean policy or schema object |
| enum, const | Nonempty unique enum values; const may be any bounded JSON value. Numeric equality and object-key order are considered when checking duplicate enum values |
| title, description, pattern, format | Strings; `pattern` must parse in the admitted regex subset and `format` must name a registry entry, see [below](#pattern-and-format-syntax) |
| default, examples | Bounded JSON annotation; examples is an array. Defaults do not make strict properties optional |
| minimum, maximum, exclusiveMinimum, exclusiveMaximum | Numbers |
| multipleOf | Positive number |
| min/maxLength, min/maxItems, min/maxProperties, min/maxContains | Nonnegative integers; a present lower bound cannot exceed its upper bound |
| uniqueItems | Boolean |
| $ref | Resolvable local JSON Pointer fragment; see below |

Unknown keywords, alternate dialect declarations (`$schema`), resource rebasing (`$id`), anchors/dynamic references and unsupported boolean-schema positions are rejected, not ignored. This is a deliberately closed vocabulary, not a statement that these are invalid in every JSON Schema dialect.

## Strict subset

Both strict modes require an object root (possibly reached through a local reference), not root anyOf. Non-reference/non-anyOf nodes require a type; array types require items. Type arrays in the strict profile express a nullable single type. Object/array-specific structural fields must agree with that type. A strict $ref node may carry only title/description/default/examples annotations and $defs alongside the reference; constraints are not silently merged or dropped.

Explicit strict additionally requires every object to use additionalProperties=false and requires exactly its declared property names, with no duplicates. Empty objects may omit an empty required list. Nested anyOf branches and definition schemas receive the same checks. allOf/oneOf/not/if/then/else, dynamic-object/tuple/contains/unevaluated policies, uniqueItems and property-count constraints are outside this strict subset. General structural mode may preserve the listed forms.

Supported string/number/array constraints receive shape and basic bound checks. This validates `pattern`/`format` syntax against the closed admission below, but no regex execution, format semantics, model-specific restrictions, schema satisfiability or generated data. No schema is rewritten to satisfy strictness. Numeric comparisons and enum equality are exact; exponent-sized integers are never allocated. The bounded normalization domain is owned by [schema numbers](../../src/semantic/task/generation/schema_number.rs), and exceeding it fails rather than rounding.

## Pattern and format syntax

`pattern` values and `patternProperties` keys use ECMAScript Unicode (`u`) syntax, parsed by the pinned Oxc parser. Alternation, capturing/non-capturing/named groups, lookahead/lookbehind, character classes, quantifiers and Unicode property escapes are admitted when valid in that syntax. Unicode property names and capture references must be valid; legacy identity escapes such as `\!` are not accepted. Inline flag modifiers, Unicode-set (`v`) syntax and buffer assertions (`\A`, `\z`, `\Z`) are outside the local profile. There is no permissive legacy-mode fallback. Parsing does not compile a matcher, evaluate an instance or rewrite the authoritative pattern.

Before parsing, a non-recursive scan enforces at most 65,536 UTF-8 bytes, 64 nested groups/assertions and 1,024 group openings per pattern. Escaped delimiters and character-class contents do not increase group depth. Exceeding these budgets returns Limit, independently of the containing JSON's depth. AST inspection uses an iterative work list; parser diagnostics containing input are never exposed.

`format` must name an entry of the closed registry drawn from the JSON Schema 2020-12 validation vocabulary: `date`, `time`, `date-time`, `duration`, `email`, `idn-email`, `hostname`, `idn-hostname`, `ipv4`, `ipv6`, `uri`, `uri-reference`, `uri-template`, `iri`, `iri-reference`, `uuid`, `json-pointer`, `relative-json-pointer`, `regex`. Unknown names are rejected rather than ignored; this is not a claim that they are invalid in every JSON Schema dialect. Neither rule predicts whether a given model enforces the constraint.

## References and resources

A reference must be `#` or a `#/...` pointer, with valid percent encoding, UTF-8 and ~0/~1 escapes. Targets must be registered schema positions, not arbitrary objects in default/enum/examples or the properties map itself. Forward references and recursive graphs are allowed. Every schema node is checked once in the physical document, including unused definitions; every reference edge is resolved without recursively expanding its target. Removing a referenced definition must fail revalidation.

Local resource bounds apply in every mode:

- Serialized schema: 1 MiB; preflight raw Value depth 64 and 65,536 values/keys before recursive serialization or cloning of enum data.
- Schema nodes: 16,384; reference edges: 8,192; stored pointer-path bytes: 4 MiB.
- Enum entries: 8,192 total; canonical enum comparison data: 4 MiB total.

The two strict modes additionally use the fixed baseline ceilings: 5,000 properties, 1,000 enum entries, 120,000 characters across property/definition names and string contents of enum/const values, and 10 nested object/array levels in each declared schema shape. Definitions start their own shape; applicators do not add an instance level. References do not unfold this depth: legal recursion is not rejected as infinite nesting. These are local admission bounds, not a prediction of maximum generated-instance depth or universal model capabilities.

Validation borrows the authoritative schema. It must not sort properties, fill required, change additionalProperties, resolve remote data or resurrect source values. Order-sensitive transformation/encoding rules remain in the [text profile](responses-text-profile.md#schema-property-order). Structural/profile failures use a schema-specific semantic error; exhausted resource budgets use Limit.
