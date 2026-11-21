"""Normalize an exact-level native probe log without conflating it with backend evidence."""
import argparse
import hashlib
import json
from pathlib import Path

LEVELS = {"9_1": "9100", "9_2": "9200", "9_3": "9300", "10_0": "a000",
          "10_1": "a100", "11_0": "b000", "11_1": "b100"}

def read_log(path):
    data = path.read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    rows = data.decode(encoding).splitlines()
    tag, version, runtime, arch = rows[0].split("|")
    if tag != "schema" or version not in ("1", "2"):
        raise ValueError(f"{path}: unsupported log format")
    results = {"probe_schema": int(version), "runtime": runtime, "architecture": arch, "raw_sha256": hashlib.sha256(data).hexdigest(),
               "raw_log": path.name, "adapters": [], "overall_smoke": None}
    adapter = None
    test = None
    for line in rows[1:]:
        tag, *fields = line.split("|")
        if tag == "adapter_begin":
            adapter = {"enumeration_index": int(fields[0]), "requests": []}
            results["adapters"].append(adapter)
        elif tag == "request":
            if adapter is None:
                adapter = {"enumeration_index": None, "requests": []}
                results["adapters"].append(adapter)
            tier, enum, status, returned, actual = fields
            if LEVELS[tier] != enum:
                raise ValueError(f"{path}: {tier} encoded as {enum}")
            test = {"requested": tier, "requested_enum": enum, "create_hresult": status,
                    "returned_enum": returned, "get_feature_level_enum": actual,
                    "exact_match": status == "00000000" and returned == actual == enum,
                    "identity": None, "checks": {}, "format_query_count": 0, "msaa_query_count": 0}
            adapter["requests"].append(test)
        elif tag == "adapter":
            vendor, device, subsystem, luid, flags, name = fields
            test["identity"] = {"name": name, "vendor": vendor, "device": device, "subsystem": subsystem,
                                "luid": luid, "flags": flags, "software": bool(int(flags, 16) & 2)}
        elif tag == "options":
            test["options"] = dict(field.split("=", 1) for field in fields)
        elif tag == "format":
            test["format_query_count"] += 1
        elif tag == "msaa":
            test["msaa_query_count"] += 1
        elif tag == "operation":
            name, status = fields
            if name in test["checks"]:
                raise ValueError(f"{path}: repeated operation {name!r}")
            test["checks"][name] = status
        elif tag in ("observation", "stage") and version == "2":
            name, *observation = fields
            group = test.setdefault("observations" if tag == "observation" else "stages", {})
            if name in group:
                raise ValueError(f"{path}: repeated {tag} {name!r}")
            group[name] = observation
        elif tag == "device_status":
            test["device_removed_reason"] = fields[0]
        elif tag == "11_1_invalidarg_control":
            test["runtime_control"] = {"requested": fields[0], "hresult": fields[1]}
        elif tag == "identity_or_level_mismatch":
            test["identity_or_level_mismatch"] = fields[0]
        elif tag.startswith("overall="):
            results["overall_smoke"] = tag.split("=", 1)[1]
        elif tag != "enumeration":
            raise ValueError(f"{path}: unexpected report row {tag!r}")
    for adapter in results["adapters"]:
        if [test["requested"] for test in adapter["requests"]] != list(LEVELS):
            raise ValueError(f"{path}: missing or repeated exact-level requests")
        for test in adapter["requests"]:
            identity = test["identity"]
            if test["exact_match"] and identity and not identity["software"]:
                test["hardware_confirmed"] = True
            else:
                test["hardware_confirmed"] = False
    if results["overall_smoke"] not in ("PASS", "PARTIAL", "FAIL"):
        raise ValueError(f"{path}: missing final result")
    return results

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", type=Path, nargs="+")
    args = parser.parse_args()
    print(json.dumps([read_log(path) for path in args.logs], indent=2))

if __name__ == "__main__":
    main()
