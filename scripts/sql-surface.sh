#!/usr/bin/env bash
# sql-surface.sh — imprime a SUPERFICIE SQL da extensao numa revisao, em forma canonica.
#
# POR QUE EXISTE (#217, gate 1). O `schema-drift-gate` decidia por PROXY DE CAMINHO: qualquer arquivo
# tocado sob `theodb_rs/src/` exigia bump de `.control` ou script de upgrade. Num PR de release o
# `base.sha` e `main`, entao o diff e `main..develop` — o MILESTONE INTEIRO. Todo milestone que toca
# planner/executor disparava o gate mesmo sem alterar uma unica funcao SQL: nao havia caminho para o
# verde, e o gate reprovou em 100% dos PRs de release recentes.
#
# Um gate que sempre falha nao e um gate. Ele ensina o time a mergear por cima do CI, e no dia em que
# pegar um defeito real ninguem vai olhar (`testing.md`: "once red tests are ignored, all tests lose
# value").
#
# ESTE SCRIPT MEDE O EFEITO, nao o proxy: quais objetos a extensao EXPOE ao catalogo. Se a superficie
# nao mudou, um refactor interno passa; se mudou, o gate cobra a cadeia de upgrade — que e exatamente
# a regra que o M137 estabeleceu, agora aplicada ao que ela sempre quis dizer.
#
# LIMITE HONESTO, e ele importa: isto e uma aproximacao TEXTUAL, nao o schema que o
# `cargo pgrx schema` emite. Um `#[pg_extern]` cujo CORPO muda sem mudar assinatura nao aparece aqui —
# e esta correto, porque o catalogo tambem nao muda. Mas uma macro que gere `#[pg_extern]`
# indiretamente, ou um tipo cujo layout mude sem renomear, escapariam. A alternativa fiel
# (`cargo pgrx schema` nas duas revisoes) exige um Postgres inicializado por revisao — caro demais
# para um gate de PR. Trocamos fidelidade total por um gate que EXISTE e passa quando deve.
#
# Uso:  scripts/sql-surface.sh <git-rev>     # ex: HEAD, origin/main, <sha>
set -euo pipefail

REV="${1:?uso: sql-surface.sh <git-rev>}"

# Objetos que chegam ao catalogo. `pg_operator`/`pg_aggregate`/`PostgresType`/`PostgresEnum` ainda nao
# aparecem neste crate (medido: 0 ocorrencias), mas ficam na lista porque o custo e zero e a ausencia
# deles amanha seria um buraco silencioso — o mesmo erro do proxy que este script substitui.
ATTR_RE='#\[(pg_extern|pg_operator|pg_aggregate)'
TYPE_RE='#\[derive\([^)]*(PostgresType|PostgresEnum)'

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

# `WORKTREE` extrai da arvore atual em vez de uma revisao. Existe para que o proprio script seja
# TESTAVEL sem criar commits descartaveis: e assim que se prova que ele acusa uma funcao nova e ignora
# uma reformatacao. Um extrator de superficie que ninguem consegue exercitar e mais um proxy, so que
# mais caro.
if [ "${REV}" = "WORKTREE" ]; then
  mkdir -p "${tmp}/theodb_rs"
  cp -r theodb_rs/src "${tmp}/theodb_rs/src"
else
  # `git archive` em vez de `git checkout`: nao mexe na arvore de trabalho nem no HEAD (git-safety.md
  # proibe `checkout`), e funciona igual para um sha remoto.
  git archive "${REV}" -- theodb_rs/src 2>/dev/null | tar -x -C "${tmp}" 2>/dev/null || {
    echo "sql-surface: revisao ${REV} sem theodb_rs/src" >&2
    exit 0
  }
fi

{
  # 1) Assinaturas exportadas: o atributo mais a linha de declaracao que o segue. `grep -A6` cobre
  #    atributos multi-linha (`#[pg_extern(immutable, parallel_safe)]` quebrado pelo rustfmt) sem
  #    arrastar o corpo da funcao.
  grep -rhE -A6 "${ATTR_RE}" "${tmp}/theodb_rs/src" --include='*.rs' 2>/dev/null \
    | grep -oE '(pub(\([^)]*\))? )?(unsafe )?fn [a-zA-Z0-9_]+' \
    | sed 's/.*fn /fn /' || true

  # 2) Tipos/enums expostos ao catalogo.
  grep -rhE -A4 "${TYPE_RE}" "${tmp}/theodb_rs/src" --include='*.rs' 2>/dev/null \
    | grep -oE '(struct|enum) [a-zA-Z0-9_]+' || true

  # 3) `extension_sql!` — DDL literal. O CONTEUDO importa, nao so a presenca: e onde vivem operadores,
  #    casts e ALTERs que nenhum atributo declara.
  #
  #    COMENTARIOS SAO REMOVIDOS ANTES do casamento. Sem isso, uma frase em portugues contendo
  #    "CREATE FUNCTION" viraria item de superficie, e EDITAR UM COMENTARIO marcaria mudanca de
  #    catalogo — um gate que reprova por prosa e tao inutil quanto o proxy de caminho que ele
  #    substitui. Medido: sem o filtro, entravam itens como "drop the persisted" e
  #    "CREATE EXTENSION so".
  grep -rhA40 'extension_sql!' "${tmp}/theodb_rs/src" --include='*.rs' 2>/dev/null \
    | sed 's|//.*||; s|^[[:space:]]*\*.*||' \
    | grep -oiE '(CREATE|ALTER|DROP)[[:space:]]+(OR[[:space:]]+REPLACE[[:space:]]+)?(TABLE|INDEX|FUNCTION|TYPE|SCHEMA|OPERATOR|EVENT|TRIGGER|CAST|AGGREGATE|EXTENSION|CLASS)([[:space:]]+(IF[[:space:]]+(NOT[[:space:]]+)?EXISTS[[:space:]]+)?[a-zA-Z0-9_."]+)?' \
    | tr -s ' \t' ' ' || true
} | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//' | grep -v '^$' | sort -u
