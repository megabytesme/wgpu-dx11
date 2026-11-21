#!/usr/bin/env python3
"""Rebuild the retained FL9_1 CTS skip inventory from pinned receipts."""

import argparse
import csv
import hashlib
import json
import re
from pathlib import Path


SUMMARY = re.compile(r"^== Summary for (.+) ==$")
COUNTS = {
    "passed_without_warnings": re.compile(r"^Passed\s+w/o warnings\s*=\s*(\d+)\s*/\s*(\d+)"),
    "passed_with_warnings": re.compile(r"^Passed with warnings\s*=\s*(\d+)\s*/\s*(\d+)"),
    "skipped": re.compile(r"^Skipped\s*=\s*(\d+)\s*/\s*(\d+)"),
    "failed": re.compile(r"^Failed\s*=\s*(\d+)\s*/\s*(\d+)"),
}


def sha256(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def parse_transcript(path):
    rows = []
    current = None
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        match = SUMMARY.match(line)
        if match:
            if current:
                rows.append(current)
            current = {"selector": match.group(1)}
            continue
        if current:
            for key, pattern in COUNTS.items():
                match = pattern.match(line)
                if match:
                    current[key] = int(match.group(1))
                    current[key + "_total"] = int(match.group(2))
    if current:
        rows.append(current)
    return rows



def excluded_selectors(path):
    # Matches xtask/src/cts.rs TEST_LINE_REGEX and exact backend membership.
    pattern = re.compile(r"(?:fails-if\s*\(\s*(?P<conditions>\w+(?:,\w+)*?)\s*\)\s+)?(?P<selector>.*)")
    rows = []
    total = 0
    for number, original in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        trimmed = original.strip()
        if not trimmed or trimmed.startswith(("//", "#")):
            continue
        total += 1
        match = pattern.match(trimmed)
        conditions = match.group("conditions") or ""
        if "dx11" not in conditions.split(","):
            continue
        selector = match.group("selector")
        lower = selector.lower()
        if ",validation," in lower or ":validation" in lower:
            family = "validation"
            disposition = "VALIDATION_REQUIRED_REVIEW"
        elif any(term in lower for term in ("texture_compression", "bc1", "bc2", "bc3", "bc4", "bc5", "bc6", "bc7", "texturecompression")):
            family = "texture_compression"
            disposition = "MIXED_CAPABILITIES_EXPAND_CASES"
        elif any(term in lower for term in ("compute", "storage_texture", "storagebuffer", "storage_buffer", "workgroup", "atomic")):
            family = "compute_storage"
            disposition = "MIXED_CAPABILITIES_EXPAND_CASES"
        elif any(term in lower for term in ("texture", "sampler", "sampling", "image_copy")):
            family = "textures_sampling_copy"
            disposition = "MANDATORY_REVIEW_MIXED_CASES"
        elif any(term in lower for term in ("render", "blend", "depth", "stencil", "multisample", "color")):
            family = "render_pipeline_depth_stencil"
            disposition = "MANDATORY_REVIEW_MIXED_CASES"
        elif any(term in lower for term in ("buffer", "bind_group", "bindgroup")):
            family = "buffers_and_bindings"
            disposition = "MANDATORY_REVIEW_MIXED_CASES"
        elif any(term in lower for term in ("shader", "pipeline", "vertex", "fragment")):
            family = "shader_pipeline"
            disposition = "MANDATORY_REVIEW_MIXED_CASES"
        else:
            family = "general_or_unclassified"
            disposition = "MANDATORY_REVIEW"
        rows.append({"line": number, "original_line": original, "conditions": conditions,
                     "selector": selector, "capability_family": family, "disposition": disposition,
                     "reason": "Triage only; expand selector to cases, distinguish unsupported positive operations from required validation and mandatory behavior, then retain evidence."})
    return total, rows

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--tests-json", type=Path, required=True)
    parser.add_argument("--transcript", type=Path, required=True)
    parser.add_argument("--test-list", type=Path, required=True)
    parser.add_argument("--cts-revision", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    receipt = json.loads(args.tests_json.read_text(encoding="utf-8"))
    recorded = receipt["cts_curated_candidate"]
    run = parse_transcript(args.transcript)
    total = {key: sum(row.get(key, 0) for row in run) for key in COUNTS}
    expected = recorded["case_occurrence_counts_in_transcripts"]
    checks = {
        "selector_summaries": len(run) == recorded["completed_selectors"],
        "passed": total["passed_without_warnings"] == expected["Passed  w/o warnings"],
        "warnings": total["passed_with_warnings"] == expected["Passed with warnings"],
        "skipped": total["skipped"] == expected["Skipped"],
        "failed": total["failed"] == expected["Failed"],
        "test_list_sha256": sha256(args.test_list) == recorded["list_sha256"],
        "cts_revision": args.cts_revision.read_text(encoding="utf-8").strip() == recorded["suite_revision"],
    }

    args.out.mkdir(parents=True, exist_ok=True)
    with (args.out / "runtime-skips.csv").open("w", newline="", encoding="utf-8") as stream:
        fields = ["selector", "skipped", "selector_case_total", "disposition", "reason"]
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        for row in run:
            if row.get("skipped", 0):
                writer.writerow({
                    "selector": row["selector"],
                    "skipped": row["skipped"],
                    "selector_case_total": row.get("skipped_total", ""),
                    "disposition": "UNRESOLVED_MANDATORY_REVIEW",
                    "reason": "Summary transcript has no per-case outcome or skip reason; preserve as required until case-level evidence proves inapplicable.",
                })

    list_rows, excluded = excluded_selectors(args.test_list)
    checks["excluded_count"] = len(excluded) == recorded["dx11_list_exclusions"]
    checks["list_partition"] = list_rows == len(run) + len(excluded)
    with (args.out / "excluded-selectors.csv").open("w", newline="", encoding="utf-8") as stream:
        fields = ["line", "original_line", "conditions", "selector", "capability_family", "disposition", "reason"]
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(excluded)

    summary = {
        "state": "INCOMPLETE",
        "classification_policy": "Conservative: no skip is waived without per-case reason and capability applicability evidence.",
        "source_pins": {
            "wgpu_candidate": receipt["sources"]["wgpu"],
            "wgpu_baseline": receipt["wgpu_baseline"],
            "cts_revision": recorded["suite_revision"],
        },
        "inputs": {
            "tests_json": {"path": str(args.tests_json), "sha256": sha256(args.tests_json)},
            "transcript": {"path": str(args.transcript), "sha256": sha256(args.transcript)},
            "test_list": {"path": str(args.test_list), "sha256": sha256(args.test_list)},
            "cts_revision_file": {"path": str(args.cts_revision), "sha256": sha256(args.cts_revision)},
        },
        "counts": {
            "selector_summaries": len(run),
            "runtime_skipped_cases": total["skipped"],
            "selectors_with_skips": sum(bool(row.get("skipped", 0)) for row in run),
            "runtime_skip_disposition": "UNRESOLVED_MANDATORY_REVIEW",
            "excluded_selectors_reported_by_receipt": recorded["dx11_list_exclusions"],
            "test_list_noncomment_rows": list_rows,
            "excluded_selectors_extracted": len(excluded),
            "excluded_selector_disposition": "UNRESOLVED_CASE_LEVEL_REVIEW",
        },
        "checks": checks,
        "limitations": [
            "The retained CTS transcript contains selector summaries only; it does not identify the exact skipped subcases or why they skipped.",
            "Selectors are reconstructed from pinned test.lst and the xtask exact backend exclusion rule. Classification is heuristic triage, not proof of individual-case applicability; upstream selectors outside this list are not audited.",
            "The transcript SHA is recorded independently; the receipt's full_transcript_sha256 refers to its original captured run transcript and differs from the retained summary log when logs were transformed.",
        ],
    }
    (args.out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    if not all(checks.values()):
        raise SystemExit("provenance/count check failed; inspect summary.json")
    print(json.dumps(summary["counts"], indent=2))


if __name__ == "__main__":
    main()
