# turboxml

[![PyPI version](https://img.shields.io/pypi/v/turboxml?color=%2334D058&label=pypi%20package)](https://pypi.org/project/turboxml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A fast, lightweight Python library for reading and writing XML files, powered by Rust.

In a [recorded manual benchmark](benchmarks/README.md#recorded-optimization-comparison) on CPython 3.12.14 / Linux x86-64, `turboxml` achieved **1.54×** the throughput of `xml.etree.ElementTree` for structured XML with attributes, **1.33×** for deeply nested XML, and **9.31×** for text-heavy XML. These are three synthetic workloads on one machine, not a typical-speedup guarantee; results depend on the document, Python version, hardware, and build. The benchmark measures full-tree parsing and destruction through each library's public API.

---

## Features

- **Rust-powered parsing** - See the [reproducible benchmark](benchmarks/README.md) for measured results, input definitions, and limitations.
- **Simple API** - Read, traverse, and write XML with minimal boilerplate.
- **Thread-friendly** - Parsing and writing release the GIL, so multiple documents can be processed in parallel from Python threads.
- **Clear errors** - Malformed XML raises `ValueError` with position information; a missing file raises `FileNotFoundError`.
- **Type-safe** - Ships with a `.pyi` stub file for full editor autocompletion and type checking.
- **Cross-platform** - Wheels for CPython 3.10+ and PyPy 3.11 / 3.12 on Windows, macOS, and Linux.

## Installation

```bash
pip install turboxml
```

## Quick Start

### Reading XML

Given an XML file `note.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<note example_attr="example value">
    <to>
        <name>Example Name</name>
    </to>
    <from>
        <name>Example Name</name>
    </from>
    <heading>An Example Heading</heading>
    <body>An Example Body!</body>
</note>
```

Parse it with `turboxml`:

```python
from turboxml import read_file

root = read_file("note.xml", "note")

for child in root.children:
    print(child.name, child.text)
```

### Writing XML

```python
from turboxml import Node, write_file

node = Node(
    name="greeting",
    attrs={"lang": "en"},
    text="Hello, World!",
)
write_file(node, "greeting.xml")
```

### The `Node` Object

Every parsed element is represented as a `Node`:

```python
class Node:
    name: str                # Tag name
    attrs: dict[str, str]    # Element attributes
    children: list[Node]     # Child nodes
    text: str | None         # Text content, if any
```

`children` and `search` results are shared references to the same node objects, not copies — repeated access and traversal are cheap.

Refer to the [`turboxml.pyi`](turboxml.pyi) stub file for the complete API surface, including `read_string`, `write_string`, and additional utilities.

## Development

`turboxml` is built with [PyO3](https://pyo3.rs) and [Maturin](https://www.maturin.rs/).

### Prerequisites

- Python 3.10+
- Rust toolchain (stable)
- [Maturin](https://www.maturin.rs/) (`pip install maturin`)

### Building from Source

```bash
git clone https://github.com/RustedBytes/turboxml.git
cd turboxml
python -m venv .venv && source .venv/bin/activate
pip install maturin
maturin develop
```

### Running Tests

```bash
cargo test
```

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on [GitHub](https://github.com/RustedBytes/turboxml).

## License

This project is licensed under the [MIT License](LICENSE).
