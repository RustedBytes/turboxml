#!/usr/bin/env python3
"""Compare two release extension builds with rotated parser order."""

import argparse
import gc
import importlib.util
import json
import math
import platform
import statistics
import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

from parsing import measure, signature, workloads


def load_extension(path):
    spec = importlib.util.spec_from_file_location("turboxml", str(path.resolve()))
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load extension from {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--before", type=Path, required=True)
    ap.add_argument("--after", type=Path, required=True)
    ap.add_argument(
        "--traverse",
        action="store_true",
        help="Include reading every field through the public API",
    )
    ap.add_argument("--iterations", type=int, default=10)
    ap.add_argument("--output", type=Path, default=Path("target/comparison.json"))
    args = ap.parse_args()
    if args.iterations < 1:
        ap.error("iterations must be positive")
    before = load_extension(args.before)
    after = load_extension(args.after)
    parsers = [
        ("before", lambda xml: before.read_string(xml, "root")),
        ("after", lambda xml: after.read_string(xml, "root")),
        ("ElementTree", ET.fromstring),
    ]
    rows = []
    for name, xml in workloads():
        expected = signature(ET.fromstring(xml), False)
        for label, parse in parsers:
            if signature(parse(xml), label != "ElementTree") != expected:
                raise ValueError(f"Unequal output: {name} / {label}")
        timed_parsers = parsers
        if args.traverse:
            timed_parsers = [
                (
                    label,
                    lambda xml, parse=parse, turbo=label != "ElementTree": signature(
                        parse(xml), turbo
                    ),
                )
                for label, parse in parsers
            ]
        samples = {label: [] for label, _ in parsers}
        gc.collect()
        enabled = gc.isenabled()
        gc.disable()
        try:
            for batch in range(36):
                order = timed_parsers[batch % 3 :] + timed_parsers[: batch % 3]
                for label, parse in order:
                    value = measure(parse, xml, args.iterations)
                    if batch >= 5:
                        samples[label].append(value)
        finally:
            if enabled:
                gc.enable()
        row = {
            "name": name,
            "input_bytes_utf8": len(xml.encode()),
            "parsers": {
                label: {
                    "median_ms": statistics.median(values),
                    "p95_ms": sorted(values)[math.ceil(0.95 * len(values)) - 1],
                    "samples_ms": values,
                }
                for label, values in samples.items()
            },
        }
        print(
            name,
            {label: p["median_ms"] for label, p in row["parsers"].items()},
            flush=True,
        )
        rows.append(row)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(
            {
                "python": sys.version,
                "platform": platform.platform(),
                "timestamp_utc": datetime.now(timezone.utc).isoformat(),
                "warmup_batches": 5,
                "repeats": 31,
                "iterations_per_batch": args.iterations,
                "metric": "parse, all public fields, destruction"
                if args.traverse
                else "parse and destruction",
                "before_module": str(args.before),
                "after_module": str(args.after),
                "workloads": rows,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
