"""Record desktop and cross-build results for the exact candidate."""

import csv
from collections import Counter
import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent
SOURCE = json.loads((ROOT / "source.json").read_text(encoding="utf-8"))
TREE = SOURCE["candidate_tree"]
NATIVE_BUILD = (
    "cargo +stable test --offline --locked {target}-p wgpu-hal "
    "--no-default-features --features dx11 --test dx11_fl10 --no-run"
)
UWP_CHECK = (
    "cargo +1.99.1 check -Zbuild-std=std,panic_abort --offline --locked --lib "
    "--target {target} -p wgpu-hal --no-default-features --features dx11"
)


def log(name):
    path = ROOT / "logs" / f"{name}.log"
    if not path.is_file():
        return "", None
    raw = path.read_bytes()
    body = raw.decode("utf-8-sig")
    match = re.search(r"^Executable SHA256=([0-9A-F]+)", body, re.MULTILINE)
    record = {"path": f"logs/{path.name}", "sha256": hashlib.sha256(raw).hexdigest()}
    if match:
        record["executable_sha256"] = match[1].lower()
    tree = re.search(r"^Candidate tree=([0-9a-f]{40})", body, re.MULTILINE)
    if tree:
        record["candidate_tree"] = tree[1]
    return body, record


def outcome(body, expected):
    if f"Candidate tree={TREE}" not in body:
        return "unknown"
    if "Exit code=timeout after " in body:
        return "timeout"
    if expected in body and "test result: ok." in body:
        return "pass"
    return "fail" if "test result: FAILED." in body else "unknown"


