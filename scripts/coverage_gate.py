#!/usr/bin/env python3
"""Coverage gate: parse a cargo-llvm-cov summary table (stdin) and enforce
per-crate + overall line-coverage floors. Exit 1 with a report on breach.

Floors are a ratchet, set just under measured coverage (2026-09, full
suite, main.rs entrypoints excluded): they catch a change that tanks one
crate without demanding busywork. Raise them as coverage grows.
"""

import re
import sys

# crate -> minimum line coverage %
FLOORS = {
    "licensing-core": 95,
    "auth_service": 80,
    "platform": 78,
    "license_service": 74,
    "movie_service": 70,
    "notification_service": 70,
    "search_service": 70,
    "song_service": 65,
    "test-support": 90,
}
OVERALL_FLOOR = 75

ROW = re.compile(r"^(crates|services)/([a-z_-]+)/")


def main() -> int:
    crates: dict[str, list[int]] = {}
    for line in sys.stdin:
        match = ROW.match(line)
        if not match:
            continue
        crate = match.group(2)
        parts = line.split()
        # columns: file, total_lines, missed, line%
        total, missed = int(parts[1]), int(parts[2])
        bucket = crates.setdefault(crate, [0, 0])
        bucket[0] += total
        bucket[1] += missed

    if not crates:
        print("coverage gate: no rows parsed — did llvm-cov run?", file=sys.stderr)
        return 1

    failed = False
    print(f"{'crate':22} {'coverage':>9} {'floor':>6}")
    for crate in sorted(crates):
        total, missed = crates[crate]
        coverage = 100 * (total - missed) / max(1, total)
        floor = FLOORS.get(crate)
        if floor is None:
            print(f"{crate:22} {coverage:8.1f}%     —")
            continue
        mark = "ok" if coverage >= floor else "FAIL"
        if coverage < floor:
            failed = True
        print(f"{crate:22} {coverage:8.1f}% {floor:5}%  {mark}")

    grand_total = sum(c[0] for c in crates.values())
    grand_missed = sum(c[1] for c in crates.values())
    overall = 100 * (grand_total - grand_missed) / max(1, grand_total)
    mark = "ok" if overall >= OVERALL_FLOOR else "FAIL"
    if overall < OVERALL_FLOOR:
        failed = True
    print(f"{'TOTAL':22} {overall:8.1f}% {OVERALL_FLOOR:5}%  {mark}")

    if failed:
        print("\ncoverage floors breached — raise tests or lower floors deliberately", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
