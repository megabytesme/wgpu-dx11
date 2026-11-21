"""Normalize Microsoft DXGI tables and compare adjacent tiers within each source scope."""
import argparse
import csv
from html import unescape
from pathlib import Path
import re
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parent
TIERS = ("9_1", "9_2", "9_3", "10_0", "10_1", "11_0", "11_1")
DXGI = "https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/"
DRIVER = "https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats"
PAGES = {
    level: DXGI + ("format-support-for-direct3d-" + level.replace("_", "-") + "-feature-level-hardware"
                   if level.startswith("11_") else
                   "format-support-for-direct3d-feature-level-" + level.replace("_", "-") + "-hardware")
    for level in TIERS if level != "9_2"
}
FORMAT_COUNTS = {"9_1": 109, "9_3": 109, "10_0": 109, "10_1": 109, "11_0": 115, "11_1": 115}
SCHEMA = ("id", "tier", "source_scope", "format", "dxgi_id", "use", "status", "condition",
          "owner", "hal_entry", "public_mapping", "test", "hardware_observation", "result", "source")
DELTA_SCHEMA = ("from_tier", "to_tier", "source_scope", "format", "dxgi_id", "use", "change",
                "from_status", "to_status", "from_condition", "to_condition", "from_id", "to_id",
                "from_source", "to_source")
DDI_TIERS = TIERS[:3]

PROPERTY = {
    "bits_per_element_bpe": "bits_per_element",
    "input_assembler_vertex_buffer": "vertex_input",
    "input_assembler_index_buffer": "index_input",
    "rendertarget": "color_attachment",
    "blendable_rendertarget": "blend_attachment",
    "depth_stencil_target": "depth_attachment",
    "4x_multisample_rendertarget": "msaa_4x",
    "8x_multisample_rendertarget": "msaa_8x",
    "other_multisample_count_rt": "msaa_other",
    "mipmap_auto_generation": "generate_mips",
    "shader_sample_point_sample_only": "sample_point",
    "shader_sample_any_filter": "sample_filtered",
    "shader_sample_c_comparison_filter": "sample_comparison",
    "shader_sample_mono_1_bit_filter": "sample_mono",
    "typed_uav_load": "uav_typed_load",
    "backbuffer_castable_even_fully_typed": "back_buffer_cast",
    "shader_ld": "shader_load",
    "uav_atomic_signed_min_or_max": "uav_atomic_signed_min_max",
    "uav_atomic_unsigned_min_or_max": "uav_atomic_unsigned_min_max",
}
DRIVER_PROPERTY = {
    "IA_VERTEX_BUFFER": "vertex_input", "IA_INDEX_BUFFER": "index_input",
    "MIP": "mipmap", "MIP_AUTOGEN": "generate_mips", "BLENDABLE": "blend_attachment",
    "RENDER_TARGET": "color_attachment", "DEPTH_STENCIL": "depth_attachment",
    "DISPLAY": "display_scan_out", "CAST_WITHIN_BIT_LAYOUT": "cast_within_bit_layout",
}
CLASSIFY = {
    "required": "mandatory", "optional": "optional", "dependant": "conditional",
    "dependent": "conditional", "-": "unavailable", "–": "unavailable",
    "✔": "mandatory", "✅": "mandatory", "❔": "optional", "❌": "unavailable", "〰": "conditional",
}
FORMAT_HEADING = re.compile(r"<h2 id=\"([^\"]+)\">(DXGI_FORMAT_\w+)\s*(?:<sup>(.*?)</sup>)?\s*\((\d+)\)</h2>")
ROW = re.compile(r"<tr(?:\s[^>]*)?>(.*?)</tr>", re.S)
CELL = re.compile(r"<td(?:\s[^>]*)?>(.*?)</td>", re.S)
PARAGRAPH = re.compile(r"<p>(.*?)</p>", re.S)

