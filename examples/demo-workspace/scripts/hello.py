#!/usr/bin/env python3
"""The Python half of the demo workspace's tour: reads data/sample.csv and
prints a tiny summary. Deliberately unfinished in one place."""

import csv
import sys
from pathlib import Path


def load_rows(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def main() -> None:
    data_path = Path(__file__).parent.parent / "data" / "sample.csv"
    rows = load_rows(data_path)

    print(f"{len(rows)} rows in {data_path.name}")
    for row in rows:
        print(f"  {row['name']}: {row['score']}")

    # TODO: also print the average score across all rows.
    # FIXME: this assumes every row has a "score" column that parses as a
    # number -- it doesn't validate that before use.


if __name__ == "__main__":
    main()
