#!/usr/bin/env python3
"""Validate the per-tier dependency graph against roadmap matrices and live candidates."""

import csv
from datetime import datetime, timezone
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASE = "aa7b65c43ef008d0a92d20fcdf9f10fa0883f157"
COORD = Path(r"C:\dev\personal\HorizonCodexCoord-Nov2026")
LANES = {
    "FL9_1": Path(r"C:\dev\personal\wgpu-dx11-FL91-cert-Nov21"),
    "FL9_2": Path(r"C:\dev\personal\wgpu-dx11-FL92-Nov21"),
}
ORDER = ["FL9_1", "FL9_2", "FL9_3", "FL10_0", "FL10_1", "FL11_0", "FL11_1"]


def git(path, *args):
    return subprocess.check_output(["git", "-C", str(path), *args], text=True).strip()


def matrix_rows(name):
    with (ROOT / "deliverables" / "dx11-levels" / name).open(encoding="utf-8-sig", newline="") as f:
        return list(csv.DictReader(f))


def file_sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def paths_from_entry(entry):
    result = set()
    for token in entry.replace("::", " ").replace(",", " ").split():
        token = token.strip("`()")
        if token.endswith(".rs"):
            if token.startswith("wgpu-"):
                result.add(token)
            elif "/" in token:
                result.add("wgpu-hal/src/dx11/" + token.split("/")[-1])
            else:
                result.add("wgpu-hal/src/dx11/" + token)
    return sorted(result)


def target_paths(row):
    paths = paths_from_entry(row["hal_entry"])
    if paths:
        return paths, "matrix_hal_entry"
    feature = (row["requirement"] + " " + row["native_value"]).lower()
    if any(word in feature for word in ("sampler", "mirror_once", "address", "border")):
        return ["wgpu-hal/src/dx11/hal_sampler.rs", "wgpu-hal/src/dx11/hal_shader.rs"], "inferred_target_module"
    if any(word in feature for word in ("geometry", "tessellation", "shader", "stream_output", "adjacency", "profile")):
        return ["wgpu-hal/src/dx11/hal_pipeline.rs", "wgpu-hal/src/dx11/hal_shader.rs"], "inferred_target_module"
    if any(word in feature for word in ("query", "occlusion")):
        return ["wgpu-hal/src/dx11/query.rs", "wgpu-hal/src/dx11/hal_encoder.rs"], "inferred_target_module"
    if any(word in feature for word in ("mip", "format", "texture", "bgrx", "volume", "cube", "render_xr")):
        return ["wgpu-hal/src/dx11/texture.rs", "wgpu-hal/src/dx11/hal_device.rs", "wgpu-hal/src/dx11/capabilities.rs"], "inferred_target_module"
    return ["wgpu-hal/src/dx11/hal_pipeline.rs", "wgpu-hal/src/dx11/hal_encoder.rs"], "inferred_target_module"


def candidate_info(tier):
    lane = LANES.get(tier)
    if not lane:
        return {"state": "NOT_YET_ASSIGNED", "base": BASE}
    head = git(lane, "rev-parse", "HEAD")
    branch_paths = set(git(lane, "diff", "--name-only", f"{BASE}..{head}").splitlines())
    working_paths = set(git(lane, "diff", "--name-only", "HEAD").splitlines())
    working_paths.update(git(lane, "ls-files", "--others", "--exclude-standard").splitlines())
    paths = branch_paths | working_paths
    dirty = git(lane, "status", "--porcelain")
    return {"state": "LIVE_CANDIDATE_INCOMPLETE", "worktree": str(lane), "head": head,
            "base": BASE, "committed_changed_paths": sorted(branch_paths),
            "working_tree_changed_paths": sorted(working_paths),
            "changed_paths": sorted(paths), "working_tree_dirty": bool(dirty)}


