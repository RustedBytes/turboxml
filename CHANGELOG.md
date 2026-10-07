# Changelog

## [1.1.0] - Unreleased

### Added

- Reproducible manual benchmarks against `xml.etree.ElementTree` for structured
  XML with attributes, deep nesting, and text-heavy UTF-8 documents. Record
  input sizes, warmup/repeats, median/p95 and interpreter/platform metadata.
- Paired build comparison and full-field traversal modes. Performance runs are
  optional and have no required PR gate or timing threshold; JSON output is
  written locally under ignored `target/`.
- Regression coverage for Unicode input, text/CDATA/entity accumulation,
  concurrent parsing, retained child nodes, file-buffer lifetime and copying.

### Changed

- Avoid copying Python XML input into a Rust `String` using `PyBackedStr`, while
  continuing to release the GIL during parsing.
- Keep parsed tag names, attribute keys/plain values and single plain-text
  segments as ranges into one shared immutable document buffer. Decoding,
  whitespace normalization and joining text segments still use owned buffers.
- Reduce intermediate text copies, initialize randomized attribute hashing
  once per document and reduce the initial allocation for child vectors.
- Preserve Python string/dict property types and dictionary snapshot behavior;
  materialize Python values when fields are accessed. Serialization snapshots
  share parsed string storage instead of copying every plain string.
- Replace general README performance claims with links to measured synthetic
  results, reproduction instructions and limitations.

- Rename the workflow to `ci.yml`, filter normal CI to code/test/packaging
  changes, skip draft PR checks and cancel obsolete PR/main runs. Keep the full
  artifact matrix for version tags or an explicit manual request; publishing
  requires a version tag or the manual `publish` option. Add installed-wheel
  regression smoke tests for CPython and PyPy.

### Fixed

- Remove redundant `XmlText` conversions in writer test fixtures that caused
  `cargo clippy --all-targets -- -D warnings` to fail.

### Memory behavior

- A retained parsed node now keeps the entire document buffer alive, including
  when a small subtree is selected. To keep an independent copy, use
  `turboxml.Node.from_dict(node.to_dict())` and release the original nodes.
- Python objects, the structural index, attribute maps and child vectors still
  allocate; shared string storage does not make the entire tree zero-allocation.

[1.1.0]: https://github.com/RustedBytes/turboxml/pull/1
