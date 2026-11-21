"""Compare exact-level native format query observations with the recorded source matrix.

Example: python verify-format-claims.py evidence/desktop-raw.txt --sha256 DIGEST
         --tier 10_0 --adapter 0 > report.json
The digest must come from an independent trusted record, not the log being checked.
Query results are observations only; no result grants conformance or certification.
"""
import argparse
import collections
import csv
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
LEVELS = {'9_1': '9100', '9_2': '9200', '9_3': '9300', '10_0': 'a000',
          '10_1': 'a100', '11_0': 'b000', '11_1': 'b100'}
# D3D11_FORMAT_SUPPORT and D3D11_FORMAT_SUPPORT2 from the Windows SDK.
SUPPORT = dict(zip(('buffer vertex_input index_input stream_output_buffer texture1d texture2d '
                    'texture3d texturecube shader_load shader_sample sample_comparison sample_mono '
                    'mipmap generate_mips color_attachment blend_attachment depth_attachment cpu_lockable '
                    'multisample_resolve display_scan_out cast_within_bit_layout').split(),
                   (1 << x for x in range(21))))
SUPPORT.update({'multisample_load': 0x400000, 'shader_gather4': 0x800000,
                'back_buffer_cast': 0x1000000, 'typed_uav': 0x2000000,
                'shader_gather4_c': 0x4000000, 'video_decoder_support': 0x8000000,
                'video_processor_output': 0x10000000, 'video_processor_input': 0x20000000})
SUPPORT2 = dict(zip(('uav_atomic_add uav_atomic_bitwise_ops uav_atomic_cmp_store_cmp_exch '
                     'uav_atomic_exchange uav_atomic_signed_min_max uav_atomic_unsigned_min_max '
                     'uav_typed_load uav_typed_store output_merger_logic_op tiled_resource shared_resource').split(),
                    (1 << x for x in range(11))))


def read_exact(path, digest, tier, index):
    raw = path.read_bytes()
    actual = hashlib.sha256(raw).hexdigest()
    if actual != digest.lower():
        raise ValueError(f'raw SHA-256 mismatch: actual {actual}')
    lines = raw.decode('utf-8-sig').splitlines()
    schema = lines[0].split('|')
    if len(schema) != 4 or schema[0] != 'schema' or schema[1] not in ('1', '2'):
        raise ValueError('unsupported raw schema')
    adapter = None
    current = None
    candidates = []
    for line in lines[1:]:
        parts = line.split('|')
        tag, fields = parts[0], parts[1:]
        if tag == 'adapter_begin':
            adapter = int(fields[0])
        elif tag == 'request':
            if len(fields) != 5 or fields[0] not in LEVELS or LEVELS[fields[0]] != fields[1]:
                raise ValueError('malformed or inconsistent request')
            current = {'adapter_index': adapter, 'request': fields, 'formats': {}, 'msaa': {}, 'identity': None}
            if adapter == index and fields[0] == tier:
                candidates.append(current)
        elif tag == 'adapter' and current is not None:
            if current['identity'] is not None or len(fields) != 6:
                raise ValueError('duplicate or malformed adapter identity')
            current['identity'] = dict(zip(('vendor', 'device', 'subsystem', 'luid', 'flags', 'name'), fields))
        elif tag in ('format', 'msaa') and current is not None:
            if len(fields) != (5 if tag == 'format' else 4):
                raise ValueError(f'malformed {tag} row')
            key = int(fields[0]) if tag == 'format' else (int(fields[0]), int(fields[1]))
            dest = current['formats' if tag == 'format' else 'msaa']
            if key in dest:
                raise ValueError(f'duplicate {tag} query {key}')
            dest[key] = fields[1:] if tag == 'format' else fields[2:]
        elif tag == 'identity_or_level_mismatch' and current is not None:
            current['identity_mismatch'] = True
    if len(candidates) != 1:
        raise ValueError('adapter and requested tier must identify exactly one request')
    result = candidates[0]
    request = result['request']
    if request[2:] != ['00000000', LEVELS[tier], LEVELS[tier]] or not result['identity'] or result.get('identity_mismatch'):
        raise ValueError('device creation, exact returned/GetFeatureLevel identity, or adapter identity not verified')
    if not result['formats']:
        raise ValueError('selected request contains no format queries')
    return result, actual, schema


