"""Run the pinned CTS against an exact hardware FL10_0 build."""

import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time
from uuid import uuid4


ROOT = Path(__file__).resolve().parent
WORKTREE = ROOT.parents[2]
FOCUSED = [
    "webgpu:api,operation,rendering,basic:clear:*",
    "webgpu:api,operation,rendering,basic:fullscreen_quad:*",
    'webgpu:api,operation,rendering,color_target_state:blending,formats:format="rgba8unorm-srgb"',
    "webgpu:api,operation,render_pipeline,pipeline_output_targets:color,component_count,blend:*",
    "webgpu:api,operation,command_buffer,queries,occlusionQuery:occlusion_query,alpha_to_coverage:*",
    "webgpu:api,operation,command_buffer,queries,occlusionQuery:*",
    "webgpu:api,validation,encoding,queries,begin_end:occlusion_query,*",
]


@contextmanager
def gpu_lock(path, action, wait_seconds=600):
    token = uuid4().hex
    deadline = time.monotonic() + wait_seconds
    while True:
        try:
            handle = path.open("x", encoding="utf-8")
            break
        except FileExistsError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(1)
    with handle:
        json.dump({
            "owner": "fl10_0", "session": token, "pid": os.getpid(),
            "host": os.environ.get("COMPUTERNAME"), "action": action,
            "utc": datetime.now(timezone.utc).isoformat(),
        }, handle)
    try:
        yield
    finally:
        if json.loads(path.read_text(encoding="utf-8"))["session"] == token:
            path.unlink()


def curated_selectors():
    for line in (WORKTREE / "cts_runner" / "test.lst").read_text(encoding="utf-8").splitlines():
        line = line.strip()
        conditional = re.fullmatch(r"fails-if\(([^)]+)\)\s+(.+)", line)
        if conditional:
            backends = re.findall(r"[\w-]+", conditional[1])
            yield conditional[2], "dx11" in backends
        elif line.startswith(("webgpu:", "unittests:")):
            yield line, False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("runner", type=Path)
    parser.add_argument("cts", type=Path)
    parser.add_argument("--curated", action="store_true")
    parser.add_argument("--start", type=int, default=0, help="zero-based curated selector offset")
    parser.add_argument("--count", type=int, default=0, help="number of selectors, or zero for all")
    parser.add_argument("--lock-wait-seconds", type=int, default=600)
    parser.add_argument("--coordination", type=Path, default=Path("C:/dev/personal/HorizonCodexCoord-Nov2026"))
    args = parser.parse_args()
    runner = args.runner.resolve(strict=True)
    cts = args.cts.resolve(strict=True)
    revision = (WORKTREE / "cts_runner" / "revision.txt").read_text().strip()
    actual_revision = subprocess.check_output(["git", "-C", str(cts), "rev-parse", "HEAD"], text=True).strip()
    if revision != actual_revision or subprocess.check_output(["git", "-C", str(cts), "status", "--porcelain", "--untracked-files=no"]):
        raise SystemExit("CTS checkout must match the pinned revision with clean tracked files")

    source = json.loads((ROOT / "source.json").read_text(encoding="utf-8"))
    runner_sha = hashlib.sha256(runner.read_bytes()).hexdigest()
    selectors = list(curated_selectors()) if args.curated else [(line, False) for line in FOCUSED]
    last = args.start + args.count if args.count else len(selectors)
    selectors = selectors[args.start:last]
    mode = f"curated-{args.start}" if args.curated else "focused"
    out = ROOT / "logs"
    env = os.environ.copy()
    env.pop("WGPU_DX11_FORCE_FL9_1", None)
    env.update(
        DENO_WEBGPU_BACKEND="dx11",
        WGPU_DX11_EXPERIMENTAL_FEATURE_LEVEL="10_0",
        WGPU_DX11_REQUIRE_HARDWARE="1",
    )
    lock = args.coordination / "locks" / "desktop-gpu.lock"
    metadata = {"candidate_tree": source["candidate_tree"], "cts_revision": revision,
                "runner_sha256": runner_sha, "planned_selectors": len(selectors)}
    with (out / f"cts-{mode}.jsonl").open("w", encoding="utf-8") as results, (
        out / f"cts-{mode}.log"
    ).open("w", encoding="utf-8") as log:
        results.write(json.dumps(metadata) + "\n")
        results.flush()
        for offset in range(0, len(selectors), 12):
            with gpu_lock(lock, f"Exact FL10_0 CTS {mode} batch at {offset}", args.lock_wait_seconds):
                for selector, skip in selectors[offset:offset + 12]:
                    if skip:
                        row = {"selector": selector, "result": "curated_skip"}
                    else:
                        command = [str(runner), "./tools/run_deno", "--verbose", selector]
                        try:
                            result = subprocess.run(
                                command, cwd=cts, env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=90,
                                creationflags=subprocess.CREATE_NO_WINDOW,
                            )
                            text = result.stdout.decode("utf-8", errors="replace")
                            counts = {
                                " ".join(category.split()): int(count)
                                for category, count in re.findall(
                                    r"^(Passed\s+w/o warnings|Passed with warnings|Skipped|Failed)\s*=\s*(\d+)",
                                    text, re.MULTILINE,
                                )
                            }
                            row = {"selector": selector, "exit": result.returncode, "cases": counts,
                                   "exact_hardware": any(
                                       "D3D11 strict hardware selection:" in line
                                       and "requested level 0xa000; actual level 0xa000" in line
                                       and "verified_exact_hardware=true" in line
                                       for line in text.splitlines()
                                   ),
                                   "result": "complete" if result.returncode == 0 else "fail"}
                        except subprocess.TimeoutExpired as error:
                            text = (error.stdout or b"").decode("utf-8", errors="replace")
                            row = {"selector": selector, "result": "timeout"}
                        log.write(f"=== {selector} ===\n{text}\n")
                        log.flush()
                    results.write(json.dumps(row) + "\n")
                    results.flush()
            print(f"{mode}: {min(offset + 12, len(selectors))}/{len(selectors)} selectors recorded", flush=True)


if __name__ == "__main__":
    main()
