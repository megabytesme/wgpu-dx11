"""Capture exact FL9_1 CTS cases with native-adapter identity and file digests."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import uuid
from datetime import datetime, timezone


def run_git(path, *args):
    return subprocess.check_output(["git", "-C", str(path), *args], text=True).strip()


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


@contextmanager
def gpu_lock(path):
    token = uuid.uuid4().hex
    record = {
        "owner": "wgpu-dx11",
        "session": f"cts-fl91-{token[:12]}",
        "host": socket.gethostname(),
        "pid": os.getpid(),
        "utc": datetime.now(timezone.utc).isoformat(),
        "action": "Strict FL9_1 focused CTS batch",
        "token": token,
    }
    try:
        stream = path.open("x", encoding="utf-8")
    except FileExistsError:
        raise SystemExit(f"Desktop GPU is in use: {path}") from None
    with stream:
        json.dump(record, stream)
    try:
        yield
    finally:
        if json.loads(path.read_bytes()).get("token") != token:
            raise RuntimeError(f"GPU lock ownership changed during the run: {path}")
        path.unlink()


def run_selector(binary, checkout, env, selector, log):
    args = [str(binary), "./tools/run_deno", "--verbose", selector]
    with log.open("wb") as stream:
        try:
            code = subprocess.run(args, cwd=checkout, env=env, stdout=stream,
                                  stderr=subprocess.STDOUT, timeout=120).returncode
        except subprocess.TimeoutExpired:
            code = "timeout"
    output = log.read_text(encoding="utf-8", errors="replace")
    summary = dict(re.findall(
        r"(?m)^(Passed  w/o warnings|Passed with warnings|Skipped|Failed)\s+= (\d+) / \d+",
        output,
    ))
    adapters = sorted({
        line for line in output.splitlines()
        if line.startswith("D3D11 strict hardware selection:")
    })
    counts = {label: int(count) for label, count in summary.items()}
    strict_hardware = (len(adapters) == 1
                       and adapters[0].startswith("D3D11 strict hardware selection: backend=Dx11 ")
                       and adapters[0].endswith("verified_FL9_1_marker=true"))
    passed = (code == 0 and strict_hardware and len(counts) == 4
              and counts["Passed  w/o warnings"] > 0
              and not any(counts[label] for label in ("Skipped", "Failed", "Passed with warnings")))
    return {"selector": selector, "exit": code, "pass": passed, "counts": counts,
            "adapters": adapters, "log": log.name, "sha256": sha256(log)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path, help="wgpu repository containing the candidate")
    parser.add_argument("--candidate", required=True, help="Full commit recorded in the binary build receipt")
    parser.add_argument("--runner", required=True, type=Path, help="DX11-enabled cts_runner executable")
    parser.add_argument("--runner-sha256", required=True, help="Executable digest from the build receipt")
    parser.add_argument("--cts", required=True, type=Path, help="Clean checkout of the pinned WebGPU CTS")
    parser.add_argument("--selectors", required=True, type=Path, help="Exactly scoped batch, one selector per line")
    parser.add_argument("--output", required=True, type=Path, help="Fresh output directory")
    parser.add_argument("--gpu-lock", required=True, type=Path, help="Shared desktop GPU lock file")
    args = parser.parse_args()

    if not re.fullmatch(r"[0-9a-f]{40}", args.candidate):
        parser.error("--candidate requires a full lowercase Git SHA")
    revision = run_git(args.source, "show", f"{args.candidate}:cts_runner/revision.txt")
    if (run_git(args.cts, "rev-parse", "HEAD") != revision
            or run_git(args.cts, "status", "--porcelain")):
        parser.error("CTS checkout must be clean and match the candidate's pin")
    runner = args.runner.resolve(strict=True)
    digest = sha256(runner)
    if digest != args.runner_sha256.lower():
        parser.error("Runner digest differs from the supplied build receipt")
    selectors = [line.strip() for line in args.selectors.read_text(encoding="utf-8-sig").splitlines()
                 if line.strip() and not line.lstrip().startswith("#")]
    if (not selectors or len(selectors) != len(set(selectors))
            or any(not line.startswith("webgpu:") for line in selectors)):
        parser.error("Supply unique webgpu: selectors without fails-if clauses")

    strict_env = {"DENO_WEBGPU_BACKEND": "dx11", "WGPU_DX11_FORCE_FL9_1": "1",
                  "WGPU_DX11_REQUIRE_HARDWARE": "1"}
    env = dict(os.environ, **strict_env)
    env.pop("WGPU_DX11_FEATURE_LEVEL", None)
    receipt = {"candidate": args.candidate, "cts_revision": revision, "runner_sha256": digest,
               "selector_sha256": sha256(args.selectors), "environment": strict_env, "runs": []}
    with gpu_lock(args.gpu_lock):
        args.output.mkdir(parents=True)
        for index, selector in enumerate(selectors, start=1):
            row = run_selector(runner, args.cts, env, selector, args.output / f"{index:03}.log")
            receipt["runs"].append(row)
            (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
            print(f"{row['log']} {'PASS' if row['pass'] else 'REVIEW'} {selector}", flush=True)
    return 0 if all(row["pass"] for row in receipt["runs"]) else 1


if __name__ == "__main__":
    sys.exit(main())
