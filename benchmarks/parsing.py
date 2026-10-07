#!/usr/bin/env python3
"""Manual full-tree parsing benchmark; no third-party benchmark dependencies."""
import argparse
import gc
from datetime import datetime, timezone
import hashlib
import importlib.metadata
import json
import math
import platform
import statistics
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

import turboxml


def workloads():
    yield 'structured-attributes', '<root>' + ''.join(
        f'<item id="{i}" kind="record" active="true"><name>Item {i}</name>'
        '<value unit="UAH">123.45</value></item>' for i in range(10000)
    ) + '</root>'
    yield 'deep-nested', '<root>' + ('<level index="1">' * 128 + '<leaf>value</leaf>'
        + '</level>' * 128) * 100 + '</root>'
    yield 'text-heavy', '<root>' + ''.join(
        '<section id="%d">%s</section>' % (i, ('Plain XML text with Ukrainian: Україна. ' * 4096).rstrip())
        for i in range(32)
    ) + '</root>'


def signature(root, turbo):
    # Iterative traversal avoids Python recursion limits; preserve child order.
    result = []
    stack = [root]
    while stack:
        node = stack.pop()
        result.append((node.name if turbo else node.tag,
                       sorted((node.attrs if turbo else node.attrib).items()),
                       node.text or ''))
        stack.extend(reversed(node.children if turbo else list(node)))
    return result


def measure(parse, xml, iterations):
    start = time.perf_counter_ns()
    for _ in range(iterations):
        root = parse(xml)
        del root  # Include full-tree destruction equally for both parsers.
    return (time.perf_counter_ns() - start) / iterations / 1e6


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--warmup', type=int, default=5)
    ap.add_argument('--repeats', type=int, default=31)
    ap.add_argument('--iterations', type=int, default=10)
    ap.add_argument('--output', type=Path, default=Path('target/parsing-results.json'))
    args = ap.parse_args()
    if args.warmup < 0 or args.repeats < 20 or args.iterations < 1:
        ap.error('warmup >= 0, repeats >= 20 and iterations >= 1 are required')
    try:
        revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        revision = None
    cpu = platform.processor()
    if sys.platform.startswith('linux'):
        for line in Path('/proc/cpuinfo').read_text().splitlines():
            if line.startswith('model name'):
                cpu = line.split(':', 1)[1].strip()
                break
    report = {'timestamp_utc': datetime.now(timezone.utc).isoformat(),
              'benchmark_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'python': sys.version, 'implementation': platform.python_implementation(),
              'platform': platform.platform(), 'machine': platform.machine(),
              'processor': cpu, 'turboxml_version': importlib.metadata.version('turboxml'),
              'turboxml_module': turboxml.__file__, 'source_revision': revision,
              'warmup_batches': args.warmup, 'repeats': args.repeats,
              'iterations_per_batch': args.iterations, 'gc': 'disabled during timing',
              'metric': 'parse plus tree destruction; ms per call; nearest-rank p95 of batch means',
              'workloads': []}
    parsers = [('ElementTree', ET.fromstring), ('turboxml', lambda xml: turboxml.read_string(xml, 'root'))]
    for name, xml in workloads():
        reference = signature(ET.fromstring(xml), False)
        assert signature(turboxml.read_string(xml, 'root'), True) == reference, name
        samples = {name: [] for name, _ in parsers}
        gc.collect()
        enabled = gc.isenabled()
        gc.disable()
        try:
            for _ in range(args.warmup):
                for _, parse in parsers:
                    measure(parse, xml, args.iterations)
            for repeat in range(args.repeats):
                # Alternate order to reduce systematic order/thermal bias.
                for parser, parse in parsers[::1 if repeat % 2 == 0 else -1]:
                    samples[parser].append(measure(parse, xml, args.iterations))
        finally:
            if enabled:
                gc.enable()
        row = {'name': name, 'input_bytes_utf8': len(xml.encode()),
               'input_characters': len(xml), 'sha256_utf8': hashlib.sha256(xml.encode()).hexdigest(),
               'nodes': len(reference), 'parsers': {}}
        for parser, values in samples.items():
            row['parsers'][parser] = {'median_ms': statistics.median(values),
                'p95_ms': sorted(values)[math.ceil(.95 * len(values)) - 1], 'samples_ms': values}
        row['speedup_median'] = row['parsers']['ElementTree']['median_ms'] / row['parsers']['turboxml']['median_ms']
        report['workloads'].append(row)
        print(f"{name}: {row['input_bytes_utf8']} bytes, speedup {row['speedup_median']:.3f}x", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
