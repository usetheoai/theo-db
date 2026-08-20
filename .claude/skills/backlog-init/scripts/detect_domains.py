#!/usr/bin/env python3
"""Deriva a tabela de roteamento DO PROJETO, em vez de herdá-la de outro.

O PROBLEMA
----------
`rules/cycle-backlog.md § Domain routing` embarcava os 8 domínios do ecossistema
`theo`. Toda instalação levava essa tabela junto, e o `backlog-init` mandava
encaixar os repos do alvo *dentro* dos 8, com a instrução explícita de não
"inventar um nono domínio". Num projeto que não é o `theo`, nenhum repo encaixa.

Medido no `theokit-sdk` em 2026-08-18: 88 itens de backlog com evidência
`file:line` medida, todos reprovados como `BLOCKER/unroutable_repo` — 68 citando
`packages/sdk`, 14 `theokit-sdk`, e mais quatro pacotes. O gate estava certo no
que dizia (*"não sei para quem mandar isto"*); errada estava a tabela, que era
dado de um projeto morando dentro do template de todos.

A REGRA DE DERIVAÇÃO
--------------------
A unidade de propriedade é o repositório, então:

- **Guarda-chuva** (subdiretórios com `.git` próprio): um domínio por repo. É o
  formato do ecossistema `theo`, onde cada repo tem dono distinto.
- **Repo único**: UM domínio, com o nome do repositório. Se ele for um monorepo,
  cada pacote entra como um `repo` endereçado por caminho (`packages/sdk`) — a
  forma que o kit já suporta e documenta (`theo-cloud/dashboard`). Um domínio por
  pacote criaria seis especialistas onde existe um SDK.

O que NÃO é derivado: quem é o especialista. O arquivo `agents/<domínio>.md` é
nomeado aqui, mas escrevê-lo é trabalho humano — um especialista sem conteúdo
rotearia o item para um prompt vazio, e `route_domain.py` sai 3 quando o arquivo
não existe, de propósito.

DUAS FONTES, E A SEGUNDA MANDA
------------------------------
`--from-backlog` deriva dos pares (domain, repo) que os itens JÁ declaram. Use-a
sempre que o registro existir: a topologia diz o que existe, não quem é dono.
Medido no `theokit-sdk` — o registro separa `sdk-core`, `repo-platform`,
`sdk-satellites`, `edge-cli-acp` e `memory-adapters`, cinco domínios que nenhum
layout de diretório revela e que nenhum detector deveria adivinhar.

Uso:
    python3 detect_domains.py                       # imprime a tabela proposta
    python3 detect_domains.py --from-backlog BACKLOG.md
    python3 detect_domains.py --write rules/cycle-backlog.md
    python3 detect_domains.py --json

Exit codes:
    0 — tabela derivada (e escrita, se --write)
    1 — nenhum domínio derivável (diretório sem repo e sem manifesto)
    2 — erro de escrita (arquivo sem a seção `## Domain routing`)
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

#: Diretórios que nunca são unidade arquitetural, em qualquer ecossistema.
_IGNORED_DIRS = {
    "node_modules", "vendor", "dist", "build", "target", "coverage",
    ".git", ".claude", "__pycache__", ".venv", "venv", ".tox", "testdata",
}

#: Onde um monorepo guarda seus pacotes, por convenção de cada ecossistema.
_WORKSPACE_PARENTS = ("packages", "apps", "services", "crates", "libs", "modules")

#: Manifesto que prova que um subdiretório é uma unidade publicável.
_PACKAGE_MANIFESTS = ("package.json", "pyproject.toml", "Cargo.toml", "go.mod", "composer.json")

_GO_USE_BLOCK_RE = re.compile(r"^use\s*\((.*?)^\)", re.MULTILINE | re.DOTALL)
_GO_USE_SINGLE_RE = re.compile(r"^use\s+(\S+)\s*$", re.MULTILINE)
_ROUTING_SECTION_RE = re.compile(
    r"^##\s+Domain routing\b.*?(?=^##\s|\Z)", re.MULTILINE | re.DOTALL
)


@dataclass(frozen=True)
class Domain:
    name: str
    repos: list[str]
    agent: str
    #: Repos que o registro cita e o disco não tem. Ficam NA tabela, nomeados —
    #: apagá-los esconderia a divergência, e um item filed contra eles routes
    #: para código que ninguém abre. Mesma decisão da seção "Repos an inventory
    #: names but disk does not" que o `theo` mantém à mão.
    missing_on_disk: list[str] = None  # type: ignore[assignment]

    def __post_init__(self) -> None:
        if self.missing_on_disk is None:
            object.__setattr__(self, "missing_on_disk", [])


def _is_repo(path: Path) -> bool:
    return (path / ".git").exists()


def _child_repos(root: Path) -> list[Path]:
    return sorted(
        (p for p in root.iterdir()
         if p.is_dir() and p.name not in _IGNORED_DIRS and not p.name.startswith(".")
         and _is_repo(p)),
        key=lambda p: p.name,
    )


def detect_scope(root: Path) -> str:
    """`umbrella` quando há mais de um repositório governado abaixo; senão `single-repo`.

    O `backlog-init` recusava rodar sem guarda-chuva — *"no umbrella detected: run
    at the workspace root"* — o que, num projeto autônomo, manda criar o
    `BACKLOG.md` na raiz do guarda-chuva, **fora do projeto**. Medido no
    `theokit-framework`: dez repos independentes, cada um com seu ciclo, e o kit
    empurrava o registro dos dez para um diretório que não é repositório de nada.

    O princípio que a regra defende ("uma pergunta, um lugar para olhar") não
    exige guarda-chuva: exige **um registro por escopo governado**. Um repo
    autônomo é um escopo.
    """
    root = root.resolve()
    # A raiz SER um repositório é o que decide: um projeto com um clone vendorizado
    # abaixo continua sendo um projeto. Guarda-chuva é o diretório que não é
    # repositório de nada e existe para agrupar os que são.
    if _is_repo(root):
        return "single-repo"
    return "umbrella" if _child_repos(root) else "single-repo"


def _go_workspace_members(root: Path) -> list[str]:
    work = root / "go.work"
    if not work.is_file():
        return []
    text = work.read_text(encoding="utf-8")
    entries: list[str] = []
    for block in _GO_USE_BLOCK_RE.findall(text):
        entries.extend(line.strip() for line in block.splitlines() if line.strip())
    entries.extend(_GO_USE_SINGLE_RE.findall(text))

    members: list[str] = []
    for entry in entries:
        entry = entry.strip().strip('"')
        # Um caminho que sai do repositório pertence a outro repositório, com
        # gates próprios — o `go.work` do `theo` lista `../theo-contracts`.
        if not entry or entry.startswith(".."):
            continue
        rel = entry[2:] if entry.startswith("./") else entry
        if rel and (root / rel).is_dir() and rel not in members:
            members.append(rel)
    return members


def _workspace_packages(root: Path) -> list[str]:
    """Pacotes de um monorepo, endereçados pelo caminho relativo à raiz."""
    found: list[str] = []
    for parent_name in _WORKSPACE_PARENTS:
        parent = root / parent_name
        if not parent.is_dir():
            continue
        for child in sorted(parent.iterdir(), key=lambda p: p.name):
            if not child.is_dir() or child.name in _IGNORED_DIRS or child.name.startswith("."):
                continue
            if any((child / manifest).is_file() for manifest in _PACKAGE_MANIFESTS):
                found.append(f"{parent_name}/{child.name}")
    return found + [m for m in _go_workspace_members(root) if m not in found]


def detect_domains(root: Path) -> list[Domain]:
    """Deriva os domínios da topologia real do projeto."""
    root = root.resolve()
    children = _child_repos(root)

    if children:
        # Guarda-chuva: a unidade de propriedade é o repositório.
        return [
            Domain(name=repo.name, repos=[repo.name], agent=f"agents/{repo.name}.md")
            for repo in children
        ]

    name = root.name
    return [Domain(name=name, repos=[name, *_workspace_packages(root)],
                   agent=f"agents/{name}.md")]


_ITEM_BLOCK_RE = re.compile(r"^##\s+(B-\d+)\s+—", re.MULTILINE)
_FIELD_RE = re.compile(r"^(domain|repo):\s*`?([^`\n]+?)`?\s*$", re.MULTILINE)


def domains_from_backlog(backlog_path: Path, root: Path) -> list[Domain]:
    """Deriva a tabela dos pares (domain, repo) que os itens JÁ declaram.

    A topologia diz o que existe; ela não diz a semântica de propriedade. Medido
    no `theokit-sdk`: o registro separa `sdk-core`, `repo-platform`,
    `sdk-satellites`, `edge-cli-acp` e `memory-adapters` — cinco domínios que
    nenhum layout de diretório revela e que nenhum detector deveria adivinhar.
    Os itens já carregam a resposta; isto apenas a lê.

    Levanta ValueError quando um repo aparece em dois domínios: `route_domain`
    exige um-repo-um-domínio, e uma tabela ambígua rotearia por ordem de
    iteração — o mesmo item indo para lugares diferentes em execuções diferentes.
    """
    content = backlog_path.read_text(encoding="utf-8-sig")
    blocks = list(_ITEM_BLOCK_RE.finditer(content))

    by_domain: dict[str, list[str]] = {}
    owner_of: dict[str, str] = {}
    for i, match in enumerate(blocks):
        start = match.end()
        end = blocks[i + 1].start() if i + 1 < len(blocks) else len(content)
        fields = dict(_FIELD_RE.findall(content[start:end]))
        domain, repo = fields.get("domain"), fields.get("repo")
        if not domain or not repo:
            continue
        if repo in owner_of and owner_of[repo] != domain:
            raise ValueError(
                f"`{repo}` is declared in two domains ({owner_of[repo]} and {domain}). "
                "route_domain requires one repo, one domain — fix the items before deriving."
            )
        owner_of[repo] = domain
        by_domain.setdefault(domain, [])
        if repo not in by_domain[domain]:
            by_domain[domain].append(repo)

    domains: list[Domain] = []
    for name in sorted(by_domain):
        repos = sorted(by_domain[name])
        missing = [r for r in repos if not (root / r).exists() and r != root.name]
        domains.append(Domain(name=name, repos=repos, agent=f"agents/{name}.md",
                              missing_on_disk=missing))
    return domains


def render_table(domains: list[Domain]) -> str:
    lines = [
        "## Domain routing",
        "",
        "`domain` is what assigns the item to a specialist. This table is **derived from this",
        "project** by `skills/backlog-init/scripts/detect_domains.py` — never copied from another",
        "ecosystem's inventory. Re-run it when a repo or package is added; edit it by hand when",
        "ownership does not follow the directory layout.",
        "",
        "| Domain | Repos (present on disk) | Specialist |",
        "|---|---|---|",
    ]
    for domain in domains:
        repos = ", ".join(f"`{r}`" for r in domain.repos)
        lines.append(f"| `{domain.name}` | {repos} | `{domain.agent}` |")
    lines.append("")
    return "\n".join(lines)


def rewrite_routing_section(rule_path: Path, domains: list[Domain]) -> None:
    """Substitui a seção `## Domain routing` preservando o resto do arquivo."""
    content = rule_path.read_text(encoding="utf-8-sig")
    if not _ROUTING_SECTION_RE.search(content):
        raise ValueError(f"{rule_path}: sem a seção '## Domain routing' para substituir")
    rule_path.write_text(
        _ROUTING_SECTION_RE.sub(lambda _: render_table(domains) + "\n", content, count=1),
        encoding="utf-8",
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--from-backlog", type=Path, default=None,
                        help="deriva dos pares (domain, repo) que os itens já declaram — "
                             "use quando o registro existe: a semântica de propriedade está "
                             "lá, e nenhum layout de diretório a revela")
    parser.add_argument("--write", type=Path, default=None,
                        help="caminho de rules/cycle-backlog.md a atualizar")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    try:
        domains = (domains_from_backlog(args.from_backlog, args.root.resolve())
                   if args.from_backlog else detect_domains(args.root))
    except ValueError as exc:
        print(f"FATAL: {exc}", file=sys.stderr)
        return 1
    except OSError as exc:
        print(f"FATAL: {exc}", file=sys.stderr)
        return 2
    if not domains:
        print("nenhum domínio derivável — o diretório não é repo nem tem manifesto",
              file=sys.stderr)
        return 1

    missing = [d.agent for d in domains if not (args.root / ".claude" / d.agent).is_file()
               and not (args.root / d.agent).is_file()]

    if args.json:
        print(json.dumps({
            "domains": [{"name": d.name, "repos": d.repos, "agent": d.agent} for d in domains],
            "scope": detect_scope(args.root),
            "specialists_missing": missing,
            "repos_missing_on_disk": sorted(
                {r for d in domains for r in d.missing_on_disk}),
        }, indent=2, ensure_ascii=False))
    else:
        print(render_table(domains))
        absent = sorted({r for d in domains for r in d.missing_on_disk})
        if absent:
            print("Repos que o registro cita e o disco não tem "
                  "(ficam na tabela, nomeados, para a divergência não sumir):")
            for repo in absent:
                print(f"  - {repo}")
        if missing:
            print("Especialistas que precisam ser escritos (route_domain sai 3 sem eles):")
            for agent in missing:
                print(f"  - {agent}")

    if args.write:
        try:
            rewrite_routing_section(args.write, domains)
        except (ValueError, OSError) as exc:
            print(f"FATAL: {exc}", file=sys.stderr)
            return 2
        print(f"\n==> `## Domain routing` reescrita em {args.write}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
