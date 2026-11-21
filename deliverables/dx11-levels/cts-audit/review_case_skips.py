"""Reconcile archived case IDs and classify the unresolved FL9_1 CTS skips."""
import argparse
from collections import Counter
import csv
from hashlib import sha256
import io
import json
from pathlib import Path
import re
import subprocess

WGPU = "dbdc67913fcc858924ca553f68764fd8b432bd70"
CTS = "3fdd32363410f2bd4e3e2f046980967e6abc27e6"
RECEIPT = "b532abe5b59f8f057f4508756d0ef9e6bc71cb3c"
ROOT = "deliverables/dx11-levels/FL9_1"
SKIP = re.compile(r"^\[skip\] (.+?) \([0-9.]+ms\)\. Log:$")
FORMATS = {
    "r32sint": "R32_SINT", "r32uint": "R32_UINT", "rg11b10ufloat": "R11G11B10_FLOAT",
    "rg32float": "R32G32_FLOAT", "rg32sint": "R32G32_SINT", "rg32uint": "R32G32_UINT",
    "rgba32float": "R32G32B32A32_FLOAT", "rgba32sint": "R32G32B32A32_SINT",
    "rgba32uint": "R32G32B32A32_UINT",
}
ATTACHMENTS = "api/validation/render_pass/attachment_compatibility.spec.ts"
INTERPOLATE = "shader/validation/shader_io/interpolate.spec.ts"
ANCHORS = {
    "bare_vertex": (INTERPOLATE, "t.skip('vertex output must include a position builtin"),
    "bundle_limit": (ATTACHMENTS, "t.skipIf(passAttachments.length > maxColorAttachments)"),
    "encoder_limit": (ATTACHMENTS, "t.skipIf(encoderAttachments.length > maxColorAttachments)"),
    "offscreen": ("util/create_elements.ts", "test.skip('Cannot create an OffscreenCanvas')"),
    "onscreen": ("util/create_elements.ts", "test.skip('Cannot create HTMLCanvasElement')"),
    "language": ("shader/validation/decl/context_dependent_resolution.spec.ts",
                 "t.skipIf(!t.hasLanguageFeature(t.params.case), 'Missing language feature')"),
    "attachment_format": ("gpu_test.ts", "is not usable as a render attachment"),
    "resolve": ("api/validation/render_pass/render_pass_descriptor.spec.ts",
                "g.test('resolveTarget,format_supports_resolve')"),
}


def blob(repo, revision, path):
    return subprocess.check_output(["git", "-C", str(repo), "show", f"{revision}:{path}"])


def anchor(repo, revision, path, text):
    for index, line in enumerate(blob(repo, revision, path).decode("utf-8").splitlines(), 1):
        if text in line:
            return f"{path}:{index}"
    raise ValueError(f"Pinned source condition is missing: {text}")


def read_inventory(lane):
    receipt = json.loads(blob(lane, RECEIPT, f"{ROOT}/tests.json"))
    followup = receipt["followup"]
    if followup["cts_audit_input_sha"] != WGPU or receipt["cts_curated_candidate"]["suite_revision"] != CTS:
        raise ValueError("Unexpected source revision in lane receipt")
    inventory = blob(lane, RECEIPT, f"{ROOT}/cts-runtime-skips.csv")
    digest = sha256(inventory).hexdigest()
    recorded = next(i["sha256"] for i in followup["cts_audit"] if i["path"] == "cts-runtime-skips.csv")
    if digest != recorded:
        raise ValueError("Lane case inventory failed its recorded digest")
    return list(csv.DictReader(io.StringIO(inventory.decode("utf-8-sig")))), digest


def read_captures(logs, lane):
    counts, locations, artifacts = Counter(), {}, []
    for name in ("cts-x64-candidate-prefix.log", "cts-x64-continuation.log"):
        data = (logs / name).read_bytes()
        receipt_name = Path(name).with_suffix(".json").name
        receipt = json.loads(blob(lane, RECEIPT, f"{ROOT}/evidence/{receipt_name}"))
        digest = sha256(data).hexdigest()
        if digest != receipt["full_transcript_sha256"]:
            raise ValueError(f"Archived transcript does not match the committed digest: {name}")
        local = Counter()
        for line_number, line in enumerate(data.decode("utf-8-sig").splitlines(), 1):
            if match := SKIP.fullmatch(line):
                local[match[1]] += 1
                locations.setdefault(match[1], (name, line_number))
        counts.update(local)
        artifacts.append({"file": name, "sha256": digest, "runner_sha256": receipt["runner_sha256"],
                          "raw_skip_blocks": local.total(), "unique_case_ids": len(local)})
    return counts, locations, artifacts