def observation(row, device):
    use, fmt = row['use'], row['dxgi_id']
    if not fmt:
        return {'kind': 'unmapped', 'reason': 'no DXGI numeric ID'}
    fmt = int(fmt)
    if use in ('msaa_4x', 'msaa_8x'):
        samples = 4 if use == 'msaa_4x' else 8
        q = device['msaa'].get((fmt, samples))
        if q is None:
            return {'kind': 'not_queried', 'query': 'msaa', 'samples': samples}
        hr, quality = q
        return {'kind': 'query_failed' if int(hr, 16) & 0x80000000 else 'observed',
                'query': 'msaa', 'samples': samples, 'hresult': hr, 'quality_levels': int(quality),
                'supported': int(quality) > 0 if not int(hr, 16) & 0x80000000 else None}
    group = 1 if use in SUPPORT else 2 if use in SUPPORT2 else None
    if group is None:
        return {'kind': 'unmapped', 'reason': 'no direct equivalent query bit'}
    bit = (SUPPORT if group == 1 else SUPPORT2)[use]
    q = device['formats'].get(fmt)
    query = 'format_support' if group == 1 else 'format_support2'
    if q is None:
        return {'kind': 'not_queried', 'query': query, 'mask': f'{bit:08x}'}
    hr, value = q[:2] if group == 1 else q[2:]
    failed = bool(int(hr, 16) & 0x80000000)
    return {'kind': 'query_failed' if failed else 'observed', 'query': query,
            'hresult': hr, 'flags': value, 'mask': f'{bit:08x}',
            'supported': None if failed else bool(int(value, 16) & bit)}


def classify(row, obs):
    if obs['kind'] != 'observed':
        return obs['kind']
    if row['source_scope'] == 'DDI':
        return 'ddi_applicability_unresolved'
    if row['status'] == 'metadata':
        return 'metadata'
    if row['status'] == 'conditional' or ';' in row['condition']:
        return 'conditional_applicability_unresolved'
    if row['status'] == 'mandatory' and not obs['supported']:
        return 'potential_missing_mandatory'
    if row['status'] == 'unavailable' and obs['supported']:
        return 'potential_unexpected_support'
    return 'observation_consistent_or_optional'


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('log', type=Path)
    p.add_argument('--sha256', required=True, help='independently recorded raw-log SHA-256')
    p.add_argument('--tier', choices=LEVELS, required=True)
    p.add_argument('--adapter', required=True, help='enumeration index, or default for non-enumerating AppContainer logs')
    p.add_argument('--matrix', type=Path, default=ROOT / 'dxgi-format-support.csv')
    args = p.parse_args()
    if len(args.sha256) != 64 or any(c not in '0123456789abcdefABCDEF' for c in args.sha256):
        p.error('--sha256 must be 64 hexadecimal characters')
    try:
        index = None if args.adapter == 'default' else int(args.adapter)
        device, digest, schema = read_exact(args.log, args.sha256, args.tier, index)
        matrix_bytes = args.matrix.read_bytes()
        rows = list(csv.DictReader(matrix_bytes.decode('utf-8-sig').splitlines()))
        findings = []
        for row in rows:
            if row['tier'] != args.tier:
                continue
            obs = observation(row, device)
            findings.append({'id': row['id'], 'source_scope': row['source_scope'], 'format': row['format'],
                             'dxgi_id': row['dxgi_id'], 'use': row['use'], 'status': row['status'],
                             'condition': row['condition'], 'source': row['source'],
                             'classification': classify(row, obs), 'observation': obs})
        if not findings:
            raise ValueError('matrix contains no rows for selected tier')
        print(json.dumps({'schema_version': 1, 'scope': 'Query observations only; no operation, HAL, public API, or tier certification.',
                          'raw_log': str(args.log), 'raw_sha256': digest,
                          'matrix_sha256': hashlib.sha256(matrix_bytes).hexdigest(),
                          'runtime': schema[2], 'architecture': schema[3], 'tier': args.tier,
                          'adapter_index': args.adapter, 'identity': device['identity'],
                          'query_counts': {'format': len(device['formats']), 'msaa': len(device['msaa'])},
                          'counts': dict(sorted(collections.Counter(f['classification'] for f in findings).items())),
                          'findings': findings}, indent=2))
    except (ValueError, KeyError, IndexError, UnicodeError, OSError) as exc:
        p.error(str(exc))


if __name__ == '__main__':
    main()
