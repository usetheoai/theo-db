#!/usr/bin/env python3
r"""M137 — gera o salto CONVERGENTE da cadeia de upgrade do `theodb_rs`.

Por que gerar em vez de escrever à mão
--------------------------------------
O SQL de instalação que o pgrx emite tem ~2300 linhas e 196 objetos. Transcrevê-lo à mão para um script de
upgrade seria erro garantido, e o erro dessa classe é **silencioso**: um `ALTER EXTENSION UPDATE` incompleto
sobe sem falhar e deixa o banco estruturalmente diferente de uma instalação limpa (o caminho AUSENTE é erro
alto — `extension.c:1415` —, o incompleto não é).

Por que o salto é convergente (ADR-1 do plano)
----------------------------------------------
Medimos: a superfície foi 18 → 48 → 60 → 78 funções `pg_extern` ao longo das releases, com
`default_version` congelado em `1.0.0`. Ou seja, **`1.0.0` rotula pelo menos cinco catálogos diferentes** — não
existe delta correto a partir dele, porque não sabemos o que há na instalação de destino. O primeiro salto tem
de levar QUALQUER catálogo rotulado 1.0.0 ao estado alvo. Do 1.1.0 em diante, a versão volta a ser honesta e os
saltos seguintes são deltas (parsimônia: a convergência existe só onde a incerteza existe).

O que a transformação faz
-------------------------
1. `CREATE FUNCTION` → `CREATE OR REPLACE FUNCTION`. Preserva owner e ACL — o que um `DROP`+`CREATE` perderia
   (temos `REVOKE ... FROM PUBLIC` em superfície sensível).
2. Objetos SEM `OR REPLACE` na linguagem (`TYPE`, `CAST`, `OPERATOR`, `EVENT TRIGGER`) ganham guarda de
   existência: re-executar dá `42710 duplicate_object` sem ela.
3. `DROP ... IF EXISTS` para o que existiu em release anterior e não existe mais (medido, não presumido).
4. O guard `\echo ... \quit` que o Postgres exige no topo.

Os 4 ACCESS METHODs e suas opclasses **já** vêm guardados por `DO $$ IF NOT EXISTS ... pg_am` no SQL gerado
(o código os declarava idempotentes desde sempre), e `SCHEMA`/`TABLE`/`INDEX` já saem com `IF NOT EXISTS` —
nada a fazer nesses.

Uso:
    python3 scripts/gen-upgrade-script.py <install.sql> <from> <to> > theodb_rs/sql/theodb_rs--<from>--<to>.sql
"""

import re
import sys

# Objetos que existiram numa release anterior e não existem mais. MEDIDO comparando os nomes sob `#[pg_extern]`
# nas tags v0.30.0/v0.60.0/v0.90.0/v0.110.0 contra HEAD — não presumido. Um `DROP IF EXISTS` a mais é inócuo;
# um a menos deixa objeto órfão que a instalação limpa não tem, e o oráculo de schema pega isso.
REMOVED = [
    'DROP FUNCTION IF EXISTS theodb_rs."_import_pinecone"(text, text, text, text, integer);',
    'DROP FUNCTION IF EXISTS theodb."_import_pinecone"(text, text, text, text, integer);',
]

# `CREATE X` → (catálogo, coluna, expressão que extrai o nome do statement)
GUARDS = {
    "TYPE": ("pg_catalog.pg_type", "typname"),
    "CAST": None,          # tratado à parte: cast não tem nome, é (source, target)
    "OPERATOR": None,      # idem: identificado por (oprname, esquerda, direita)
    "EVENT TRIGGER": ("pg_catalog.pg_event_trigger", "evtname"),
}


def statements(sql: str):
    """Quebra o SQL em statements de topo, respeitando `$$ ... $$` (os DO blocks do pgrx)."""
    out, buf, in_dollar = [], [], False
    for line in sql.split("\n"):
        if line.count("$$") % 2 == 1:
            in_dollar = not in_dollar
        buf.append(line)
        if not in_dollar and line.rstrip().endswith(";"):
            out.append("\n".join(buf))
            buf = []
    if buf:
        out.append("\n".join(buf))
    return out