def review(row, sources, formats):
    selector, reason = row["selector"], row["reason"]
    if ("vertex output must include a position builtin" in reason
            and 'stage="vertex"' in selector and "use_struct=false" in selector):
        return ("GENERATOR_PRECONDITION", sources["bare_vertex"],
                "Generated bare vertex return cannot supply position. Preserve legal struct cases.")
    if "color_sparse:" in selector and int(re.search(r"attachmentCount=(\d+)", selector)[1]) > 1:
        key = "bundle_limit" if "render_pass_and_bundle," in selector else "encoder_limit"
        return ("ADAPTER_LIMIT_PRECONDITION", f"{sources[key]} / {sources['limit']}",
                "The fixture requires an attachment array above the reported limit. Keep above-limit rejection in scope.")
    for kind, message in (("offscreen", "Cannot create an OffscreenCanvas"),
                          ("onscreen", "Cannot create HTMLCanvasElement")):
        if message in reason:
            return ("BROWSER_FIXTURE_REQUIRED", sources[kind],
                    "Needs a browser with this backend to check missing-feature canvas configuration.")
    if "Missing language feature" in reason and 'case="unrestricted_pointer_parameters"' in selector:
        return ("WGSL_EXTENSION_PRECONDITION", sources["language"],
                "Track extension availability and negative shader validation independently.")
    if "rg11b10ufloat is not usable as a render attachment" in reason:
        row_id = "FL9_1.DXGI.DXGI_FORMAT_R11G11B10_FLOAT.color_attachment"
        if formats[row_id] == "unavailable":
            return ("FORMAT_ATTACHMENT_PRECONDITION", f"{sources['attachment_format']} / {row_id}",
                    "This render format is unavailable in the application table. Keep negative format validation in scope.")
    if row["applicability"] == "RUNTIME_CONDITIONAL_FORMAT_SAMPLE_COUNT":
        native = FORMATS[re.search(r'format="([^"]+)"', selector)[1]]
        keys = [f"FL9_1.DXGI.DXGI_FORMAT_{native}.{use}" for use in ("msaa_4x", "multisample_resolve")]
        label = "DXGI_TABLE_LACKS_4X_AND_RESOLVE" if all(formats[k] == "unavailable" for k in keys) else "OPEN_FORMAT_MSAA_CONTRACT"
        evidence = " / ".join([sources["resolve"], *(f"{k}={formats[k]}" for k in keys)])
        return (label, evidence,
                "The fixture must first create a legal 4x source. Keep unsupported-combination validation and native format controls.")
    return "UNRESOLVED", "", "Inspect exact case parameters and pinned sources."


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lane", type=Path, required=True)
    parser.add_argument("--logs", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    inventory, digest = read_inventory(args.lane)
    counts, locations, artifacts = read_captures(args.logs, args.lane)
    ids = {row["selector"] for row in inventory}
    if len(inventory) != len(ids) or len(ids) != 4720 or ids != set(counts) or set(counts.values()) != {2}:
        raise ValueError("CTS case inventory and detailed captures disagree")
    sources = {key: anchor(args.lane / "cts", CTS, f"src/webgpu/{path}", text)
               for key, (path, text) in ANCHORS.items()}
    sources["limit"] = anchor(args.lane, WGPU, "wgpu-hal/src/dx11/capabilities.rs",
                              "assert_eq!(exposed.limits.max_color_attachments, 1)")
    matrix = Path(__file__).resolve().parent.parent / "dxgi-format-support.csv"
    with matrix.open(encoding="utf-8", newline="") as stream:
        formats = {row["id"]: row["status"] for row in csv.DictReader(stream) if row["source_scope"] == "DXGI"}
    reviewed = []
    for row in inventory:
        if row["applicability"] not in ("UNRESOLVED_PARENT_SELECTOR", "RUNTIME_CONDITIONAL_FORMAT_SAMPLE_COUNT"):
            continue
        disposition, source, action = review(row, sources, formats)
        name, line = locations[row["selector"]]
        reviewed.append(dict(selector=row["selector"], raw_log=name, raw_line=line, reason=row["reason"],
                             classification=disposition, source=source, next_validation=action))
    if len(reviewed) != 86:
        raise ValueError("Unresolved case count differs from the prior audit")
    args.out.mkdir(parents=True, exist_ok=True)
    with (args.out / "case-review.csv").open("w", encoding="utf-8", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(reviewed[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(reviewed)
    summary = dict(scope="Archived detailed captures and pinned sources", wgpu_candidate=WGPU,
                   cts_revision=CTS, receipt_commit=RECEIPT, case_inventory_sha256=digest,
                   captures=artifacts, unique_runtime_skip_ids=len(ids),
                   duplicate_skip_blocks=counts.total() - len(ids), reviewed_cases=len(reviewed),
                   classifications=dict(Counter(row["classification"] for row in reviewed)), waived=0)
    result = json.dumps(summary, indent=2) + "\n"
    (args.out / "case-review-summary.json").write_text(result, encoding="utf-8", newline="\n")
    print(result, end="")


if __name__ == "__main__":
    main()
