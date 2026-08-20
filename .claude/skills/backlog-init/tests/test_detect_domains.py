"""A tabela de roteamento é dado do projeto, e vivia dentro do template.

`rules/cycle-backlog.md § Domain routing` embarca os 8 domínios do ecossistema
`theo` (`engine-go`, `control-plane`, `theo-db`, …). Toda instalação copia essa
tabela, e o `backlog-init` mandava classificar os repos do alvo *dentro* desses
8, proibindo "inventar um nono domínio". O resultado, medido no `theokit-sdk`:
88 itens com evidência `file:line` medida, todos `BLOCKER/unroutable_repo`,
porque `packages/sdk` e `theokit-sdk` não existem no mapa de outro ecossistema.

O gate estava certo em recusar — ele não sabia para quem mandar o trabalho. O
que estava errado era a tabela vir pronta de fora.
"""
from __future__ import annotations

from pathlib import Path

from detect_domains import detect_domains, render_table, rewrite_routing_section


def _repo(root: Path, name: str, *, git: bool = True) -> Path:
    path = root / name
    path.mkdir(parents=True, exist_ok=True)
    if git:
        (path / ".git").mkdir(exist_ok=True)
    return path


def test_single_repo_becomes_one_domain_named_after_it(tmp_path: Path) -> None:
    root = _repo(tmp_path, "theokit-sdk")
    domains = detect_domains(root)
    assert [d.name for d in domains] == ["theokit-sdk"]
    assert domains[0].repos == ["theokit-sdk"]
    assert domains[0].agent == "agents/theokit-sdk.md"


def test_npm_monorepo_lists_each_package_by_path(tmp_path: Path) -> None:
    """O caso do theokit-sdk: um repo, vários pacotes, itens citando `packages/x`.

    Um domínio só — existe um SDK, não seis times. Os pacotes entram como repos
    endereçados por caminho, forma que o kit já suporta (`theo-cloud/dashboard`).
    """
    root = _repo(tmp_path, "theokit-sdk")
    for pkg in ("sdk", "acp", "sdk-pty"):
        (root / "packages" / pkg).mkdir(parents=True)
        (root / "packages" / pkg / "package.json").write_text("{}", encoding="utf-8")
    (root / "node_modules" / "lodash").mkdir(parents=True)
    (root / "node_modules" / "lodash" / "package.json").write_text("{}", encoding="utf-8")

    domains = detect_domains(root)
    assert len(domains) == 1
    assert domains[0].repos == ["theokit-sdk", "packages/acp", "packages/sdk", "packages/sdk-pty"]


def test_go_workspace_modules_become_repos(tmp_path: Path) -> None:
    root = _repo(tmp_path, "theo")
    (root / "go.work").write_text("go 1.22\n\nuse (\n\t./api\n\t./operators\n\t../sibling\n)\n",
                                  encoding="utf-8")
    (root / "api").mkdir()
    (root / "operators").mkdir()
    domains = detect_domains(root)
    assert domains[0].repos == ["theo", "api", "operators"]  # o irmão fora do repo não entra


def test_umbrella_gives_one_domain_per_checked_out_repo(tmp_path: Path) -> None:
    """Workspace guarda-chuva: a unidade de propriedade é o repositório."""
    root = tmp_path / "umbrella"
    root.mkdir()
    _repo(root, "theo-lens")
    _repo(root, "theo-db")
    (root / "docs").mkdir()  # sem .git — não é repo, não vira domínio
    domains = detect_domains(root)
    assert [d.name for d in domains] == ["theo-db", "theo-lens"]
    assert all(d.repos == [d.name] for d in domains)


def test_rendered_table_is_parseable_by_route_domain(tmp_path: Path) -> None:
    """O contrato real: o que sai daqui tem que entrar no parser do route_domain."""
    import sys
    root = _repo(tmp_path, "theokit-sdk")
    (root / "packages" / "sdk").mkdir(parents=True)
    (root / "packages" / "sdk" / "package.json").write_text("{}", encoding="utf-8")

    rule = tmp_path / "cycle-backlog.md"
    rule.write_text(
        "# Cycle: BACKLOG\n\n## Domain routing\n\n| Domain | Repos | Specialist |\n"
        "|---|---|---|\n| `velho` | `outro-eco` | `agents/velho.md` |\n\n"
        "## Verdicts\n\nintocado\n",
        encoding="utf-8",
    )
    rewrite_routing_section(rule, detect_domains(root))

    sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "scripts"))
    from route_domain import parse_routing_table, route

    table = parse_routing_table(rule)
    assert "velho" not in table, "a tabela do outro ecossistema tem de sair"
    assert route("packages/sdk", table) == ("theokit-sdk", "agents/theokit-sdk.md")
    assert route("theokit-sdk", table) == ("theokit-sdk", "agents/theokit-sdk.md")
    assert "## Verdicts" in rule.read_text(encoding="utf-8"), "o resto do arquivo sobrevive"


def test_render_names_the_specialist_files_that_must_exist(tmp_path: Path) -> None:
    """route_domain sai 3 quando a tabela nomeia um agente que não está em disco —
    trocar 88 blockers por esse erro não seria conserto."""
    root = _repo(tmp_path, "theokit-sdk")
    table = render_table(detect_domains(root))
    assert "agents/theokit-sdk.md" in table


