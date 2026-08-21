"""T2.1 — tree-sitter scaffolding + per-language extraction tests.

Tests that the shared `extract_imports_and_calls` function correctly parses
imports across Python, TypeScript, Rust, and Go using tree-sitter queries.
"""
from __future__ import annotations

import pytest

# B-090 — estes testes exercitam o parser tree-sitter, e a dep dele e declarada OPCIONAL pelo
# proprio modulo (`check_symbol_fab.tree_sitter_available`, e o golden rule trata ferramenta ausente
# como `auditor_unavailable_{tool}`, um soft cap — nao uma falha).
#
# Sem este guard, a ausencia da dep produzia oito falhas com `assert False` e `assert [] == [...]`,
# que reportam "o parser esta quebrado" quando o verdadeiro estado e "o parser nao foi carregado".
# Um teste que falha pela razao errada e pior que um teste que pula dizendo por que.
#
# A dep ESTA declarada em `.claude/requirements-dev.txt`, entao no CI eles rodam de verdade; o skip
# e para quem trabalha sem ela instalada.
from scripts.check_symbol_fab import tree_sitter_available

pytestmark = pytest.mark.skipif(
    not tree_sitter_available(),
    reason="tree-sitter-languages ausente — o modulo declara a dep como opcional",
)


from scripts.check_symbol_fab import ExtractedSymbol, extract_imports_and_calls


def _modules(symbols: list[ExtractedSymbol]) -> list[str]:
    return [s.module for s in symbols if s.kind == "import"]


# --- Python ---


def test_extract_python_import_statement(tmp_path) -> None:
    src = tmp_path / "x.py"
    src.write_text("import requests\n")
    symbols = extract_imports_and_calls(src, "python")
    assert "requests" in _modules(symbols)


def test_extract_python_from_import(tmp_path) -> None:
    src = tmp_path / "x.py"
    src.write_text("from foo.bar import baz\n")
    symbols = extract_imports_and_calls(src, "python")
    modules = _modules(symbols)
    assert any(m.startswith("foo") for m in modules)


def test_extract_python_relative_import_marked(tmp_path) -> None:
    src = tmp_path / "x.py"
    src.write_text("from . import sibling\nfrom ..parent import x\n")
    symbols = extract_imports_and_calls(src, "python")
    # Relative imports MUST be marked with leading dot so detectors skip registry lookup
    modules = _modules(symbols)
    assert any(m.startswith(".") for m in modules)


# --- TypeScript ---


def test_extract_typescript_named_import(tmp_path) -> None:
    src = tmp_path / "x.ts"
    src.write_text("import { foo } from 'bar';\n")
    symbols = extract_imports_and_calls(src, "typescript")
    assert "bar" in _modules(symbols)


def test_extract_typescript_relative_import_marked(tmp_path) -> None:
    src = tmp_path / "x.ts"
    src.write_text("import { foo } from './sibling';\n")
    symbols = extract_imports_and_calls(src, "typescript")
    modules = _modules(symbols)
    # Relative imports are kept verbatim — detector decides to skip via path prefix
    assert any(m.startswith("./") or m.startswith("../") for m in modules)


# --- Rust ---


def test_extract_rust_use_statement(tmp_path) -> None:
    src = tmp_path / "x.rs"
    src.write_text("use serde::Deserialize;\n")
    symbols = extract_imports_and_calls(src, "rust")
    modules = _modules(symbols)
    assert any(m.startswith("serde") for m in modules)


# --- Go ---


def test_extract_go_import_statement(tmp_path) -> None:
    src = tmp_path / "x.go"
    src.write_text(
        'package main\n\nimport "github.com/spf13/cobra"\n\nfunc main() {}\n'
    )
    symbols = extract_imports_and_calls(src, "go")
    assert "github.com/spf13/cobra" in _modules(symbols)


# --- Robustness ---


def test_tree_sitter_handles_syntax_error_gracefully(tmp_path) -> None:
    """EC-8 — malformed code MUST NOT crash the parser; return empty or partial."""
    src = tmp_path / "broken.py"
    src.write_text("def foo(  # missing close paren and body\n")
    # MUST NOT raise
    symbols = extract_imports_and_calls(src, "python")
    assert isinstance(symbols, list)


def test_canary_known_findings_count(tmp_path) -> None:
    """EC-14 — canary: known imports count MUST match expectation; grammar regression guard."""
    src = tmp_path / "canary.py"
    src.write_text(
        "import requests\n"
        "from pathlib import Path\n"
        "import os\n"
    )
    symbols = extract_imports_and_calls(src, "python")
    modules = set(_modules(symbols))
    # Expect at least 3 distinct modules detected
    assert len(modules) >= 3, f"Expected ≥ 3 modules, got: {modules}"


def test_extract_returns_empty_for_unsupported_language(tmp_path) -> None:
    src = tmp_path / "x.unknown"
    src.write_text("anything\n")
    with pytest.raises(ValueError, match="unsupported language"):
        extract_imports_and_calls(src, "ruby")