def read_page(cache, key, url):
    if cache is not None:
        return (cache / f"{key}.html").read_text(encoding="utf-8-sig")
    with urlopen(url, timeout=30) as response:
        return response.read().decode("utf-8-sig")

def strip(text):
    return " ".join(unescape(re.sub("<[^>]+>", "", text)).split())

def slug(text):
    return re.sub(r"[^a-z0-9]+", "_", text.lower()).strip("_")

EXACT_TEXTURE_GAPS = {
    "A8_UNORM": "Alpha-only native channels differ from public R8Unorm",
    "B8G8R8X8_UNORM": "Public BGRA8 formats include a stored alpha channel",
    "B8G8R8X8_UNORM_SRGB": "Public BGRA8 sRGB includes a stored alpha channel",
    "R10G10B10_XR_BIAS_A2_UNORM": "Public Rgb10a2Unorm has a different value encoding",
    "R32G32B32_FLOAT": "Three-component 96-bit native texture has no public TextureFormat",
    "R32G32B32_UINT": "Three-component 96-bit native texture has no public TextureFormat",
    "R32G32B32_SINT": "Three-component 96-bit native texture has no public TextureFormat",
    "R1_UNORM": "Public TextureFormat has no single-bit packed format",
    "R8G8_B8G8_UNORM": "Native packed two-pixel layout has no public TextureFormat",
    "G8R8_G8B8_UNORM": "Native packed two-pixel layout has no public TextureFormat",
}
NATIVE_USE_GAPS = {
    "generate_mips": "No public GenerateMips operation",
    "cast_within_bit_layout": "Native casting exceeds public compatible view formats",
    "shared_resource": "No equivalent public D3D11 shared-handle contract",
    "tiled_resource": "No equivalent D3D11 tiled mapping contract",
    "stream_output_buffer": "No public stream-output stage or output routing",
    "output_merger_logic_op": "No public output-merger logic-op selection",
    "sample_mono": "Native monochrome filter is absent from public SamplerDescriptor",
}
PACKED_16 = {"B5G6R5_UNORM", "B5G5R5A1_UNORM", "B4G4R4A4_UNORM"}
FL10 = ("10_0", "10_1")
TEXTURE_USES = {"format_support", "texture1d", "texture2d", "texture3d", "texturecube",
                "color_attachment", "blend_attachment", "depth_attachment", "mipmap"}


def texture_gap(fmt, prop, format_note, tier):
    if prop not in TEXTURE_USES and not prop.startswith(("shader_", "sample_", "msaa", "multisample")):
        return None
    if prop in TEXTURE_USES and fmt.endswith("_TYPELESS") and format_note in ("PCS", "PCC"):
        return "Native typeless resource identity exceeds public typed formats and compatible views"
    if format_note == "V" and fmt not in ("NV12", "P010"):
        return "This DXGI video format has no exact public TextureFormat"
    if tier in FL10 and fmt.startswith(("BC4_", "BC5_")) and not fmt.endswith("_TYPELESS"):
        return "Public TEXTURE_COMPRESSION_BC groups these formats with BC6H/BC7"
    return EXACT_TEXTURE_GAPS.get(fmt)


