# Manual parsing benchmark

Build the checked-out source in release mode (do not benchmark `maturin develop`
without `--release`):

```sh
python -m venv .venv
. .venv/bin/activate
python -m pip install maturin
maturin build --release --locked --interpreter python --out dist
python -m pip install --force-reinstall dist/*.whl
python benchmarks/parsing.py --warmup 5 --repeats 31 --iterations 10 --output benchmarks/results.json
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

## Recorded run

CPython 3.12.14, Linux-6.18.44-x86_64-with-glibc2.39, AMD EPYC 9V74 80-Core Processor; release wheel built from
`9e3c37ce02ff6cd90ba010f16e7f59c86027cc74` on 2026-10-07.

5 warmup batches, 31 measured batches, 10 calls per batch. Times are ms/call.

| Input | UTF-8 bytes | Nodes | ET median | ET p95 | turboxml median | turboxml p95 | Median speedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| structured-attributes | 1,047,793 | 30,001 | 18.938 | 22.413 | 16.442 | 18.490 | 1.152× |
| deep-nested | 321,813 | 12,901 | 5.842 | 6.181 | 5.967 | 6.289 | 0.979× |
| text-heavy | 6,161,219 | 33 | 10.822 | 11.304 | 1.582 | 2.024 | 6.842× |

This is one run in a shared virtualized environment, not a universal speedup
guarantee. The nested input was slightly slower with turboxml. Raw batch samples
and environment/build provenance are in [results.json](results.json).
