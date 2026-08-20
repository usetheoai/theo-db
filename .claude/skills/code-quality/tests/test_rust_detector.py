"""T1.3 — RustDetector.detect_dead_code (cargo-udeps wrapper) tests.

cargo-udeps requires nightly toolchain. Tests use subprocess mocks.
"""
from __future__ import annotations

import json
from pathlib import Path
from unittest.mock import patch

import pytest

from scripts.detectors.rust import RustDetector

pytestmark = pytest.mark.rust


_UDEPS_POSITIVE_JSON = json.dumps(
    {
        "success": False,
        "unused_deps": {
            "my-crate 0.1.0": {
                "manifest_path": "/abs/Cargo.toml",
                "normal": ["unused-crate"],
                "development": [],
                "build": [],
            }
        },
    }
)

_UDEPS_CLEAN_JSON = json.dumps({"success": True, "unused_deps": {}})


def _mock_run(stdout: str, returncode: int = 0, stderr: str = ""):
    class _R:
        def __init__(self) -> None:
            self.stdout = stdout
            self.stderr = stderr
            self.returncode = returncode

    return _R()


def test_rust_detector_flags_unused_dep(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", return_value=_mock_run(_UDEPS_POSITIVE_JSON, 1)):
        findings = det.detect_dead_code(tmp_path)
    dead = [f for f in findings if f.detector == "d1_dead_code"]
    assert any("unused-crate" in f.symbol_or_line for f in dead)


def test_rust_detector_no_findings_on_clean(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", return_value=_mock_run(_UDEPS_CLEAN_JSON, 0)):
        findings = det.detect_dead_code(tmp_path)
    dead = [f for f in findings if f.detector == "d1_dead_code"]
    assert dead == []


def test_rust_detector_emits_auditor_unavailable_when_nightly_missing(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", side_effect=FileNotFoundError("cargo +nightly missing")):
        findings = det.detect_dead_code(tmp_path)
    assert len(findings) == 1
    assert "auditor_unavailable_cargo-udeps" in findings[0].allowlist_key


def test_rust_detector_handles_malformed_json(tmp_path: Path) -> None:
    det = RustDetector()
    with patch("subprocess.run", return_value=_mock_run("not json at all", 1)):
        findings = det.detect_dead_code(tmp_path)
    assert len(findings) == 1
    assert "auditor_output_malformed_cargo-udeps" in findings[0].allowlist_key


# --------------------------------------------------------------------------
# D2 false-positive guards — ported with the detector, which shipped untested
# --------------------------------------------------------------------------

from scripts.detectors.rust import (  # noqa: E402
    _RUST_BUILTIN_CRATES,
    _has_glob_import,
    _in_scope_names,
    _local_module_names,
    _normalize_use_module,
    _workspace_crate_names,
)


class TestLocalModuleNames:
    """`mod foo;` in this file means `use foo::X` resolves in-crate, not on crates.io."""

    def test_finds_plain_bare_and_pub_and_scoped_declarations(self) -> None:
        src = (
            "mod plain;\n"
            "pub mod exported;\n"
            "pub(crate) mod scoped;\n"
            "mod inline { fn f() {} }\n"
        )
        assert _local_module_names(src) == {"plain", "exported", "scoped", "inline"}

    def test_ignores_the_word_mod_inside_an_identifier(self) -> None:
        assert _local_module_names("let module_count = 1;\nfn modify() {}\n") == set()

    def test_empty_source_declares_nothing(self) -> None:
        assert _local_module_names("") == set()


class TestBuiltinCrates:
    """The 117/117 false positives that motivated the fix were mostly `std`."""

    def test_toolchain_crates_are_listed(self) -> None:
        for crate in ("std", "core", "alloc", "proc_macro", "test"):
            assert crate in _RUST_BUILTIN_CRATES

    def test_a_real_published_crate_is_not_listed(self) -> None:
        assert "serde" not in _RUST_BUILTIN_CRATES
        assert "pgrx" not in _RUST_BUILTIN_CRATES


class TestInScopeNames:
    """`use pgrx::pg_sys;` makes a later `use pg_sys::X` a re-scoped path, not a crate."""

    def test_collects_the_last_segment_of_each_use(self) -> None:
        assert _in_scope_names("use pgrx::pg_sys;\nuse std::collections::HashMap;\n") == {
            "pg_sys",
            "HashMap",
        }

    def test_strips_an_alias(self) -> None:
        assert "XactEvent" in _in_scope_names("use pg_sys::XactEvent as XE;\n")

    def test_a_brace_group_yields_the_crate_segment_not_the_braced_names(self) -> None:
        """Known imprecision, pinned so a future change is a decision and not an accident.

        The capture stops at `{`, so `use foo::{A, B};` yields `foo` — the crate — rather
        than `A` and `B`, the names actually brought into scope. Consequence: a later
        `use foo::X` is skipped as already-in-scope and `foo` is never checked against
        crates.io. That under-reports, which is the safe direction for this detector: it
        declines to claim fabrication it did not establish.
        """
        assert _in_scope_names("use foo::{A, B};\n") == {"foo"}


class TestHasGlobImport:
    """A glob brings in names no static scan can enumerate — severity must soften."""

    def test_detects_a_glob(self) -> None:
        assert _has_glob_import("use pgrx::prelude::*;\n") is True

    def test_detects_a_pub_glob(self) -> None:
        assert _has_glob_import("pub use crate::api::*;\n") is True

    def test_plain_import_is_not_a_glob(self) -> None:
        assert _has_glob_import("use pgrx::prelude::PgRelation;\n") is False


class TestNormalizeUseModule:
    def test_strips_each_visibility_prefix(self) -> None:
        assert _normalize_use_module("pub use cache::X") == "cache::X"
        assert _normalize_use_module("pub(crate) use cache::X") == "cache::X"
        assert _normalize_use_module("pub(super) use cache::X") == "cache::X"
        assert _normalize_use_module("use cache::X") == "cache::X"

    def test_leaves_a_bare_path_untouched(self) -> None:
        assert _normalize_use_module("serde::Deserialize") == "serde::Deserialize"


class TestWorkspaceCrateNames:
    """A path/workspace dependency is ours; crates.io answers 'not found' for it."""

    def test_collects_member_names_and_normalises_hyphens(self, tmp_path) -> None:
        (tmp_path / "Cargo.toml").write_text('[workspace]\nmembers = ["a", "b"]\n')
        for name in ("a", "b"):
            member = tmp_path / name
            member.mkdir()
            (member / "Cargo.toml").write_text(f'[package]\nname = "theo-{name}"\n')

        names = _workspace_crate_names(tmp_path / "a")

        assert {"theo-a", "theo_a", "theo-b", "theo_b"} <= names

    def test_skips_the_target_directory(self, tmp_path) -> None:
        (tmp_path / "Cargo.toml").write_text('[workspace]\n[package]\nname = "root"\n')
        vendored = tmp_path / "target" / "debug" / "vendored"
        vendored.mkdir(parents=True)
        (vendored / "Cargo.toml").write_text('[package]\nname = "not-ours"\n')

        assert "not-ours" not in _workspace_crate_names(tmp_path)

    def test_no_manifest_anywhere_yields_nothing(self, tmp_path) -> None:
        assert _workspace_crate_names(tmp_path) == set()


# ----------------------------------------------------------------------------------------
# B-039 — o auditor roda onde o ambiente de build existe
#
# Medido no B-035 e reconfirmado em quatro ciclos: no host o erro real NÃO é permissão, é
# `/home/paulo/.pgrx/config.toml not found. Have you run 'cargo pgrx init' yet?` — o host
# nunca instalou o pgrx, e nenhum `chown` conserta isso. Dentro do `theodb-toolchain`, que
# tem `cargo pgrx init` feito, o mesmo audit fecha em 2 min 07 s com
# `All deps seem to have been used.`
#
# Quatro ciclos declararam `auditor_unavailable_cargo-udeps` como "limitação de ambiente",
# que é a forma educada de dizer que ninguém investigou. Um cap que dispara sempre deixa de
# ser sinal.
# ----------------------------------------------------------------------------------------


def test_udeps_falls_back_to_the_pinned_container_when_the_host_lacks_pgrx(tmp_path: Path) -> None:
    """O host sem pgrx não é 'auditor indisponível' — é 'auditor no lugar errado'."""
    calls: list[list[str]] = []

    def _fake_run(cmd, **kwargs):
        calls.append(cmd)
        if "docker" not in cmd[0]:
            return _mock_run("", returncode=101, stderr="Error: /root/.pgrx/config.toml not found.")
        return _mock_run(_UDEPS_CLEAN_JSON, returncode=0)

    det = RustDetector()
    with patch("subprocess.run", side_effect=_fake_run):
        findings = det.detect_dead_code(tmp_path)

    assert findings == [], f"o audit no contêiner passou limpo; não deveria haver achado: {findings}"
    assert any("docker" in c[0] for c in calls), "o fallback para o contêiner nunca foi tentado"


def test_udeps_reports_the_real_reason_when_docker_is_absent(tmp_path: Path) -> None:
    """Falhar sem docker é legítimo — dizer 'cargo-udeps não encontrado' não é.

    A mensagem genérica é o que fez quatro ciclos lerem o cap como ausência da ferramenta,
    quando a ferramenta estava instalada e o ambiente é que faltava.
    """
    def _fake_run(cmd, **kwargs):
        if "docker" in cmd[0]:
            raise FileNotFoundError("docker")
        return _mock_run("", returncode=101, stderr="Error: /root/.pgrx/config.toml not found.")

    det = RustDetector()
    with patch("subprocess.run", side_effect=_fake_run):
        findings = det.detect_dead_code(tmp_path)

    assert len(findings) == 1
    msg = findings[0].message.lower()
    assert "docker" in msg, f"a razão real tem de nomear o docker: {findings[0].message}"


def test_udeps_reports_the_image_name_when_the_pinned_image_is_missing(tmp_path: Path) -> None:
    def _fake_run(cmd, **kwargs):
        if "docker" in cmd[0]:
            return _mock_run("", returncode=125, stderr="Unable to find image 'theodb-toolchain:latest' locally")
        return _mock_run("", returncode=101, stderr="Error: /root/.pgrx/config.toml not found.")

    det = RustDetector()
    with patch("subprocess.run", side_effect=_fake_run):
        findings = det.detect_dead_code(tmp_path)

    assert len(findings) == 1
    assert "theodb-toolchain" in findings[0].message, findings[0].message


def test_udeps_does_not_reach_for_the_container_when_the_host_works(tmp_path: Path) -> None:
    """O contêiner é fallback, não o caminho padrão: pagá-lo sempre custaria 2 min por audit."""
    calls: list[list[str]] = []

    def _fake_run(cmd, **kwargs):
        calls.append(cmd)
        return _mock_run(_UDEPS_CLEAN_JSON, returncode=0)

    det = RustDetector()
    with patch("subprocess.run", side_effect=_fake_run):
        det.detect_dead_code(tmp_path)

    assert not any("docker" in c[0] for c in calls), "o host respondeu; o contêiner não devia ser tentado"


def test_udeps_falls_back_when_the_host_cannot_write_to_target(tmp_path: Path) -> None:
    """O B-039 registrou DOIS obstáculos empilhados, e o primeiro mascara o segundo.

    Medido em 2026-08-13 neste repositório: o host falha com
    `error: failed to write .../target/debug/.fingerprint/...` — resíduo de builds em contêiner que
    montaram o diretório do host — ANTES de chegar ao `config.toml not found`. Um predicado que só
    reconhecesse a assinatura do pgrx ausente deixaria o cap disparando exatamente na máquina onde
    ele foi medido.

    O critério correto não é a assinatura do erro, é a AUSÊNCIA DE DADO: um audit que rodou devolve
    JSON, com ou sem achado. Sem JSON e com exit != 0, o host não auditou — e o contêiner é onde ele
    consegue.
    """
    def _fake_run(cmd, **kwargs):
        if "docker" in cmd[0]:
            return _mock_run(_UDEPS_CLEAN_JSON, returncode=0)
        return _mock_run(
            "", returncode=101,
            stderr="error: failed to write `/repo/theodb_rs/target/debug/.fingerprint/zstd/lib-zstd`",
        )

    det = RustDetector()
    with patch("subprocess.run", side_effect=_fake_run):
        findings = det.detect_dead_code(tmp_path)

    assert findings == [], f"o contêiner auditou limpo; não deveria restar achado: {findings}"


def test_udeps_does_not_fall_back_when_the_host_actually_audited(tmp_path: Path) -> None:
    """Achado não é falha de ambiente. Cair para o contêiner aqui repetiria 2 min de trabalho
    para chegar exatamente ao mesmo achado — e o custo do fallback só se justifica quando o host
    não produziu dado nenhum."""
    calls: list[list[str]] = []

    def _fake_run(cmd, **kwargs):
        calls.append(cmd)
        return _mock_run(_UDEPS_POSITIVE_JSON, returncode=1)

    det = RustDetector()
    with patch("subprocess.run", side_effect=_fake_run):
        findings = det.detect_dead_code(tmp_path)

    assert findings, "o achado do host tem de sobreviver"
    assert not any("docker" in c[0] for c in calls), "o host auditou; o contêiner não devia ser tentado"


def test_container_run_does_not_share_the_hosts_target_directory(tmp_path: Path) -> None:
    """Host e contêiner não podem compilar no MESMO `target/`.

    Medido em 2026-08-13: a primeira execução do audit após o fallback reportou
    `exit 101: Updating crates.io index`; a segunda, com o `target/` já aquecido por uma corrida
    direta, passou limpo. Dois `cargo` disputando o mesmo diretório é estado compartilhado mutável
    entre dois processos — a mesma classe que o B-027 eliminou dando nome único ao contêiner, em vez
    de remediar a colisão.

    Um conserto que só funciona com o cache quente é um conserto que falha na máquina de quem chega
    depois, e o modo de falha dele é indistinguível de "o auditor não está disponível" — que é
    exatamente o cap que este item existe para remover.
    """
    def _fake_run(cmd, **kwargs):
        if "docker" in cmd[0]:
            assert any("CARGO_TARGET_DIR" in str(a) for a in cmd), (
                f"o contêiner tem de compilar num target próprio: {cmd}"
            )
            return _mock_run(_UDEPS_CLEAN_JSON, returncode=0)
        return _mock_run("", returncode=101, stderr="error: failed to write .../target/.fingerprint/x")

    det = RustDetector()
    with patch("subprocess.run", side_effect=_fake_run):
        det.detect_dead_code(tmp_path)