def route(name, prop, tier, format_note):
    fmt = name.removeprefix("DXGI_FORMAT_")
    if prop == "bits_per_element":
        return "formats", "texture.rs", "Format block metadata", "FORMAT_CREATE"
    if prop == "vertex_input":
        mapping = ("BLOCKER: no public Uint vertex format with 10/10/10/2 bit packing"
                   if fmt == "R10G10B10A2_UINT" else "VertexFormat subset")
        return "graphics pipeline", "hal_device.rs::create_render_pipeline and hal_pipeline.rs::vertex_format", mapping, "VERTEX"
    if prop == "index_input":
        return "command recording", "hal_encoder.rs::set_index_buffer", "IndexFormat Uint16/Uint32 with native limits", "INDEX"
    if prop in ("display_scan_out", "back_buffer_cast"):
        mapping = ("Native display scan-out and mode capability exceed public SurfaceConfiguration"
                   if prop == "display_scan_out" else
                   "Native back-buffer casting exceeds the subset of supported Surface view formats")
        return "presentation", "surface.rs and swapchain.rs", "BLOCKER: " + mapping, "PRESENT"
    if prop == "cpu_lockable":
        if fmt == "UNKNOWN":
            return "buffers and transfers", "buffer.rs and hal_device.rs", "Buffer mapping and copies subset", "TRANSFER"
        return ("buffers and transfers", "texture.rs and hal_transfer.rs",
                "BLOCKER: native typed-resource lock modes exceed public buffer mapping and texture copies", "TRANSFER")
    if prop in NATIVE_USE_GAPS or prop.startswith("video_"):
        mapping = NATIVE_USE_GAPS.get(prop, "No public equivalent for the native DXGI video operation")
        return "native API extension", "No full native operation at the pin", "BLOCKER: " + mapping, "FORMAT_NATIVE"
    if prop == "buffer":
        if fmt == "UNKNOWN":
            return "buffers and transfers", "buffer.rs and hal_device.rs", "Public byte-oriented Buffer and BufferUsages", "FORMAT_CREATE"
        return ("native API extension", "buffer.rs, hal_bind_group.rs and hal_device.rs",
                "BLOCKER: no exact public DXGI-typed buffer view", "FORMAT_NATIVE")

    gap = texture_gap(fmt, prop, format_note, tier)
    if gap:
        return "native API extension", "No exact public mapping at the pin", "BLOCKER: " + gap, "FORMAT_NATIVE"
    if prop == "multisample_load":
        return ("shader translation", "hal_shader.rs and hal_device.rs::create_texture_view",
                "WGSL textureLoad and sampled multisample view subset", "MSAA")
    if prop == "multisample_resolve":
        return ("graphics pipeline", "hal_encoder.rs and hal_transfer.rs",
                "Public resolve target with format-specific resolve flags", "MSAA")
    if prop.startswith("msaa"):
        return ("graphics pipeline", "hal_pipeline.rs, hal_device.rs and hal_encoder.rs::begin_render_pass",
                "TextureFormatFeatureFlags and wgpu sample-count subset; no native sample quality selection", "MSAA")
    if prop.startswith(("raw_uav", "structured_uav", "typed_uav")):
        return ("storage and binding", "buffer.rs, texture.rs, hal_device.rs and hal_bind_group.rs",
                "Storage resource creation subset; public bindings do not select native raw/structured/typed view classes", "STORAGE")
    if prop.startswith("uav_"):
        return ("storage and binding", "hal_shader.rs, hal_bind_group.rs and texture.rs",
                "Storage texture operations and atomics subset; verify exact format, access and stage", "STORAGE")
    if prop in ("color_attachment", "blend_attachment", "depth_attachment"):
        return "graphics pipeline", "hal_pipeline.rs and hal_device.rs::create_texture_view", "TextureFormat feature intersection", "BLEND_DEPTH"
    if prop.startswith(("shader_", "sample_")):
        mapping = ("SM4.1 single-component native gather; verify any exact public textureGather subset"
                   if prop == "shader_gather4" and tier == "10_1" else "WGSL sampling/load/compare/gather subset")
        return "shader translation", "hal_shader.rs and hal_sampler.rs", mapping, "SAMPLING"
    if fmt in ("NV12", "P010"):
        return ("formats", "capabilities.rs and hal_device.rs::create_texture",
                "Public multi-planar TextureFormat and typed plane views; video processing is separate", "FORMAT_CREATE")
    return ("formats", "capabilities.rs and hal_device.rs::create_texture",
            "TextureFormat and get_texture_format_features intersection", "FORMAT_CREATE")


