"""Build the FL10_0 ledger from the pinned roadmap and this candidate's receipts."""

import argparse
import csv
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent
SOURCE = json.loads((ROOT / "source.json").read_text(encoding="utf-8"))
MS = "https://learn.microsoft.com/en-us/windows/win32/api/d3d11/"
FORMAT_CASES = {
    ("DXGI_FORMAT_D24_UNORM_S8_UINT", "depth_attachment"): "graphics",
    ("DXGI_FORMAT_D24_UNORM_S8_UINT", "texture2d"): "graphics",
    ("DXGI_FORMAT_R32_UINT", "color_attachment"): "graphics",
    ("DXGI_FORMAT_R32_UINT", "texture2d"): "graphics",
    ("DXGI_FORMAT_R8G8B8A8_TYPELESS", "texture2d"): "texture-array",
    ("DXGI_FORMAT_R8G8B8A8_UNORM", "shader_load"): "texture-array",
    ("DXGI_FORMAT_R8G8B8A8_UNORM", "texture3d"): "texture-array",
    ("DXGI_FORMAT_R8G8B8A8_UNORM_SRGB", "generate_mips"): "texture-array",
    ("DXGI_FORMAT_R8G8B8A8_UNORM_SRGB", "shader_load"): "texture-array",
}

OPERATIONS = [
    ("native-context", "fl10::Device::with_context", "Lifetime and ordering across batches", "mandatory", "texture-array"),
    ("open", "D3D11CreateDevice / GetFeatureLevel", "Exact singleton and hardware identity", "mandatory", "native"),
    ("vs", "CreateVertexShader / VSSetShader", "Execute vs_4_0 bytecode", "mandatory", "graphics"),
    ("ps", "CreatePixelShader / PSSetShader", "Execute ps_4_0 bytecode", "mandatory", "graphics"),
    ("gs", "CreateGeometryShader / GSSetShader", "Create and bind gs_4_0 bytecode for stream output", "mandatory", "graphics"),
    ("so-declaration", "CreateGeometryShaderWithStreamOutput", "Capture one SM4 stream, reject invalid semantic", "mandatory", "graphics"),
    ("so-four-buffers", "CreateGeometryShaderWithStreamOutput / SOSetTargets", "Four buffer slots, offsets, append, overflow", "mandatory", ""),
    ("so-replay", "SOSetTargets / DrawAuto", "Capture and replay triangle adjacency", "mandatory", "graphics"),
    ("gs-layer", "GSSetShader", "Route SV_RenderTargetArrayIndex and SV_ViewportArrayIndex", "mandatory", ""),
    ("gs-line", "IASetPrimitiveTopology / GSSetShader", "Line adjacency, strips, primitive ID and GS output limits", "mandatory", ""),
    ("ia-instancing", "DrawInstanced / DrawIndexedInstanced", "Step rate, start instance and signed base vertex", "mandatory", ""),
    ("ia-last-slot", "CreateInputLayout / IASetVertexBuffers", "Slot 15 valid, slot 16 rejected or diagnosed", "mandatory", ""),
    ("tex-1d-array", "CreateTexture1D / CreateShaderResourceView", "1D array subresources and typed views", "mandatory", ""),
    ("tex-2d-array", "CreateTexture2D / CreateShaderResourceView", "Create NPOT array and read nonzero base slice", "mandatory", "texture-array"),
    ("tex-3d", "CreateTexture3D / CreateShaderResourceView", "Upload and load a nonzero depth slice", "mandatory", "texture-array"),
    ("tex-cube", "CreateTexture2D / CreateShaderResourceView", "Six faces, sampler axes and invalid cube arrays", "mandatory", ""),
    ("srgb-cast", "CreateShaderResourceView", "Read typeless RGBA8 through sRGB, reject a foreign format family", "mandatory", "texture-array"),
    ("transfer", "UpdateSubresource / CopyResource", "Upload base subresources and copy generated array mip chain", "mandatory", "texture-array"),
    ("transfer-boxes", "CopySubresourceRegion", "Box copies at nonzero offsets for every resource shape", "mandatory", ""),
    ("mips", "GenerateMips / PSSetShaderResources", "Generate and load a nonzero sRGB array mip", "mandatory", "texture-array"),
    ("mip-filter", "CreateSamplerState / VSSetSamplers / PSSetSamplers", "Wrap and explicit LOD on NPOT, anisotropy and comparison", "mandatory", ""),
    ("buffers", "CreateBuffer / Map / Unmap", "Default, dynamic, immutable, staging and mapped range limits", "mandatory", ""),
    ("buffer-srv", "CreateShaderResourceView", "Typed buffer SRV and index range", "mandatory", ""),
    ("cb", "VSSetConstantBuffers / GSSetConstantBuffers / PSSetConstantBuffers", "14 slots, size limits and packed data", "mandatory", ""),
    ("mrt", "CreateRenderTargetView / OMSetRenderTargets", "Eight R32_UINT targets, exact pixels in each", "mandatory", "graphics"),
    ("portable-mrt", "wgpu render pass / D3D11 output merger", "Eight color targets with shared blending and per-slot write masks", "mandatory", "public-mrt"),
    ("portable-alpha-to-coverage", "MultisampleState.alpha_to_coverage_enabled / CreateBlendState", "Four-sample alpha-to-coverage through occlusion with direct draws, render bundles and disabled color writes", "mandatory", "cts-alpha-to-coverage"),
    ("viewport", "RSSetViewports / RSSetScissorRects", "Multiple viewports and scissor routing", "mandatory", ""),
    ("depth", "CreateDepthStencilView / ClearDepthStencilView", "Depth clear and equality rejection in D24S8", "mandatory", "graphics"),
    ("stencil", "CreateDepthStencilState / OMSetDepthStencilState", "Distinct front/back stencil behavior", "mandatory", ""),
    ("so-query", "CreateQuery / Begin / End / GetData", "Stream output statistics 1 written and 1 needed", "mandatory", "graphics"),
    ("occlusion", "CreateQuery / Begin / End / GetData", "Depth-tested occlusion samples 16 and 0", "mandatory", "graphics"),
    ("portable-occlusion", "wgpu render pass / ResolveQuerySet", "Resolve a written count and zero an unwritten query", "mandatory", "public-mrt"),
    ("remaining-queries", "CreateQuery / CreatePredicate / SetPredication", "Events, predication, pipeline statistics, timestamps and disjoint", "mandatory", ""),
    ("resolve", "CheckMultisampleQualityLevels / ResolveSubresource", "Exact per-format count, resolve and negative cases", "mandatory", ""),
    ("checks", "CheckFormatSupport / CheckFeatureSupport", "Per-format query masks, option bits and HRESULTs", "mandatory", ""),
    ("cs4-gate", "CheckFeatureSupport", "D3D10_X_HARDWARE_OPTIONS independently queries CS4", "optional", "compute"),
    ("cs4-dispatch", "CreateComputeShader / CSSetUnorderedAccessViews / Dispatch", "Single structured UAV, complete readback, typed UAV compiler rejection", "optional", "compute"),
    ("cs4-limits", "CreateComputeShader / Dispatch / CreateUnorderedAccessView", "Raw UAV alignment, thread limits and group-shared restrictions", "optional", ""),
    ("no-indirect", "DrawInstancedIndirect / DrawIndexedInstancedIndirect / DispatchIndirect", "Higher-tier calls rejected or debug-layer diagnosed", "unavailable", ""),
    ("present", "Present / ResizeBuffers / GetDeviceRemovedReason", "Signed surfaces, resize, suspend/resume and device loss", "mandatory", ""),
    ("cross-tier", "Certification gates", "Review FL9_1 through FL9_3 and rerun inherited tests at exact integrated SHA", "mandatory", ""),
    ("horizon", "HorizonUWP", "Pinned release shaders, DRS, HUD, mods, presentation and same-scene controls", "mandatory", ""),
]


