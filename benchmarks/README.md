# Manual parsing benchmark

Build the checked-out source in release mode (do not benchmark `maturin develop`
without `--release`):

```sh
python -m venv .venv
. .venv/bin/activate
python -m pip install maturin
maturin build --release --locked --interpreter python --out dist
python -m pip install --force-reinstall dist/*.whl
python benchmarks/parsing.py --warmup 5 --repeats 31 --iterations 10 --output target/parsing-results.json
```

This benchmark is manual and is not a PR gate. It has no timing threshold.
Input generation, UTF-8 sizing/hashing, and iterative equality validation happen
outside timing. Both parsers receive the same Python `str` and build the complete
root tree: `ElementTree.fromstring(xml)` versus `turboxml.read_string(xml, 'root')`.
These are different tree implementations; this measures their public APIs,
including conversion to Python nodes and destruction, not just tokenizer speed.
It excludes file I/O, traversal, serialization, and multi-thread throughput.
The synthetic inputs contain no mixed-content tails, DTDs, or namespaces;
equality here does not establish general API/semantic equivalence.

Each warmup and measured batch makes 10 calls by default. GC is disabled during
timing, and parser order alternates between measured batches. Median and p95 are
computed from 31 batch means in milliseconds per call. p95 uses nearest rank
(`ceil(0.95 * repeats)`); it is not individual-call tail latency. Speedup is the
ratio of medians, ElementTree / turboxml; below 1 means turboxml was slower.
JSON includes all samples, exact input sizes/hashes, interpreter/platform,
module path, package version and source revision. The source revision describes
the checkout, so install its freshly built wheel as above. Machine load, CPU,
Python version, compiler and build mode can materially change results.

Fixtures are deterministic: 10,000 records with attributes and two children;
100 sibling chains each containing 128 nested levels; and 32 text sections each
containing 4,096 repetitions of ASCII plus Ukrainian text. See `parsing.py` for
the exact input definitions. Re-run on your production documents before drawing
broader performance conclusions.

Text fixtures intentionally have no leading/trailing whitespace: turboxml trims
it while ElementTree preserves it. All fixtures are checked for identical tree
content before any timing. Equality failures stop the run instead of publishing
numbers for unequal outputs.

## Comparing two builds

For a paired comparison, build the baseline and optimized revisions separately
in release mode and pass their native extension paths to:

```sh
python benchmarks/compare.py --before /path/to/before/turboxml.so --after /path/to/after/turboxml.so
```

Use the same Python, Rust toolchain and build flags for both builds. Native
module filenames are platform-specific (`.so` on Linux, `.pyd` on Windows).
The helper uses the same fixtures, equality checks, 5 warmup batches and
31 measured batches of 10 calls, rotating all three parser orders. JSON output
is local under ignored `target/`; results are not checked into the repository.

## Recorded optimization comparison

CPython 3.12.14, Linux x86-64, AMD EPYC 9V74, 2026-10-07. Baseline source:
`9e3c37ce02ff6cd90ba010f16e7f59c86027cc74`. Both native extensions used the same
locked dependencies, release profile and `pyo3/extension-module` feature. Timings
include tree destruction; p95 describes batch means. Parser order rotates in
one process: 5 warmup batches, 31 measured batches, 10 calls per batch.

| Input | Bytes | Before median / p95 (ms) | After median / p95 (ms) | ET median / p95 (ms) | Before / after median | ET / after median |
|---|---:|---:|---:|---:|---:|---:|
| structured-attributes | 1,047,793 | 14.759 / 16.847 | 12.782 / 13.809 | 19.726 / 23.362 | 1.155× | 1.543× |
| deep-nested | 321,813 | 5.408 / 5.996 | 4.645 / 5.026 | 6.178 / 7.356 | 1.164× | 1.330× |
| text-heavy | 6,161,219 | 1.669 / 1.917 | 1.178 / 1.270 | 10.973 / 12.196 | 1.417× | 9.314× |

Shared virtualized environment; a single paired run is not a universal speedup
guarantee. These results do not establish a typical 1.5–2.5× structured-XML
speedup across documents and machines. No raw results JSON is tracked.

Optimized `src/read.rs` SHA-256: `240e0a821c71b5d615e4f6f5bf2032e532af8e1a4b9fa0025f0966c71dcf260c`.

## Shared-buffer string storage

Parsed tag names, attribute keys/plain values and single plain-text segments
refer to byte ranges in one immutable owned input buffer. There are no separate
Rust string allocations for these fields. Entity decoding, attribute whitespace
normalization and joining separate text segments still allocate owned buffers.
This is not a zero-allocation tree: the structural index, HashMaps, child vectors,
shared owner and Python Node objects still allocate. Python property access
materializes Python strings/dicts, and `to_dict()` materializes the whole tree.

A surviving node keeps the entire document buffer alive, including when a small
subtree was selected. To retain only a copied subtree, use
`copy = turboxml.Node.from_dict(node.to_dict())` and release the original nodes.
Nodes constructed with Node/Node.from_dict use independent owned strings.

## Recorded zero-copy comparison

Baseline: PR commit `8c22a02eca5c6f07979d8e8d214ca1085bf17a9a`. Both builds use
locked dependencies, release profile and pyo3/extension-module. CPython 3.12.14,
Linux x86-64, AMD EPYC 9V74, 2026-10-07; same-process rotating parser order,
5 warmup batches and 31 measured batches of 10 calls, including destruction.
Results are one run in a shared virtualized environment.

| Input | Before median / p95 (ms) | After median / p95 (ms) | ET median / p95 (ms) | Before / after median | ET / after median |
|---|---:|---:|---:|---:|---:|
| structured-attributes | 17.797 / 21.606 | 14.158 / 17.536 | 21.517 / 25.428 | 1.257× | 1.520× |
| deep-nested | 6.453 / 7.863 | 5.881 / 6.463 | 6.527 / 7.698 | 1.097× | 1.110× |
| text-heavy | 1.287 / 1.453 | 0.934 / 1.236 | 11.261 / 13.619 | 1.378× | 12.055× |

Also test public-field materialization so parse-only gains are not confused with
an entire consumer pipeline. This mode runs the same full-tree signature walk
for both builds and ElementTree, including every name, attribute and text:

```sh
python benchmarks/compare.py --before /path/to/before/turboxml.so --after /path/to/after/turboxml.so --traverse --iterations 1
```

Recorded with 5 warmup / 31 measured batches of 1 call:

| Input | Before median / p95 (ms) | After median / p95 (ms) | Before / after median |
|---|---:|---:|---:|
| structured-attributes | 54.072 / 73.102 | 51.580 / 72.130 | 1.048× |
| deep-nested | 21.530 / 28.604 | 21.814 / 27.672 | 0.987× |
| text-heavy | 4.845 / 5.686 | 4.366 / 4.859 | 1.110× |

Nested traversal is slightly slower in this run; the difference is small and
may be noise. The largest parse-only gains do not carry through unchanged to
full materialization. All JSON output remains local under ignored `target/`.

Zero-copy `src/read.rs` SHA-256: `9474ee090887c6f26c40fa033335d9380a1f6810a6f33aad62f9cc110670bb6a`.
