"""Build the FL10.1 ledger from the pinned roadmap and explicit test evidence."""

import argparse
import csv
import json
from pathlib import Path
import subprocess
from io import StringIO


HERE = Path(__file__).resolve().parent
ROADMAP_COMMIT = "126114a9fa069194e2bd63ac8e8d46c327a35d83"
ROADMAP_DIR = "deliverables/dx11-levels"
TIER = "10_1"
FIELDS = (
    "id", "layer", "requirement", "classification", "native_value", "qualifiers",
    "owner", "hal_entry", "public_mapping", "test_id", "source",
    "candidate_sha", "native_result", "hal_result", "public_result",
    "devices", "test_command", "artifact_sha256", "notes",
)


def roadmap_rows(repo, pathname):
    source = subprocess.check_output(
        ["git", "-C", str(repo), "show", f"{ROADMAP_COMMIT}:{ROADMAP_DIR}/{pathname}"],
        encoding="utf-8",
    )
    return (row for row in csv.DictReader(StringIO(source)) if row["tier"] == TIER)


def native_requirements(repo):
    for row in roadmap_rows(repo, "native-guarantees.csv"):
        yield {
            key: row[key]
            for key in (
                "id", "requirement", "classification", "native_value", "qualifiers",
                "owner", "hal_entry", "public_mapping", "test_id", "source",
            )
        } | {"layer": "native-operation"}


def format_requirements(repo):
    for row in roadmap_rows(repo, "dxgi-format-support.csv"):
        yield {
            "id": row["id"],
            "layer": f"format-{row['source_scope']}",
            "requirement": f"{row['format']} / {row['use']}",
            "classification": row["status"],
            "qualifiers": row["condition"],
            "owner": row["owner"],
            "hal_entry": row["hal_entry"],
            "public_mapping": row["public_mapping"],
            "test_id": row["test"],
            "source": row["source"],
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roadmap_repository", type=Path)
    args = parser.parse_args()
    tests = json.loads((HERE / "tests.json").read_text(encoding="utf-8"))
    supplemental = tests["supplemental_requirements"]
    evidence = tests["requirement_evidence"]
    rows = list(native_requirements(args.roadmap_repository))
    rows += list(format_requirements(args.roadmap_repository))
    rows += supplemental
    ids = {row["id"] for row in rows}
    if len(ids) != len(rows):
        raise ValueError("Duplicate requirement ID")
    if missing := evidence.keys() - ids:
        raise ValueError(f"Unknown requirement IDs in evidence: {sorted(missing)}")

    with (HERE / "requirements.csv").open("w", encoding="utf-8", newline="") as output:
        writer = csv.DictWriter(output, fieldnames=FIELDS)
        writer.writeheader()
        for row in rows:
            writer.writerow({
                "candidate_sha": tests["source_sha"],
                "native_result": "unknown",
                "hal_result": "unknown",
                "public_result": "unknown",
            } | row | evidence.get(row["id"], {}))


if __name__ == "__main__":
    main()
