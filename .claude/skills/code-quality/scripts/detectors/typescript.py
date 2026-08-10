"""TypeScript detector — wraps knip (D1) + npm registry lookup (D2) + stryker (D4).

T1.2 implementation: detect_dead_code via knip subprocess.
T2.3 implementation: detect_symbol_fabrication via tree-sitter + npm lookup.
Other methods still stubs (T3.1 / T4.2).
"""
from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

from scripts import _registry
from scripts._shared import Finding, safe_parse_json, sanitize_symbol, to_rel_path
from scripts.check_symbol_fab import extract_imports_and_calls

from . import BaseDetector

_TS_NODE_BUILTINS = frozenset(
    {
        "fs", "path", "os", "util", "crypto", "http", "https", "url", "stream",
        "events", "buffer", "child_process", "cluster", "dgram", "dns", "net",
        "querystring", "readline", "tls", "tty", "vm", "zlib", "assert",
        "string_decoder", "process", "module", "perf_hooks", "worker_threads",
        "console", "timers", "domain", "punycode", "v8", "inspector",
    }
)

_KNIP_TIMEOUT_SEC = 120


class TypescriptDetector(BaseDetector):
    language = "typescript"
    manifest_marker = "package.json"

    def detect_dead_code(self, repo_root: Path) -> list[Finding]:
        """Run knip against `repo_root` and parse JSON into Findings.

        knip emits exit code 0 (no findings) or 1 (findings). Exit code > 1
        signals tool error and is treated as `auditor_unavailable_knip`.
        """
        cmd = ["npx", "--yes", "knip", "--reporter", "json"]
        try:
            result = subprocess.run(
                cmd,
                cwd=str(repo_root),
                capture_output=True,
                text=True,
                timeout=_KNIP_TIMEOUT_SEC,
                check=False,
            )
        except FileNotFoundError:
            return [self._auditor_unavailable("knip not found in PATH (install via npm i -g knip)")]
        except subprocess.TimeoutExpired:
            return [self._auditor_unavailable(f"knip timed out after {_KNIP_TIMEOUT_SEC}s")]
        except (subprocess.SubprocessError, OSError) as e:
            return [self._auditor_unavailable(f"knip invocation failed: {e}")]

        if result.returncode > 1:
            return [
                self._auditor_unavailable(
                    f"knip exit code {result.returncode}: {result.stderr.strip()[:200]}"
                )
            ]

        data, parse_finding = safe_parse_json(result.stdout, "knip")
        if parse_finding is not None:
            # Re-emit with TypeScript-specific language metadata + safe allowlist_key.
            return [
                Finding(
                    detector="d1_dead_code",
                    language="typescript",
                    severity="SOFT_CAP",
                    file_path=".",
                    symbol_or_line="knip",
                    message=f"knip JSON output failed to parse: {parse_finding.message}",
                    allowlist_key="typescript|.|dead_code|auditor_output_malformed_knip",
                )
            ]
        return self._parse_knip_json(data, repo_root)

    def _find_self_package_name(self, changed_files: list[Path]) -> str | None:
        """Walk up from any changed file to find the repo's package.json#name.

        Used to skip self-reference imports (`@scope/pkg-name` and its subpaths) when the
        codebase is the package being imported — pre-publication code legitimately self-references
        via workspace links (file:..) before the package ships to the npm registry. Cached on
        the detector instance after first lookup.
        """
        if hasattr(self, "_cached_self_name"):
            return self._cached_self_name
        self._cached_self_name = None
        for src_file in changed_files:
            try:
                cur = src_file.resolve().parent if src_file.exists() else Path.cwd()
            except OSError:
                continue
            for parent in [cur, *cur.parents]:
                pkg_json = parent / "package.json"
                if pkg_json.is_file():
                    try:
                        data = json.loads(pkg_json.read_text(encoding="utf-8"))
                        name = data.get("name")
                        # We want the OUTERMOST (root) name when nested workspaces exist —
                        # keep walking up after finding a name. A demo subdir's package name
                        # is shadowed by the root package's name.
                        if isinstance(name, str) and name:
                            self._cached_self_name = name
                    except (json.JSONDecodeError, OSError):
                        pass
            if self._cached_self_name:
                break
        return self._cached_self_name

    def _find_workspace_package_names(self, changed_files: list[Path]) -> frozenset[str]:
        """Every package name declared INSIDE this repo — not just the root's.

        Patch 2026-08-03. `_find_self_package_name` resolves the OUTERMOST package.json#name,
        which in a monorepo is the private root (`theo-promptly`) that nobody imports. Every
        sibling import (`@usetheo/promptly` from packages/api) therefore fell through to the
        npm registry, took a 404 and was reported as `Fabricated npm package` — 60 HARD
        findings on a repo whose build and tests are green. A workspace dependency declared
        `workspace:*` resolves perfectly; it is simply not published, by design.

        Bounded walk: from each changed file up to the repo root, then one level down over the
        declared workspace globs. Cached per detector instance.
        """
        if hasattr(self, "_cached_ws_names"):
            return self._cached_ws_names
        names: set[str] = set()
        roots: set[Path] = set()
        for src_file in changed_files:
            try:
                cur = src_file.resolve().parent if src_file.exists() else Path.cwd()
            except OSError:
                continue
            for parent in [cur, *cur.parents]:
                if (parent / ".git").exists() or (parent / "pnpm-workspace.yaml").is_file():
                    roots.add(parent)
                    break
        for root in roots:
            for pkg_json in root.glob("*/*/package.json"):
                self._read_pkg_name(pkg_json, names)
            for pkg_json in root.glob("*/package.json"):
                self._read_pkg_name(pkg_json, names)
        self._cached_ws_names = frozenset(names)
        return self._cached_ws_names

    @staticmethod
    def _read_pkg_name(pkg_json: Path, sink: set[str]) -> None:
        try:
            name = json.loads(pkg_json.read_text(encoding="utf-8")).get("name")
        except (json.JSONDecodeError, OSError):
            return
        if isinstance(name, str) and name:
            sink.add(name)

    @staticmethod
    def _is_self_reference(module: str, self_name: str | None) -> bool:
        if not self_name:
            return False
        return module == self_name or module.startswith(self_name + "/")

    @staticmethod
    def _is_workspace_reference(module: str, ws_names: frozenset[str]) -> bool:
        return any(module == n or module.startswith(n + "/") for n in ws_names)


    def _find_path_aliases(self, changed_files: list[Path]) -> tuple[str, ...]:
        """Prefixes declared as tsconfig `compilerOptions.paths` — local, never npm.

        Patch 2026-08-03, third false-positive family in this detector. `@/components/Button`
        is a PATH ALIAS, not a scoped package: the leading `@` is a convention, and the module
        resolves through tsconfig, not the registry. Treating every `@`-prefixed specifier as a
        scope produced 986 HARD findings in one dashboard, every one of them false.

        The three families share a single root — the detector resolved module names against the
        public registry without consulting what the project itself declares (workspaces,
        exports, paths). This reads the declaration instead of guessing.
        """
        if hasattr(self, "_cached_aliases"):
            return self._cached_aliases
        aliases: set[str] = set()
        seen_roots: set[Path] = set()
        for src_file in changed_files:
            try:
                cur = src_file.resolve().parent if src_file.exists() else Path.cwd()
            except OSError:
                continue
            for _ in range(8):  # bounded walk to the repo root
                if cur in seen_roots:
                    break
                seen_roots.add(cur)
                for name in ("tsconfig.json", "tsconfig.base.json", "jsconfig.json"):
                    cfg = cur / name
                    if not cfg.exists():
                        continue
                    try:
                        raw = cfg.read_text(encoding="utf-8", errors="replace")
                        # tsconfig admite comentarios; JSON estrito falharia
                        raw = re.sub(r"//[^\n]*", "", raw)
                        raw = re.sub(r"/\*.*?\*/", "", raw, flags=re.DOTALL)
                        raw = re.sub(r",(\s*[}\]])", r"\1", raw)
                        data = json.loads(raw)
                    except (OSError, ValueError):
                        continue
                    paths = (data.get("compilerOptions") or {}).get("paths") or {}
                    for pattern in paths:
                        aliases.add(pattern.split("*")[0].rstrip("/"))
                if (cur / ".git").exists() or cur.parent == cur:
                    break
                cur = cur.parent
        self._cached_aliases = tuple(sorted(a for a in aliases if a))
        return self._cached_aliases

    @staticmethod
    def _is_path_alias(module: str, aliases: tuple[str, ...]) -> bool:
        """True when the specifier matches a declared tsconfig path alias."""
        return any(module == a or module.startswith(a + "/") for a in aliases)

    def detect_symbol_fabrication(self, changed_files: list[Path]) -> list[Finding]:
        """T2.3 — Validate imports against npm. Skip relative + node: builtins + monorepo subpath (EC-16) + self-references (patch 2026-05-30)."""
        findings: list[Finding] = []
        self_name = self._find_self_package_name(changed_files)
        ws_names = self._find_workspace_package_names(changed_files)
        aliases = self._find_path_aliases(changed_files)
        for src_file in changed_files:
            if not src_file.exists():
                continue
            rel = to_rel_path(src_file)
            for sym in extract_imports_and_calls(src_file, "typescript"):
                if sym.kind != "import":
                    continue
                module = sym.module
                if not module:
                    continue
                # Relative imports
                if module.startswith("./") or module.startswith("../") or module == "." or module == "..":
                    continue
                # Node builtins
                if module.startswith("node:"):
                    continue
                top = module.split("/")[0] if not module.startswith("@") else "/".join(module.split("/")[:2])
                if top in _TS_NODE_BUILTINS:
                    continue
                # Patch 2026-05-30 — Self-reference (the workspace IS the package being imported)
                if self._is_self_reference(module, self_name):
                    continue
                # Patch 2026-08-03 — Sibling workspace package (declared `workspace:*`, unpublished by design)
                if self._is_workspace_reference(module, ws_names):
                    continue
                # Patch 2026-08-03 — tsconfig path alias (`@/components/...`), nao pacote npm
                if self._is_path_alias(module, aliases):
                    continue
                # Package name for npm lookup. `top` already collapses a scoped module to
                # `@scope/name`; using `module` here sent the whole SUBPATH to the registry
                # (`@modelcontextprotocol/sdk/server/mcp.js` is not a package name), which is
                # what produced 52 `ambiguous response` findings against a real, installed SDK.
                pkg = top if module.startswith("@") else module.split("/")[0]
                exists = _registry.package_exists_on_npm(pkg)
                if exists is True:
                    continue
                sanitized = sanitize_symbol(pkg)
                if exists is False:
                    findings.append(
                        Finding(
                            detector="d2_symbol_fab",
                            language="typescript",
                            severity="HARD",
                            file_path=rel,
                            symbol_or_line=f"import from '{module}'",
                            message=f"Fabricated npm package '{pkg}' (not found on registry)",
                            allowlist_key=f"typescript|{rel}|symbol_fab|{sanitized}",
                        )
                    )
                else:
                    findings.append(
                        Finding(
                            detector="d2_symbol_fab",
                            language="typescript",
                            severity="SOFT_FLOOR",
                            file_path=rel,
                            symbol_or_line=f"import from '{module}'",
                            message=f"Could not verify npm package '{pkg}' (ambiguous response)",
                            allowlist_key=f"typescript|{rel}|symbol_fab|symbol_fab_unverifiable_{sanitized}",
                        )
                    )
        return findings

    def detect_orphan_exports(self, repo_root: Path) -> list[Finding]:
        raise NotImplementedError("T3.1: cross-package wiring detector not yet implemented")

    def detect_mutation_score(self, critical_paths: list[Path]) -> list[Finding]:
        raise NotImplementedError("T4.2: stryker wrapper not yet implemented")

    # ------------------------------------------------------------------
    # internal helpers
    # ------------------------------------------------------------------

    def _parse_knip_json(self, data: dict, repo_root: Path) -> list[Finding]:
        findings: list[Finding] = []

        for file_path in data.get("files", []) or []:
            findings.append(self._make_finding(file_path, "unimported file", "file", repo_root))

        for export in data.get("exports", []) or []:
            file_path = export.get("file", "<unknown>")
            name = export.get("name", "<unknown>")
            findings.append(
                self._make_finding(file_path, f"unused export '{name}'", name, repo_root)
            )

        for dep in data.get("dependencies", []) or []:
            name = dep.get("name") if isinstance(dep, dict) else str(dep)
            findings.append(
                self._make_finding("package.json", f"unused dependency '{name}'", name, repo_root)
            )

        for dep in data.get("devDependencies", []) or []:
            name = dep.get("name") if isinstance(dep, dict) else str(dep)
            findings.append(
                self._make_finding(
                    "package.json", f"unused devDependency '{name}'", name, repo_root
                )
            )

        return findings

    def _make_finding(
        self, file_path: str, message: str, symbol: str, repo_root: Path
    ) -> Finding:
        rel = self._safe_relative(file_path, repo_root)
        sanitized = sanitize_symbol(symbol)
        return Finding(
            detector="d1_dead_code",
            language="typescript",
            severity="HARD",
            file_path=rel,
            symbol_or_line=f"{symbol} @ {rel}",
            message=message,
            allowlist_key=f"typescript|{rel}|dead_code|{sanitized}",
        )

    @staticmethod
    def _safe_relative(file_path: str, repo_root: Path) -> str:
        path = Path(file_path)
        if path.is_absolute():
            try:
                return path.relative_to(repo_root.resolve()).as_posix()
            except ValueError:
                return path.as_posix().lstrip("/")
        return path.as_posix()

    def _auditor_unavailable(self, reason: str) -> Finding:
        return Finding(
            detector="d1_dead_code",
            language="typescript",
            severity="SOFT_CAP",
            file_path=".",
            symbol_or_line="knip",
            message=f"Knip auditor unavailable: {reason}",
            allowlist_key="typescript|.|dead_code|auditor_unavailable_knip",
        )
