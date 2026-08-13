#!/usr/bin/env bash
# B-029 — nenhum workflow pode invocar caminho que não existe.
#
# Por que este verificador precisa existir, e por que o `actionlint` não basta:
# medido em 2026-08-13, havia **10 invocações de 6 scripts ausentes** em três workflows, e o
# `actionlint` passou VERDE sobre elas em 2026-08-12. Ele valida a sintaxe do YAML e a forma
# das expressões — não a existência do arquivo que um `run:` manda o bash executar. São
# perguntas diferentes, e só uma delas estava sendo feita.
#
# O custo de não perguntar: os scripts saíram em `8605677` e a quebra ficou **armada** por um
# dia e meio sem ninguém notar, porque os gates só rodam em push para develop/main e o commit
# ainda não chegou lá. Quando chegasse, o job falharia por arquivo ausente — indistinguível de
# uma regressão real, que é exatamente a classe que o B-027 acabou de eliminar do outro lado.
#
# Contrato:
#   check-workflow-paths.sh [<git-rev>]
#     sem argumento -> verifica a árvore de trabalho
#     com <git-rev> -> verifica a árvore daquela revisão, SEM trocar de branch
#                      (`git archive` num diretório temporário; ver `rules/git-safety.md § 2`,
#                      que proíbe `git checkout`)
#
# Saída: exit 0 quando todo caminho resolve; exit 1 listando `arquivo:linha -> caminho` para
# cada um que não resolve.
#
# LIMITE DECLARADO, e ele é reportado em vez de calado: caminho montado em variável
# (`bash "$SCRIPT_DIR/foo.sh"`) não é verificável por leitura estática. O script CONTA quantas
# dessas encontrou e imprime o número — um verificador que ignora em silêncio o que não sabe
# checar é o mesmo falso-verde que ele existe para impedir.
set -euo pipefail

REV="${1:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP=""

if [ -n "$REV" ]; then
  TMP="$(mktemp -d)"
  # shellcheck disable=SC2064  # expansão intencional no momento do trap
  trap "rm -rf '$TMP'" EXIT
  git -C "$ROOT" archive "$REV" | tar -x -C "$TMP" || {
    echo "check-workflow-paths: revisão '$REV' não pôde ser extraída" >&2
    exit 2
  }
  ROOT="$TMP"
fi

WF_DIR="$ROOT/.github/workflows"
[ -d "$WF_DIR" ] || { echo "check-workflow-paths: sem .github/workflows em $ROOT" >&2; exit 2; }

# Prefixos de diretórios do repositório que um workflow pode invocar. Deliberadamente uma
# lista fechada: casar qualquer coisa com barra produziria falso positivo em URL, imagem
# docker e caminho de runner (`/usr/bin/...`).
PREFIXOS='scripts|packaging|benchmarks|hooks|\.github'

ausentes=0
dinamicos=0

while IFS= read -r linha; do
  arquivo="${linha%%:*}"
  resto="${linha#*:}"
  numero="${resto%%:*}"
  conteudo="${resto#*:}"

  # Linha de comentário não é invocação. O `#` pode estar indentado.
  case "$(printf '%s' "$conteudo" | sed 's/^[[:space:]]*//')" in
    '#'*) continue ;;
  esac

  # Caminho montado em variável: não verificável por leitura estática. Contado e reportado.
  if printf '%s' "$conteudo" | grep -qE '\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/[A-Za-z0-9_.-]+\.(sh|py)'; then
    dinamicos=$((dinamicos + 1))
  fi

  # Todo caminho literal com um dos prefixos conhecidos.
  for caminho in $(printf '%s' "$conteudo" | grep -oE "($PREFIXOS)/[A-Za-z0-9_./-]+" | sort -u); do
    # Sufixos de pontuação que o grep arrasta do YAML (`'scripts/foo.sh'`, `foo.sh:`).
    caminho="${caminho%.}"
    [ -e "$ROOT/$caminho" ] && continue
    # Glob de `paths:` filter não é invocação — só conta se a linha executa algo.
    if printf '%s' "$conteudo" | grep -qE '(bash|sh|python3?|\./)[[:space:]]'; then
      echo "AUSENTE  $(basename "$arquivo"):$numero -> $caminho"
      ausentes=$((ausentes + 1))
    fi
  done
done < <(grep -rn -E "($PREFIXOS)/" "$WF_DIR" --include='*.yml' --include='*.yaml' 2>/dev/null || true)

if [ "$dinamicos" -gt 0 ]; then
  echo "nota: $dinamicos invocação(ões) com caminho montado em variável — NÃO verificadas por este script."
fi

if [ "$ausentes" -gt 0 ]; then
  echo "::error::$ausentes invocação(ões) de workflow apontam para caminho inexistente."
  echo "::error::Um job que falha por arquivo ausente é indistinguível de uma regressão real."
  exit 1
fi

echo "check-workflow-paths: OK — toda invocação literal resolve."
