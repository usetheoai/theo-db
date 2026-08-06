"""D2/TypeScript — dois defeitos medidos no theo-promptly em 2026-08-03.

Ambos fazem o detector chamar de FABRICADO aquilo que resolve perfeitamente, e juntos
produziram 112 achados (60 HARD) num repositório cujo build e cujos testes estão verdes.
Um detector que reprova um monorepo saudável ensina o time a ignorá-lo — é por isso que
estes casos existem.
"""
from __future__ import annotations

import json
from pathlib import Path

from scripts.detectors.typescript import TypescriptDetector


def _workspace(tmp_path: Path) -> Path:
    """Monorepo pnpm mínimo: raiz `theo-promptly`, dois membros, um importando o outro."""
    (tmp_path / "package.json").write_text(
        json.dumps({"name": "theo-promptly", "private": True}), encoding="utf-8"
    )
    (tmp_path / "pnpm-workspace.yaml").write_text("packages:\n- packages/*\n", encoding="utf-8")
    core = tmp_path / "packages" / "core"
    api = tmp_path / "packages" / "api"
    for d in (core, api):
        (d / "src").mkdir(parents=True)
    (core / "package.json").write_text(
        json.dumps({"name": "@usetheo/promptly"}), encoding="utf-8"
    )
    (api / "package.json").write_text(
        json.dumps({"name": "@usetheo/promptly-api",
                    "dependencies": {"@usetheo/promptly": "workspace:*"}}),
        encoding="utf-8",
    )
    return api


def test_sibling_workspace_import_is_not_reported_as_fabricated(tmp_path, monkeypatch):
    """Importar um IRMÃO do workspace não é fabricação.

    O patch de auto-referência (2026-05-30) resolve só o nome do package.json RAIZ
    (`theo-promptly`) — que ninguém importa. Todo import irmão ia ao registry, tomava 404 e
    virava HARD `symbol_fabrication_typescript`.
    """
    api = _workspace(tmp_path)
    src = api / "src" / "app.ts"
    src.write_text("import { createPromptVersion } from '@usetheo/promptly';\n", encoding="utf-8")

    # O registry NUNCA deve ser consultado para um pacote local — se for, é o bug.
    from scripts import _registry
    monkeypatch.setattr(_registry, "package_exists_on_npm",
                        lambda pkg: (_ for _ in ()).throw(
                            AssertionError(f"consultou o registry para pacote local: {pkg}")))

    findings = TypescriptDetector().detect_symbol_fabrication([src])
    assert findings == [], f"irmão do workspace reportado como fabricado: {findings}"


def test_scoped_subpath_import_queries_the_package_not_the_subpath(tmp_path, monkeypatch):
    """`@scope/pkg/sub/path.js` deve ser consultado como `@scope/pkg`.

    O detector calculava `top` corretamente e depois o ignorava para pacotes escopados,
    mandando o módulo INTEIRO ao registry. `@modelcontextprotocol/sdk/server/mcp.js` não é
    nome de pacote — daí 52 achados 'ambiguous response' sobre um SDK real e instalado.
    """
    api = _workspace(tmp_path)
    src = api / "src" / "mcp.ts"
    src.write_text(
        "import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';\n", encoding="utf-8"
    )

    consultados: list[str] = []
    from scripts import _registry
    monkeypatch.setattr(_registry, "package_exists_on_npm",
                        lambda pkg: (consultados.append(pkg), True)[1])

    findings = TypescriptDetector().detect_symbol_fabrication([src])
    assert consultados == ["@modelcontextprotocol/sdk"], (
        f"consultou o subpath em vez do pacote: {consultados}"
    )
    assert findings == []


class TestPathAliasNotAPackage:
    """Terceira familia de falso positivo: `@/components/...` e alias de tsconfig.

    986 achados HARD num dashboard, todos falsos, porque o detector tratava
    qualquer especificador com `@` como escopo npm e ia ao registry. A raiz das
    tres familias e a mesma: resolver nome de modulo contra o registry publico
    sem consultar o que o projeto declara.
    """

    def test_alias_declarado_no_tsconfig_nao_vai_ao_registry(self, tmp_path, monkeypatch):
        from scripts.detectors.typescript import TypescriptDetector

        (tmp_path / ".git").mkdir()
        (tmp_path / "tsconfig.json").write_text(
            '{"compilerOptions":{"paths":{"@/*":["./src/*"],"~lib/*":["./lib/*"]}}}',
            encoding="utf-8",
        )
        src = tmp_path / "src" / "page.tsx"
        src.parent.mkdir(parents=True)
        src.write_text("import {Button} from '@/components/Button';\n", encoding="utf-8")

        det = TypescriptDetector()
        aliases = det._find_path_aliases([src])

        assert "@" in aliases and "~lib" in aliases
        assert det._is_path_alias("@/components/Button", aliases) is True
        assert det._is_path_alias("~lib/util", aliases) is True

    def test_pacote_escopado_de_verdade_nao_e_confundido_com_alias(self, tmp_path):
        from scripts.detectors.typescript import TypescriptDetector

        (tmp_path / ".git").mkdir()
        (tmp_path / "tsconfig.json").write_text(
            '{"compilerOptions":{"paths":{"@/*":["./src/*"]}}}', encoding="utf-8"
        )
        src = tmp_path / "a.ts"
        src.write_text("import x from '@scope/pkg';\n", encoding="utf-8")

        det = TypescriptDetector()
        aliases = det._find_path_aliases([src])

        assert det._is_path_alias("@scope/pkg", aliases) is False

    def test_tsconfig_com_comentarios_e_virgula_final_e_lido(self, tmp_path):
        """tsconfig admite comentarios; JSON estrito falharia e o alias sumiria."""
        from scripts.detectors.typescript import TypescriptDetector

        (tmp_path / ".git").mkdir()
        (tmp_path / "tsconfig.json").write_text(
            '{\n  // caminhos\n  "compilerOptions": {\n    "paths": {"@/*": ["./src/*"],}\n  }\n}',
            encoding="utf-8",
        )
        src = tmp_path / "a.ts"
        src.write_text("import x from '@/y';\n", encoding="utf-8")

        det = TypescriptDetector()

        assert det._is_path_alias("@/y", det._find_path_aliases([src])) is True