def format_conditions(tier, name, prop, status, footnote):
    notes = [f"Microsoft format flags {footnote}"] if footnote else []
    if status == "conditional":
        notes.append("Required only when the related optional feature is supported")
    if status == "unavailable":
        return "; ".join(notes)
    fmt = name.removeprefix("DXGI_FORMAT_")
    if tier in FL10 and fmt == "R10G10B10_XR_BIAS_A2_UNORM":
        notes.append("D3D11 downlevel exceptions classify XR_BIAS as optional on both 10.x tiers; retain the source cell and check runtime/driver applicability")
    if tier in (*FL10, "11_0") and fmt in PACKED_16:
        notes.append("D3D11 feature-level overview qualifies 16-bpp formats as runtime/driver options; verify agreement before treating this cell as unconditional")
    if tier in FL10 and fmt.startswith(("B8G8R8A8", "B8G8R8X8")):
        notes.append("Verify runtime/driver support for the extended BGRA group independently of this format operation")
    if prop == "msaa_4x" and tier == "10_1":
        notes.append("Functional spec §19.2.5 sets the standard 4x floor for output formats below 64 bits/sample; broader API remarks require reconciliation")
    if prop in ("multisample_resolve", "multisample_load"):
        notes.append("Operation support alone does not guarantee a particular sample count")
    if footnote == "V":
        notes.append("Video-format status applies to the source table; verify D3D11 video interface and driver availability separately")
    return "; ".join(notes)


def format_row(tier, scope, name, number, prop, status, qualifiers, url, footnote=None):
    owner, hal, public, test = route(name, prop, tier, footnote)
    if status == "metadata":
        result = "REFERENCE"
    elif status in ("mandatory", "conditional") and public.startswith("BLOCKER"):
        result = "BLOCKER"
    else:
        result = "OPERATION_UNVERIFIED"
    receipt = ("hardware-probes.json: correlate format and MSAA queries by adapter, tier and DXGI id"
               if number not in ("", "0") else "No per-format query attribution")
    return (f"FL{tier}.{scope}.{name}.{prop}", tier, scope, name, number, prop, status, qualifiers,
            owner, hal, public, test, receipt, result, url)


def read_formats(level, html):
    headings = list(FORMAT_HEADING.finditer(html))
    if len(headings) != FORMAT_COUNTS[level]:
        raise ValueError(f"{level}: expected {FORMAT_COUNTS[level]} formats, found {len(headings)}")
    for index, h in enumerate(headings):
        anchor, name, footnote, number = h.groups()
        end = headings[index + 1].start() if index + 1 < len(headings) else len(html)
        table = re.search(r"<table(?:\s[^>]*)?>(.*?)</table>", html[h.end():end], re.S)[1]
        fields = [CELL.findall(row) for row in ROW.findall(table)]
        fields = [cells for cells in fields if cells]
        expected = 47 if level.startswith("11_") else 46
        if len(fields) != expected:
            raise ValueError(f"{level} {name}: expected {expected} cells, got {len(fields)}")
        for target, support in fields:
            prop = slug(strip(target))
            prop = PROPERTY.get(prop, prop)
            if prop == "bits_per_element":
                status = "metadata"
                note = strip(support)
            else:
                alt = re.search(r'alt="([^"]+)"', support)
                marker = alt[1] if alt else strip(support)
                if marker and marker[0] in ("✔", "✅", "❔", "❌", "〰"):
                    marker = marker[0]
                if marker not in CLASSIFY:
                    raise ValueError(f"{level}/{name}/{prop}: unknown status {marker!r}")
                status = CLASSIFY[marker]
                note = format_conditions(level, name, prop, status, footnote)
            yield format_row(level, "DXGI", name, number, prop, status, note, PAGES[level] + "#" + anchor, footnote)