def main():
    ops = matrix_rows("native-guarantees.csv")
    fmts = matrix_rows("dxgi-format-support.csv")
    by_tier_op = {tier: [r for r in ops if f"FL{tier[2:]}." in r["id"]] for tier in ORDER}
    by_tier_fmt = {tier: [r for r in fmts if r["id"].startswith(tier + ".")] for tier in ORDER}
    candidates = {tier: candidate_info(tier) for tier in ORDER}
    overlap = {}
    for a, b in [("FL9_1", "FL9_2")]:
        overlap[f"{a}__{b}"] = sorted(set(candidates[a]["changed_paths"]) & set(candidates[b]["changed_paths"]))

    tiers = []
    for index, tier in enumerate(ORDER):
        prev = ORDER[index - 1] if index else None
        feat_rows = by_tier_op[tier]
        format_rows = by_tier_fmt[tier]
        format_groups = {}
        for row in format_rows:
            key = (row["source_scope"], row["use"])
            group = format_groups.setdefault(key, {"source_scope": key[0], "use": key[1],
                "count": 0, "requirement_ids": [], "owners": set(), "module_paths": set()})
            group["count"] += 1
            group["requirement_ids"].append(row["id"])
            group["owners"].add(row["owner"])
            group["module_paths"].update(paths_from_entry(row["hal_entry"]))
        format_group_rows = [{
            "group_id": f"{tier}.FORMAT.{scope}.{use}", "source_scope": scope, "use": use,
            "count": group["count"], "requirement_ids": sorted(group["requirement_ids"]),
            "owners": sorted(group["owners"]), "module_paths": sorted(group["module_paths"]),
        } for (scope, use), group in sorted(format_groups.items())]
        module_index = {}
        for row in feat_rows:
            module_paths, _ = target_paths(row)
            for path in module_paths:
                item = module_index.setdefault(path, {"path": path, "feature_requirement_ids": [], "format_group_ids": set()})
                item["feature_requirement_ids"].append(row["id"])
        for group in format_group_rows:
            for path in group["module_paths"]:
                item = module_index.setdefault(path, {"path": path, "feature_requirement_ids": [], "format_group_ids": set()})
                item["format_group_ids"].add(group["group_id"])
        prerequisite_groups = []
        for earlier in ORDER[:index]:
            prerequisite_groups.append(f"{earlier}.OP")
            prerequisite_groups.extend(g["group_id"] for g in next(t for t in tiers if t["tier"] == earlier)["format_requirement_groups"])
        serialized_paths = set(overlap.get("FL9_1__FL9_2", []))
        tiers.append({
            "tier": tier,
            "predecessor": prev,
            "requires_certified_tiers": ORDER[:index],
            "prerequisite_requirement_groups": prerequisite_groups,
            "implementation_owner": {"role": "dedicated " + tier + " implementation lane", "assigned": tier in LANES},
            "candidate": candidates[tier],
            "gates": {
                "engineering_before_predecessor_gate": "Allowed on an isolated tier branch; keep capability advertisement, default selection and release wiring disabled.",
                "merge_or_promotion_requires": ([f"Mission Control certifies {prev} at an immutable SHA"] if prev else []) + [
                    "Rebase or cherry-pick the tier work onto the exact accepted predecessor SHA in a fresh integration worktree.",
                    "Acquire the shared-module lease for overlapping files; the serialized shared-interface owner reviews and lands common changes.",
                    "Run every inherited exact-tier gate and the current tier's positive/negative native, HAL and public checks on the resulting candidate SHA.",
                    "Promote only after zero mandatory requirements remain open; preserve the feature disabled until its gate passes."],
                "default_enabled": False,
            },
            "feature_requirements": [{
                "id": r["id"], "feature": r["requirement"], "class": r["classification"],
                "owner": r["owner"], "native_value": r["native_value"], "qualifiers": r["qualifiers"],
                "module_paths": target_paths(r)[0], "module_path_basis": target_paths(r)[1], "hal_entry": r["hal_entry"],
                "public_mapping": r["public_mapping"], "test_id": r["test_id"],
                "depends_on_gate": f"certified:{prev}" if prev else "baseline:FL9_1",
            } for r in feat_rows],
            "format_requirement_groups": format_group_rows,
            "module_ownership": [{
                "path": row["path"],
                "owner_role": "Mission Control-designated integration owner" if row["path"] in serialized_paths else "dedicated " + tier + " implementation lane",
                "serialized_shared_path": row["path"] in serialized_paths,
                "feature_requirement_ids": sorted(row["feature_requirement_ids"]),
                "format_group_ids": sorted(row["format_group_ids"]),
            } for row in sorted(module_index.values(), key=lambda x: x["path"])],
            "format_requirement_count": len(format_rows),
        })

    doc = {
        "schema_version": 1,
        "state": "INCOMPLETE",
        "captured_utc": datetime.now(timezone.utc).isoformat(),
        "source_matrices": {
            "native_operations": "native-guarantees.csv",
            "format_uses": "dxgi-format-support.csv",
            "matrix_sha256": {
                "native-guarantees.csv": file_sha256(ROOT / "deliverables" / "dx11-levels" / "native-guarantees.csv"),
                "dxgi-format-support.csv": file_sha256(ROOT / "deliverables" / "dx11-levels" / "dxgi-format-support.csv"),
            },
        },
        "tier_order": ORDER,
        "shared_interface_policy": {
            "serialized_owner": "Mission Control-designated integration owner",
            "status": "Owner must be named before concurrent edits to shared adapter/device/capability/shader interfaces.",
            "lease_scope": "Any shared source path listed in candidate overlaps or feature module_paths.",
            "current_exact_overlaps": overlap,
            "observed_candidates": {tier: candidates[tier] for tier in candidates if tier in LANES},
            "conflict_resolution": "Do not merge divergent tier branches wholesale. After predecessor certification, transplant the next tier's isolated changes onto the immutable accepted predecessor, serialize overlapping modules, and inspect the resulting diff against both parent candidates.",
        },
        "tiers": tiers,
        "interpretation": {
            "native_hal_public_layers": "Each operation retains the native requirement, backend module path(s), HAL entry and public mapping separately. A native guarantee without a portable public mapping is still mandatory; it needs a reachable backend-owned extension or remains an open blocker.",
            "format_groups": "Each group contains exact stable row IDs from dxgi-format-support.csv, partitioned by tier, source scope and use.",
            "unavailable_or_conditional": "Never reinterpret unavailable/conditional as certified. Follow its matrix classification, qualifiers and evidence status.",
        },
    }
    target = ROOT / "deliverables" / "dx11-levels" / "feature-dependencies.json"
    target.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {target}; OP={len(ops)} format={len(fmts)}")
    print(json.dumps(overlap, indent=2))


if __name__ == "__main__":
    main()