def receipt(case):
    if case == "cts-alpha-to-coverage":
        log = ROOT / "logs" / "cts-focused.jsonl"
        if log.exists():
            raw = log.read_bytes()
            rows = [json.loads(line) for line in raw.splitlines() if line]
            if rows and rows[0]["candidate_tree"] == SOURCE["candidate_tree"]:
                for row in rows[1:]:
                    if (
                        ":occlusion_query,alpha_to_coverage:" in row["selector"]
                        and row.get("exact_hardware") and row["result"] == "complete"
                        and row["cases"].get("Passed w/o warnings") == 80
                        and not row["cases"].get("Failed")
                    ):
                        return "pass", "RTX 3090 Ti x64", f"logs/{log.name}", hashlib.sha256(raw).hexdigest()
        return "unknown", "", "", ""
    success = {
        "native": "requested=0xa000 actual=0xa000",
        "graphics": "128 MRT pixels, occlusion 16/0",
        "compute": "optional structured-buffer CS4 dispatch and 16 GPU words",
        "texture-array": "WGSL SM4 loads, NPOT array mips, copied sRGB view, 3D slice, 16 pixels",
        "public-mrt": "shared blending, and occlusion resolve 16/0",
    }.get(case)
    results = []
    for arch in ("x64", "x86"):
        log = ROOT / "logs" / f"{'public-fl10-mrt' if case == 'public-mrt' else 'native'}-{arch}.log"
        if success and log.exists():
            content = log.read_text(encoding="utf-8-sig")
            if (
                success in content
                and ("test result: ok. 1 passed; 0 failed" if case == "public-mrt"
                     else "test result: ok. 2 passed; 0 failed") in content
                and f"Candidate tree={SOURCE['candidate_tree']}" in content
            ):
                digest = hashlib.sha256(log.read_bytes()).hexdigest()
                results.append((arch, f"logs/{log.name}", digest))
    if not results:
        return ("unknown", "", "", "")
    return (
        "pass",
        ";".join(f"RTX 3090 Ti {arch}" for arch, _, _ in results),
        ";".join(log for _, log, _ in results),
        ";".join(sha for _, _, sha in results),
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("roadmap", type=Path, help="pinned roadmap deliverables/dx11-levels directory")
    args = parser.parse_args()
    columns = [
        "id", "kind", "contract", "classification", "method_or_format", "qualifiers",
        "owner", "public_mapping", "case", "physical_result", "devices", "evidence",
        "evidence_sha256", "candidate_tree", "source",
    ]
    destination = ROOT / "requirements.csv"
    with destination.open("w", encoding="utf-8", newline="") as output:
        writer = csv.writer(output)
        writer.writerow(columns)
        tree = SOURCE["candidate_tree"]
        for name, method, requirement, classification, case in OPERATIONS:
            writer.writerow([
                f"FL10_0.METHOD.{name}", "method", requirement, classification, method, "",
                "fl10_0", "", case, *receipt(case), tree,
                MS + "nn-d3d11-id3d11devicecontext" if name == "native-context" else MS,
            ])
        for filename in ("native-guarantees.csv", "dxgi-format-support.csv"):
            with (args.roadmap / filename).open(encoding="utf-8-sig", newline="") as source:
                for row in csv.DictReader(source):
                    if row["tier"] != "10_0":
                        continue
                    is_format = "format" in row
                    case = FORMAT_CASES.get((row.get("format"), row.get("use")))
                    writer.writerow([
                        row["id"], "format-use" if is_format else "guarantee",
                        row["use"] if is_format else f"{row['requirement']} — {row['native_value']}",
                        row["status"] if is_format else row["classification"],
                        row["format"] if is_format else row["hal_entry"],
                        row["condition"] if is_format else row["qualifiers"],
                        row["owner"], row["public_mapping"],
                        case or (row["test"] if is_format else row["test_id"]),
                        *receipt(case), tree, row["source"],
                    ])


if __name__ == "__main__":
    main()