def write_json(name, data):
    (ROOT / name).write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main():
    tests = []
    hardware = []
    for arch in ("x64", "x86"):
        body, evidence = log(f"native-{arch}")
        status = outcome(body, "128 MRT pixels, occlusion 16/0")
        tests.append({
            "id": f"native-{arch}",
            "groups": ["native", "graphics", "texture-array", "compute", "fxc"],
            "command": f"run-desktop.ps1 -Architecture {arch} -NativeExecutable <dx11_fl10.exe>",
            "build": NATIVE_BUILD.format(target="" if arch == "x64" else "--target i686-pc-windows-msvc "),
            "result": status,
            "evidence": evidence,
        })
        cpu_body, cpu_evidence = log(f"fxc-{arch}")
        tests.append({
            "id": f"fxc-{arch}",
            "command": "<dx11_fl10.exe> --exact fxc_emits_exact_sm4_profiles --nocapture",
            "result": outcome(cpu_body, "disassembly reports gs_4_0"),
            "evidence": cpu_evidence,
        })
        hardware.append({
            "device": "Ben-Desktop RTX 3090 Ti",
            "architecture": arch,
            "runtime": "Windows desktop process",
            "driver": "32.0.16.2101",
            "fxc": "10.0.26100.9818",
            "vendor_id": "0x10de",
            "device_id": "0x2203",
            "requested_level": "0xa000",
            "actual_level": "0xa000" if status == "pass" else None,
            "adapter_luids": re.findall(r"LUID=(\w+:\w+)", body) if status == "pass" else [],
            "optional_cs4": "CS4=true" in body if status == "pass" else None,
            "result": status,
            "evidence": evidence,
        })

    for target in ("aarch64-uwp-windows-msvc", "thumbv7a-uwp-windows-msvc"):
        body, evidence = log(f"check-{target}")
        tests.append({
            "id": f"typecheck-{target}",
            "result": (
                "pass" if evidence and evidence.get("candidate_tree") == TREE
                and "Finished `dev` profile" in body else "unknown"
            ),
            "command": UWP_CHECK.format(target=target),
            "environment": {"RUSTC_BOOTSTRAP": "1"},
            "evidence": evidence,
            "scope": "HAL typecheck only",
        })

    for name, expected in (
        ("public-fl10-mrt", "shared blending, and occlusion resolve 16/0"),
        ("public-default", "matched all 1,024 pixels"),
        ("public-unsupported-9_3", "found no physical adapter"),
        ("public-unsupported-10_1", "found no physical adapter"),
    ):
        body, evidence = log(f"{name}-x64")
        tests.append({
            "id": f"{name}-x64",
            "result": outcome(body, expected),
            "command": "run-desktop.ps1 -Architecture x64 -NativeExecutable <dx11_fl10.exe> -SceneExecutable <dx11_fl91_scene.exe>",
            "evidence": evidence,
        })

    body, evidence = log("build-dx11-public")
    tests.append({
        "id": "build-dx11-public-x64",
        "result": (
            "pass" if evidence and evidence.get("candidate_tree") == TREE
            and "Finished `dev` profile" in body else "unknown"
        ),
        "environment": {"RUSTC_BOOTSTRAP": "dx11_public"},
        "command": (
            "cargo +stable rustc --offline --locked -p wgpu --no-default-features "
            "--features std,dx11,wgsl --test dx11_public -- '-Zcrate-attr=recursion_limit=\"256\"'"
        ),
        "evidence": evidence,
    })
    body, evidence = log("cts-strict-unit")
    tests.append({
        "id": "cts-strict-unit",
        "command": "cargo +stable test --offline --locked -p deno_webgpu --features dx11 --lib",
        "result": outcome(body, "11 passed; 0 failed"),
        "evidence": evidence,
    })
    body, evidence = log("clippy-current")
    tests.append({
        "id": "clippy-dx11-hal",
        "command": "cargo +stable clippy --offline --locked -p wgpu-hal --no-default-features --features dx11 --tests",
        "result": (
            "pass" if evidence and evidence.get("candidate_tree") == TREE
            and "Exit code=0" in body else "unknown"
        ),
        "evidence": evidence,
    })
    for level in ("9_1", "10_0"):
        body, evidence = log(f"regression-{level}-x64")
        counts = (
            re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", body)
            if evidence and evidence.get("candidate_tree") == TREE else None
        )
        tests.append({
            "id": f"regression-{level}-x64",
            "command": "<dx11_public.exe> --nocapture --test-threads=1",
            "environment": {"WGPU_DX11_EXPERIMENTAL_FEATURE_LEVEL": level,
                            "WGPU_DX11_REQUIRE_HARDWARE": "1"},
            "result": outcome(body, "test result: ok."),
            "counts": {"passed": int(counts[1]), "failed": int(counts[2])} if counts else None,
            "evidence": evidence,
        })

    with (ROOT / "performance.csv").open("w", encoding="utf-8", newline="") as output:
        writer = csv.writer(output)
        writer.writerow([
            "candidate_tree", "host", "architecture", "projection", "requested_level", "run",
            "result", "draws", "warmups", "samples", "median_ms", "p95_ms",
            "peak_process_working_set_observed_bytes", "log", "log_sha256",
        ])
        for level in ("9_1", "9_3", "10_0"):
            for run in (1, 2):
                name = f"scene-{level}-r{run}-x64"
                body, evidence = log(name)
                status = outcome(body, "matched all 1,024 pixels")
                matches = re.findall(
                    r"draws=(\d+) warmups=(\d+) samples=(\d+) median_ms=([.\d]+) p95_ms=([.\d]+)",
                    body,
                )
                peak = re.search(r"working set observed while running \(bytes\)=(\d+)", body)
                tests.append({
                    "id": name,
                    "result": status,
                    "command": "run-desktop.ps1 -Architecture x64 -NativeExecutable <dx11_fl10.exe> -SceneExecutable <dx11_fl91_scene.exe>",
                    "evidence": evidence if f"Candidate tree={TREE}" in body else None,
                })
                for values in matches if status == "pass" and matches else [("", "", "", "", "")]:
                    writer.writerow([
                        TREE, "Ben-Desktop RTX 3090 Ti", "x64", "FL9_1", level, run, status, *values,
                        peak[1] if peak and status == "pass" else "",
                        evidence["path"] if status != "unknown" else "",
                        evidence["sha256"] if status != "unknown" else "",
                    ])

    hardware.extend([
        {"device": "Surface Pro X Adreno 680", "architecture": "ARM64",
         "runtime": "signed AppContainer", "result": "unknown",
         "blocker": "TCP 22 on 192.168.1.227 did not respond from Ben-Desktop"},
        {"device": "Lumia 950 XL", "architecture": "ARM32", "runtime": "signed AppContainer",
         "result": "unknown", "blocker": "TCP 443 responded, but the paired Device Portal read returned a WebException"},
    ])
    for receipt in sorted((ROOT / "logs").glob("cts-*.jsonl")):
        records = [json.loads(line) for line in receipt.read_text(encoding="utf-8").splitlines() if line]
        if not records or records[0].get("candidate_tree") != TREE:
            continue
        rows = records[1:]
        counts = {status: sum(row["result"] == status for row in rows)
                  for status in ("complete", "fail", "timeout", "curated_skip")}
        complete = len(rows) == records[0].get("planned_selectors")
        hardware_seen = sum(bool(row.get("exact_hardware")) for row in rows)
        cases = Counter()
        partial_cases = Counter()
        log_text = (receipt.with_suffix(".log")).read_text(encoding="utf-8")
        sections = dict(re.findall(
            r"^=== (.+?) ===\n(.*?)(?=^=== |\Z)", log_text, re.MULTILINE | re.DOTALL,
        ))
        for row in rows:
            body = sections.get(row["selector"], "")
            totals = re.findall(
                r"^(Passed\s+w/o warnings|Passed with warnings|Skipped|Failed)\s*=\s*(\d+)",
                body, re.MULTILINE,
            )
            if totals:
                for label, count in totals:
                    cases[" ".join(label.split())] += int(count)
            elif row["result"] in ("fail", "timeout"):
                partial_cases.update(re.findall(r"^\[(pass|fail|skip|warn)\]", body, re.MULTILINE))
        failed = counts["fail"] or counts["timeout"] or cases["Failed"]
        total_passed = cases["Passed w/o warnings"] + cases["Passed with warnings"]
        tests.append({
            "id": receipt.stem,
            "result": (
                "fail" if failed else "pass" if complete and hardware_seen and total_passed else "unknown"
            ),
            "scope": "Recorded selectors only",
            "counts": counts,
            "cases": dict(cases),
            "cases_from_incomplete_selectors": dict(partial_cases),
            "planned_selectors": records[0].get("planned_selectors"),
            "recorded_selectors": len(rows),
            "selectors_with_exact_hardware_marker": hardware_seen,
            "pinned_revision": records[0]["cts_revision"],
            "runner_sha256": records[0]["runner_sha256"],
            "evidence": {"path": f"logs/{receipt.name}", "sha256": hashlib.sha256(receipt.read_bytes()).hexdigest()},
        })
    write_json("tests.json", {
        "state": "INCOMPLETE",
        "source": SOURCE,
        "worktree": str(ROOT.parents[2]),
        "build_host": "Ben-Desktop",
        "requirements_roadmap_commit": "126114a9fa069194e2bd63ac8e8d46c327a35d83",
        "build_environment": {
            "CARGO_TARGET_DIR": "E:/temp/wgpu-dx11-fl10_0-nov21-target",
            "CARGO_PROFILE_DEV_DEBUG": "0",
            "CARGO_INCREMENTAL": "0",
            "rustc": "1.99.1",
            "msvc": "14.44.35207",
        },
        "tests": tests,
        "pending": ["reviewed inherited gates and complete inherited rerun", "full CTS",
                    "signed surfaces", "physical device loss", "HorizonUWP verification",
                    "independent review"],
    })
    write_json("hardware.json", {"candidate_tree": TREE, "runs": hardware})


if __name__ == "__main__":
    main()
