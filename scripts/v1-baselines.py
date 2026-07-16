#!/usr/bin/env python3
import difflib
import hashlib
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SNAPSHOT_PATH = ROOT / "api/baselines/public-artifacts-v1.json"
ACCEPTED_BASE = "9aff9b40db1597364279f9aacf47d436718a031e"

REQUIRED_AUTHORING_FAMILIES = {
    "transport",
    "group",
    "voice",
    "pattern",
    "melody",
    "sequence",
    "fade",
    "effect",
    "output_route",
    "sample",
    "sfz",
    "buffer",
    "recording",
    "synthdef",
    "effectdef",
    "dsp_value",
    "midi",
}
REQUIRED_NEGATIVE_DEFECTS = {
    "ignored_fields",
    "stale_success",
    "invalid_ugen_labels",
    "semantic_token_mismatch",
    "push_pull_diagnostic_mismatch",
    "stale_commands",
    "wasm_bridge_false_success",
}
REQUIRED_FAILURE_PHASES = {
    "parse",
    "evaluate",
    "graph_compile",
    "validate",
    "plan",
    "stage_resources",
    "transport",
    "stop_deleted",
    "delete",
    "open_midi",
    "create",
    "update",
    "output_routes",
    "input_routes",
    "effects",
    "groups",
    "fades",
    "start_running",
    "param_routes",
    "midi_routes",
    "barrier",
    "commit",
    "cleanup",
    "wasm_bridge",
    "wasm_dispatch",
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def read_json(relative):
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def file_record(relative):
    data = (ROOT / relative).read_bytes()
    return {
        "path": relative,
        "bytes": len(data),
        "sha256": sha256(data),
    }


def category(paths, counts):
    records = [file_record(path) for path in sorted(paths)]
    tree = hashlib.sha256()
    for record in records:
        tree.update(record["path"].encode())
        tree.update(b"\0")
        tree.update(record["sha256"].encode())
        tree.update(b"\0")
        tree.update(str(record["bytes"]).encode())
        tree.update(b"\n")
    return {
        "counts": {
            "files": len(records),
            "bytes": sum(record["bytes"] for record in records),
            **counts,
        },
        "tree_sha256": tree.hexdigest(),
        "files": records,
    }


def manifest_counts(manifest):
    entries = manifest["entries"]
    return {
        "entries": len(entries),
        "overloads": sum(len(entry["overloads"]) for entry in entries),
        "registered_types": sum(entry["kind"] == "type" for entry in entries),
        "properties": sum(entry["kind"] in {"property_get", "property_set"} for entry in entries),
        "named_terminals": sum(
            entry.get("lifecycle", {}).get("terminal") == "named_terminal" for entry in entries
        ),
    }


def http_counts(snapshot):
    methods = {}
    for route in snapshot["routes"]:
        methods[route["method"]] = methods.get(route["method"], 0) + 1
    fields = sum(len(item.get("fields", [])) for item in snapshot["types"])
    return {
        "routes": len(snapshot["routes"]),
        "types": len(snapshot["types"]),
        "fields": fields,
        "mutating_routes": sum(route["method"] != "GET" for route in snapshot["routes"]),
        "methods": dict(sorted(methods.items())),
    }


def editor_counts():
    rhai = read_json("vscode-extension/src/data/rhai-api.json")
    lsp = read_json("crates/vibelang-lsp/src/data/rhai-api.json")
    stdlib = read_json("vscode-extension/src/data/stdlib.json")
    canonical_ugens = sorted((ROOT / "crates/vibelang-dsp/ugen_manifests").glob("*.json"))
    vscode_ugens = sorted((ROOT / "vscode-extension/ugen_manifests").glob("*.json"))
    lsp_ugens = sorted((ROOT / "crates/vibelang-lsp/src/data/ugen_manifests").glob("*.json"))
    return {
        "rhai_rows": len(rhai),
        "lsp_rhai_rows": len(lsp),
        "rhai_projections_equal": rhai == lsp,
        "stdlib_rows": len(stdlib["synthdefs"]),
        "canonical_ugen_categories": len(canonical_ugens),
        "vscode_ugen_categories": len(vscode_ugens),
        "lsp_ugen_categories": len(lsp_ugens),
    }


def wasm_counts(text):
    return {
        "interfaces": len(re.findall(r"^export interface ", text, re.MULTILINE)),
        "classes": len(re.findall(r"^export class ", text, re.MULTILINE)),
        "class_methods": len(re.findall(r"^  (?:static )?[A-Za-z][A-Za-z0-9]*\(", text, re.MULTILINE)),
        "module_functions": len(re.findall(r"^(?:export default function|export function) ", text, re.MULTILINE)),
    }


def docs_counts(paths):
    texts = [(ROOT / path).read_text(encoding="utf-8") for path in paths]
    return {
        "lines": sum(text.count("\n") for text in texts),
        "vibe_fences": sum(len(re.findall(r"^```(?:rhai|vibe)$", text, re.MULTILINE)) for text in texts),
        "shell_fences": sum(len(re.findall(r"^```(?:bash|sh|shell)$", text, re.MULTILINE)) for text in texts),
    }


def package_counts(paths):
    cargo_packages = []
    node_packages = []
    for path in paths:
        text = (ROOT / path).read_text(encoding="utf-8")
        if path.endswith("Cargo.toml"):
            match = re.search(r"^name\s*=\s*\"([^\"]+)\"", text, re.MULTILINE)
            if match:
                cargo_packages.append(match.group(1))
        elif path.endswith("package.json"):
            node_packages.append(json.loads(text).get("name", "<unnamed>"))
    return {
        "cargo_packages": sorted(cargo_packages),
        "node_packages": sorted(node_packages),
        "lockfiles": sum(path.endswith(("Cargo.lock", "package-lock.json")) for path in paths),
    }


def relative_paths(directory, pattern="*"):
    return [str(path.relative_to(ROOT)) for path in directory.glob(pattern) if path.is_file()]


def validate_catalogs():
    authoring = read_json("api/baselines/authoring-families-v1.json")
    families = {entry["family"] for entry in authoring["families"]}
    if families != REQUIRED_AUTHORING_FAMILIES:
        raise ValueError(
            f"authoring family catalog drift: expected {sorted(REQUIRED_AUTHORING_FAMILIES)}, got {sorted(families)}"
        )
    for entry in authoring["families"]:
        for key in ("script", "snapshot"):
            if not (ROOT / entry[key]).is_file():
                raise ValueError(f"missing authoring {key}: {entry[key]}")

    negative = read_json("api/baselines/known-defects-v1.json")
    defects = {entry["id"] for entry in negative["defects"]}
    if defects != REQUIRED_NEGATIVE_DEFECTS:
        raise ValueError(
            f"negative fixture catalog drift: expected {sorted(REQUIRED_NEGATIVE_DEFECTS)}, got {sorted(defects)}"
        )
    for entry in negative["defects"]:
        fixture_path = ROOT / entry["fixture"]
        if not fixture_path.is_file():
            raise ValueError(f"missing negative fixture: {entry['fixture']}")
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        for assertion in fixture.get("source_assertions", []):
            source = (ROOT / assertion["path"]).read_text(encoding="utf-8")
            if assertion["contains"] not in source:
                raise ValueError(
                    f"known defect {entry['id']} no longer matches {assertion['path']}: {assertion['contains']!r}"
                )

    seams = read_json("api/baselines/failure-injection-seams-v1.json")
    phases = {entry["phase"] for entry in seams["seams"]}
    if phases != REQUIRED_FAILURE_PHASES:
        raise ValueError(
            f"failure phase catalog drift: expected {sorted(REQUIRED_FAILURE_PHASES)}, got {sorted(phases)}"
        )
    for entry in seams["seams"]:
        source = (ROOT / entry["source"]).read_text(encoding="utf-8")
        if entry["anchor"] not in source:
            raise ValueError(
                f"failure seam {entry['phase']} no longer matches {entry['source']}: {entry['anchor']!r}"
            )


def build_snapshot():
    validate_catalogs()
    manifest = read_json("api/public-api-manifest-v1.json")
    http = read_json("api/http-api-snapshot-v1.json")

    editor_paths = [
        "vscode-extension/src/data/rhai-api.json",
        "crates/vibelang-lsp/src/data/rhai-api.json",
        "vscode-extension/src/data/stdlib.json",
    ]
    editor_paths += relative_paths(ROOT / "vscode-extension/ugen_manifests", "*.json")
    editor_paths += relative_paths(ROOT / "crates/vibelang-lsp/src/data/ugen_manifests", "*.json")

    doc_paths = [
        "api/README.md",
        "docs/reference/README.md",
        "docs/reference/runtime-model.md",
        "docs/reference/runtime-objects.md",
        "docs/interfaces/cli-and-config.md",
        "docs/interfaces/http-and-websocket.md",
        "docs/interfaces/lsp-and-editors.md",
        "docs/interfaces/wasm.md",
        "docs/reference/generated/stdlib.md",
        "docs/reference/generated/ugens.md",
        "docs/reference/generated/http-routes.md",
    ]

    package_paths = []
    for pattern in ("Cargo.toml", "Cargo.lock", "package.json", "package-lock.json"):
        for path in ROOT.rglob(pattern):
            relative = path.relative_to(ROOT)
            if "target" not in relative.parts and "node_modules" not in relative.parts:
                package_paths.append(str(relative))
    package_paths = sorted(set(package_paths))

    fixture_paths = relative_paths(ROOT / "api/baselines", "*.json")
    fixture_paths = [path for path in fixture_paths if path != "api/baselines/public-artifacts-v1.json"]
    fixture_paths += relative_paths(ROOT / "tests/fixtures/api-unification/v1", "**/*")

    cli_text = (ROOT / "docs/reference/generated/cli-help.txt").read_text(encoding="utf-8")
    wasm_text = (ROOT / "crates/vibelang-wasm/types/index.d.ts").read_text(encoding="utf-8")
    return {
        "schema": "https://vibelang.org/schemas/v1-public-artifact-baseline/1",
        "schema_version": 1,
        "accepted_base": ACCEPTED_BASE,
        "policy": "observed-v1-behavior-not-endorsement",
        "categories": {
            "manifest": category(
                ["api/public-api-manifest-v1.json"], manifest_counts(manifest)
            ),
            "http": category(
                ["api/http-api-snapshot-v1.json"], http_counts(http)
            ),
            "rhai_editor": category(editor_paths, editor_counts()),
            "wasm": category(
                ["crates/vibelang-wasm/types/index.d.ts"], wasm_counts(wasm_text)
            ),
            "cli": category(
                ["docs/reference/generated/cli-help.txt"],
                {
                    "help_snapshots": len(re.findall(r"^\$ vibe", cli_text, re.MULTILINE)),
                    "top_level_commands": len(
                        re.findall(r"^  (?:run|render|devices|lsp|help)\s", cli_text, re.MULTILINE)
                    ),
                },
            ),
            "docs": category(doc_paths, docs_counts(doc_paths)),
            "packages": category(package_paths, package_counts(package_paths)),
            "fixtures": category(
                fixture_paths,
                {
                    "authoring_families": len(REQUIRED_AUTHORING_FAMILIES),
                    "negative_defects": len(REQUIRED_NEGATIVE_DEFECTS),
                    "failure_phases": len(REQUIRED_FAILURE_PHASES),
                },
            ),
        },
    }


def render(snapshot):
    return json.dumps(snapshot, indent=2, sort_keys=True) + "\n"


def check(expected_text, actual_text):
    if expected_text == actual_text:
        return None
    return "".join(
        difflib.unified_diff(
            expected_text.splitlines(keepends=True),
            actual_text.splitlines(keepends=True),
            fromfile=str(SNAPSHOT_PATH.relative_to(ROOT)),
            tofile="regenerated-v1-baseline",
        )
    )


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in {"generate", "check", "test-drift"}:
        print("usage: scripts/v1-baselines.py <generate|check|test-drift>", file=sys.stderr)
        return 2

    mode = sys.argv[1]
    actual = render(build_snapshot())
    if mode == "generate":
        SNAPSHOT_PATH.parent.mkdir(parents=True, exist_ok=True)
        SNAPSHOT_PATH.write_text(actual, encoding="utf-8")
        print(f"generated {SNAPSHOT_PATH.relative_to(ROOT)}")
        return 0

    if mode == "check":
        expected = SNAPSHOT_PATH.read_text(encoding="utf-8")
        diff = check(expected, actual)
        if diff is not None:
            print(diff, file=sys.stderr)
            print("v1 public-artifact baseline drifted; classify the change before regenerating", file=sys.stderr)
            return 1
        print("v1 public-artifact baseline is current")
        return 0

    mutated = json.loads(actual)
    mutated["categories"]["manifest"]["files"][0]["bytes"] += 1
    if check(actual, render(mutated)) is None:
        print("drift self-test failed to detect a deterministic byte change", file=sys.stderr)
        return 1
    print("v1 baseline drift self-test detected a deterministic byte change")
    return 0


if __name__ == "__main__":
    sys.exit(main())