def read_driver(html, format_ids):
    heading = html.index("D3D9 format (D3DDDIFMT")
    table = html[heading:html.index("</table>", heading)]
    seen = set()
    for record in ROW.findall(table):
        columns = CELL.findall(record)
        if not columns:
            continue
        _, dxgi, flags, levels = columns
        flags = [strip(flag) for flag in PARAGRAPH.findall(flags)] or [strip(flags)]
        levels = [strip(level) for level in PARAGRAPH.findall(levels)] or [strip(levels)]
        if len(flags) != len(levels):
            raise ValueError(f"Driver table pairing changed for {strip(dxgi)}: {flags} vs {levels}")
        name = "DXGI_FORMAT_" + strip(dxgi)
        for flag, first_level in zip(flags, levels, strict=True):
            if first_level not in DDI_TIERS:
                raise ValueError(f"Unexpected graphics level {first_level!r} for {name}/{flag}")
            prop = DRIVER_PROPERTY.get(flag, slug(flag))
            for level in DDI_TIERS[DDI_TIERS.index(first_level):]:
                key = (level, name, prop)
                if key in seen:
                    continue
                seen.add(key)
                yield format_row(level, "DDI", name, format_ids.get(name, ""), prop, "mandatory",
                                 f"Minimum DDI tier {first_level}; application API restrictions apply", DRIVER)

def adjacent_deltas(rows):
    by_scope_tier = {}
    for raw in rows:
        row = dict(zip(SCHEMA, raw, strict=True))
        group = by_scope_tier.setdefault((row["source_scope"], row["tier"]), {})
        group[(row["format"], row["use"])] = row

    for lower, higher in zip(TIERS, TIERS[1:]):
        for scope in ("DDI", "DXGI"):
            before = by_scope_tier.get((scope, lower))
            after = by_scope_tier.get((scope, higher))
            if before is None or after is None:
                continue
            for fmt, use in sorted(before.keys() | after.keys()):
                old = before.get((fmt, use), {})
                new = after.get((fmt, use), {})
                old_status = old.get("status", "not_listed")
                new_status = new.get("status", "not_listed")
                old_note = old.get("condition", "")
                new_note = new.get("condition", "")
                if not old:
                    change = "newly_listed"
                elif not new:
                    change = "no_longer_listed"
                elif old_status != new_status:
                    change = "status"
                elif old_note != new_note:
                    change = "metadata" if old_status == "metadata" else "qualifier"
                else:
                    continue
                number = new.get("dxgi_id", "") or old.get("dxgi_id", "")
                yield (lower, higher, scope, fmt, number, use, change, old_status, new_status,
                       old_note, new_note, old.get("id", ""), new.get("id", ""),
                       old.get("source", ""), new.get("source", ""))


def write_csv(path, schema, rows):
    with path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output, lineterminator="\n")
        writer.writerow(schema)
        writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--html-cache", type=Path, help="Use reviewed copies named 9_1.html etc. and driver.html")
    parser.add_argument("--output", type=Path, default=ROOT / "dxgi-format-support.csv")
    parser.add_argument("--deltas-output", type=Path, help="Adjacent-tier comparison CSV (defaults beside --output)")
    args = parser.parse_args()
    deltas_output = args.deltas_output or args.output.with_name("dxgi-format-deltas.csv")
    if deltas_output.resolve() == args.output.resolve():
        parser.error("Format support and delta outputs must be different files")
    rows = [row for level, url in PAGES.items() for row in read_formats(level, read_page(args.html_cache, level, url))]
    ids = {row[3]: row[4] for row in rows}
    rows.extend(read_driver(read_page(args.html_cache, "driver", DRIVER), ids))
    if len({row[0] for row in rows}) != len(rows):
        raise ValueError("Duplicate DXGI requirement IDs")
    rows.sort(key=lambda row: (TIERS.index(row[1]), row[2], row[3], row[5]))
    deltas = list(adjacent_deltas(rows))
    write_csv(args.output, SCHEMA, rows)
    write_csv(deltas_output, DELTA_SCHEMA, deltas)
    print(f"{len(rows)} format/use rows, {len(deltas)} adjacent-tier changes")


if __name__ == "__main__":
    main()
