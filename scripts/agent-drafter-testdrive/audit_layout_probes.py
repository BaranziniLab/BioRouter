from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
STORE = ROOT / ".br-testdrive/runtime/config/biorouter/agent_drafter"
PROVIDER = "versa_azure"
MODEL = "gpt-5.5-2026-04-24"
PROBES = {
    "layout-probe-kpi-mosaic": ("dashboard", "clinical"),
    "layout-probe-centered-wizard": ("wizard", "journal"),
    "layout-probe-radial-canvas": ("canvas", "midnight"),
    "layout-probe-tabletop-workbench": ("workbench", "lab-notebook"),
    "layout-probe-constellation": ("explorer", "terminal"),
}


def model_refs(value: Any, path: str = "") -> list[tuple[str, dict[str, Any]]]:
    found: list[tuple[str, dict[str, Any]]] = []
    if isinstance(value, dict):
        if isinstance(value.get("provider"), str) or isinstance(value.get("model"), str):
            found.append((path or "/", value))
        for key, child in value.items():
            found.extend(model_refs(child, f"{path}/{key}"))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            found.extend(model_refs(child, f"{path}/{index}"))
    return found


def audit(identifier: str, family: str, pack: str) -> dict[str, Any]:
    app = STORE / identifier
    issues: list[str] = []
    manifest_path = app / "manifest.json"
    if not manifest_path.exists():
        return {"id": identifier, "built": False, "issues": ["manifest missing"]}
    manifest = json.loads(manifest_path.read_text())
    html = (app / "index.html").read_text(errors="replace")
    source = (app / "src/main.ts").read_text(errors="replace")
    built = (app / "dist/app.js").exists() and (app / "dist/app.js").stat().st_size > 500
    if not built:
        issues.append("dist/app.js missing or too small")
    if manifest.get("kind") != "agentic":
        issues.append("kind is not agentic")
    if f'data-layout-probe="{identifier}"' not in html:
        issues.append("body probe marker missing")
    if f'data-layout-family="{family}"' not in html:
        issues.append(f"layout family marker is not {family}")
    if re.search(r"\b(left|right)[-_ ]?(sidebar|rail|inspector)\b", html, re.I):
        issues.append("persistent sidebar/rail terminology found in authored HTML")
    if "@media" not in html:
        issues.append("no responsive media rule")
    if "type=\"range\"" not in html and "type='range'" not in html:
        issues.append("direct-manipulation slider missing")
    if 'actions.register("activate_probe"' not in source:
        issues.append("activate_probe action is not registered")
    if 'signals.emit("probe_adjusted"' not in source:
        issues.append("probe_adjusted signal is not emitted")
    if 'br.call("activate_probe"' not in source:
        issues.append("primary control does not call activate_probe")
    surface = manifest.get("surface") or {}
    actions = {item.get("name") for item in surface.get("actions", []) if isinstance(item, dict)}
    signals = {item.get("name") for item in surface.get("signals", []) if isinstance(item, dict)}
    if "activate_probe" not in actions:
        issues.append("activate_probe missing from manifest surface")
    if "probe_adjusted" not in signals:
        issues.append("probe_adjusted missing from manifest surface")
    orchestration = ((manifest.get("agent") or {}).get("orchestration") or {})
    agents = {
        **(orchestration.get("sub_agents") or {}),
        **(orchestration.get("agents") or {}),
    }
    for worker in ("layout_critic", "interaction_auditor"):
        if worker not in agents:
            issues.append(f"worker {worker} missing")
    for path, ref in model_refs(manifest.get("agent") or {}, "/agent"):
        if ref.get("provider") != PROVIDER or ref.get("model") != MODEL:
            issues.append(f"non-UCSF model at {path}: {ref}")
    actual_pack = (manifest.get("theme") or {}).get("pack") or "biorouter"
    if actual_pack != pack:
        issues.append(f"theme resolves to {actual_pack}, expected {pack}")
    if pack in {"midnight", "terminal"} and not re.search(
        r'createApp\s*\(\s*\{[^}]*theme\s*:\s*["\'](?:auto|dark)["\']',
        source,
        re.S,
    ):
        issues.append(f"{pack} probe does not initialize createApp with a dark/auto theme")
    return {
        "id": identifier,
        "family": family,
        "theme": actual_pack,
        "built": built,
        "regions": sorted(set(re.findall(r'data-br-region=["\']([^"\']+)', html))),
        "issues": issues,
    }


def main() -> None:
    results = [audit(identifier, *config) for identifier, config in PROBES.items()]
    print(
        json.dumps(
            {
                "provider": PROVIDER,
                "model": MODEL,
                "passed": sum(not result["issues"] for result in results),
                "total": len(results),
                "results": results,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