# ---------------------------------------------------------------------------
# Derivar do BACKLOG. A topologia dá o que EXISTE; ela não dá a SEMÂNTICA de
# propriedade. Medido no theokit-sdk: o registro declara `sdk-core`,
# `repo-platform`, `sdk-satellites`, `edge-cli-acp` e `memory-adapters` — cinco
# domínios que nenhum layout de diretório revela, e que os itens já carregam.
# ---------------------------------------------------------------------------

from detect_domains import domains_from_backlog  # noqa: E402

_BACKLOG = """# Backlog

## B-001 — um   [ ]

domain: sdk-core
repo: packages/sdk
status: triaged

## B-002 — dois   [ ]

domain: repo-platform
repo: theokit-sdk
status: triaged

## B-003 — tres   [ ]

domain: sdk-satellites
repo: packages/sdk-pty
status: raw

## B-004 — quatro   [ ]

domain: sdk-core
repo: packages/sdk
status: raw
"""


def test_domains_come_from_the_pairs_the_items_declare(tmp_path: Path) -> None:
    backlog = tmp_path / "BACKLOG.md"
    backlog.write_text(_BACKLOG, encoding="utf-8")
    root = _repo(tmp_path, "theokit-sdk")
    (root / "packages" / "sdk").mkdir(parents=True)
    (root / "packages" / "sdk-pty").mkdir(parents=True)

    domains = domains_from_backlog(backlog, root)
    assert [d.name for d in domains] == ["repo-platform", "sdk-core", "sdk-satellites"]
    assert next(d for d in domains if d.name == "sdk-core").repos == ["packages/sdk"]
    assert next(d for d in domains if d.name == "sdk-core").agent == "agents/sdk-core.md"


def test_a_repo_the_items_cite_but_disk_does_not_have_is_surfaced(tmp_path: Path) -> None:
    """Um repo que só existe no registro roteia para código que ninguém abre —
    a mesma divergência que a tabela do theo documenta em vez de apagar."""
    backlog = tmp_path / "BACKLOG.md"
    backlog.write_text(_BACKLOG, encoding="utf-8")
    root = _repo(tmp_path, "theokit-sdk")
    (root / "packages" / "sdk").mkdir(parents=True)  # sdk-pty NÃO existe

    domains = domains_from_backlog(backlog, root)
    satellites = next(d for d in domains if d.name == "sdk-satellites")
    assert satellites.repos == ["packages/sdk-pty"]
    assert satellites.missing_on_disk == ["packages/sdk-pty"]


def test_one_repo_in_two_domains_is_refused(tmp_path: Path) -> None:
    """O invariante que route_domain já exige: um repo, um domínio. Se o registro
    contradiz isso, a tabela derivada rotearia por ordem de iteração."""
    backlog = tmp_path / "BACKLOG.md"
    backlog.write_text(_BACKLOG + """
## B-005 — cinco   [ ]

domain: edge-cli-acp
repo: packages/sdk
status: raw
""", encoding="utf-8")
    root = _repo(tmp_path, "theokit-sdk")
    (root / "packages" / "sdk").mkdir(parents=True)
    (root / "packages" / "sdk-pty").mkdir(parents=True)

    import pytest
    with pytest.raises(ValueError, match="packages/sdk"):
        domains_from_backlog(backlog, root)


# ---------------------------------------------------------------------------
# O escopo do registro. `backlog-init` Step 0.2 recusava rodar quando não havia
# mais de um repo abaixo ("no umbrella detected — run at the workspace root"), o
# que num projeto autônomo manda criar o BACKLOG na raiz do guarda-chuva, FORA
# do projeto. O princípio ("um lugar para olhar") não exige guarda-chuva: exige
# um registro por escopo governado.
# ---------------------------------------------------------------------------

from detect_domains import detect_scope  # noqa: E402


def test_umbrella_scope_when_more_than_one_repo_lives_below(tmp_path: Path) -> None:
    root = tmp_path / "framework"
    root.mkdir()
    _repo(root, "theokit-sdk")
    _repo(root, "theokit-ui")
    assert detect_scope(root) == "umbrella"


def test_single_repo_scope_is_valid_not_an_error(tmp_path: Path) -> None:
    """theokit-sdk: um repo, seu próprio ciclo, seu próprio registro."""
    root = _repo(tmp_path, "theokit-sdk")
    (root / "packages" / "sdk").mkdir(parents=True)
    (root / "packages" / "sdk" / "package.json").write_text("{}", encoding="utf-8")
    assert detect_scope(root) == "single-repo"


def test_a_project_with_a_vendored_clone_is_still_single_repo(tmp_path: Path) -> None:
    """O que decide é a raiz SER um repositório, não a contagem de `.git` abaixo.

    A guarda antiga contava `find -maxdepth 2 -name .git` e exigia `> 1`, então um
    projeto com um clone vendorizado dentro passava por guarda-chuva e o registro
    dele ia para o diretório de cima.
    """
    root = _repo(tmp_path, "projeto")
    _repo(root, "vendored-thing")
    assert detect_scope(root) == "single-repo"


def test_umbrella_is_a_directory_that_is_not_itself_a_repo(tmp_path: Path) -> None:
    framework = tmp_path / "framework"
    framework.mkdir()
    _repo(framework, "um-repo-so")
    assert detect_scope(framework) == "umbrella"