def guard(stmt: str) -> str:
    """Envolve um statement não-idempotente numa guarda de existência, ou devolve-o inalterado."""
    # ANCORAR EM INÍCIO DE LINHA, não no início do statement: o pgrx emite um bloco de comentário
    # (`-- src/…`) antes de cada objeto, então `re.match` sobre o statement inteiro nunca casa. Esse foi um
    # bug real desta implementação — convertia 25 de 122 funções e passava despercebido, porque o script
    # ainda "funcionava" (só não era idempotente).
    #
    # E TOLERAR INDENTAÇÃO: o pgrx indenta `CREATE OPERATOR CLASS` em 4 espaços. Ancorar em `^CREATE` deixava
    # as opclasses desguardadas e o install morria em "operator class ... already exists". Segundo bug de
    # ancoragem no mesmo arquivo — por isso o `[ \t]*` está em TODOS os padrões, não só no que falhou.
    m = re.search(r"(?m)^[ \t]*CREATE\s+TYPE\s+(?:[\w.]+\.)?\"?(\w+)\"?", stmt, re.I)
    if m:
        return (
            f"DO $theodb_up$ BEGIN\n"
            f"  IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_type WHERE typname = '{m.group(1)}') THEN\n"
            f"{stmt}\n"
            f"  END IF;\nEND $theodb_up$;"
        )

    m = re.search(r"(?m)^[ \t]*CREATE\s+EVENT\s+TRIGGER\s+\"?(\w+)\"?", stmt, re.I)
    if m:
        return (
            f"DO $theodb_up$ BEGIN\n"
            f"  IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_event_trigger WHERE evtname = '{m.group(1)}') THEN\n"
            f"{stmt}\n"
            f"  END IF;\nEND $theodb_up$;"
        )

    # CAST e OPERATOR não têm nome simples de catálogo (um é (source,target), o outro é (nome,esq,dir)), então
    # a guarda é capturar o "já existe" — semanticamente idêntico a "crie se não existir".
    #
    # Os DOIS SQLSTATEs são necessários, e isso foi MEDIDO, não presumido: `CREATE CAST` duplicado levanta
    # `42710 duplicate_object`, mas `CREATE OPERATOR` duplicado levanta **`42723 duplicate_function`** — a
    # primeira versão deste gerador só capturava `duplicate_object` e o install falhava em `operator <-> already
    # exists`.
    #
    # Deliberadamente NÃO usamos `WHEN OTHERS`: engoliria erro real (Rule 8 — nunca engolir exceção). Capturar
    # exatamente as duas condições de "já existe" mantém qualquer outra falha ALTA.
    if re.search(r"(?m)^[ \t]*CREATE\s+(CAST|OPERATOR)\b", stmt, re.I):
        return (
            f"DO $theodb_up$ BEGIN\n"
            f"{stmt}\n"
            f"EXCEPTION WHEN duplicate_object OR duplicate_function THEN NULL;\nEND $theodb_up$;"
        )

    return stmt


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    install_sql, v_from, v_to = sys.argv[1], sys.argv[2], sys.argv[3]

    with open(install_sql, encoding="utf-8") as fh:
        raw = fh.read()

    out = [
        f'\\echo Use "ALTER EXTENSION theodb_rs UPDATE TO \'{v_to}\'" to load this file. \\quit',
        "",
        f"-- M137 — salto CONVERGENTE {v_from} → {v_to}.",
        f"-- GERADO por scripts/gen-upgrade-script.py a partir do SQL de instalação do pgrx. NÃO editar à mão:",
        f"-- regenere. Editar um script já lançado nunca chega a quem já atualizou (lição do ParadeDB 0.24.1).",
        "--",
        f"-- Convergente porque `{v_from}` rotula catálogos diferentes (a superfície foi 18→48→60→78 funções",
        f"-- com a versão congelada). Este script leva QUALQUER um deles ao estado {v_to}, e é idempotente:",
        "-- rodá-lo duas vezes não erra e não muda o schema.",
        "",
        "-- (1) objetos que existiram em release anterior e não existem mais (medido, não presumido)",
        *REMOVED,
        "",
        "-- (2) a superfície alvo, tornada idempotente",
        "",
    ]

    n_repl = n_guard = 0
    for stmt in statements(raw):
        if re.search(r"(?m)^[ \t]*CREATE\s+FUNCTION\b", stmt, re.I):
            stmt, k = re.subn(r"(?m)^([ \t]*)CREATE(\s+)FUNCTION\b", r"\1CREATE OR REPLACE\2FUNCTION", stmt, flags=re.I)
            n_repl += k
        elif re.search(r"(?m)^[ \t]*CREATE\s+(TYPE|CAST|OPERATOR|EVENT\s+TRIGGER)\b", stmt, re.I):
            g = guard(stmt)
            if g != stmt:
                n_guard += 1
            stmt = g
        out.append(stmt)

    print("\n".join(out))
    print(
        f"-- transformações aplicadas: {n_repl} CREATE FUNCTION → OR REPLACE, {n_guard} objetos guardados,",
        f"{len(REMOVED)} DROP IF EXISTS.",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
