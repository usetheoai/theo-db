# Relatório sincero — programa de benchmark ClickBench (sessão 2026-07-24)

Escrito ao fim do trabalho pedido: **(a)** liberar disco, **(b)** corrigir o viés do subsample,
**(c)** escala progressiva com gates, **(d)** AWS — com previsão de budget e nenhuma máquina ociosa.
Sem maquiar. O que deu certo, o que quebrou, o que ainda não sabemos.

## Veredito em uma frase

O pipeline de benchmark está funcional e **destravado** (o bug que impedia carregar dados reais no colunar
foi corrigido), mas o programa **NÃO chegou ao número público (AWS)** — porque cada gate barato revelou um
bloqueador antes, exatamente como deveria. Três bugs sérios foram encontrados; um foi corrigido.

## (a) Liberar disco — ✅

94 GB recuperados na `theo-e2e-runner` (312→218 GB usados; 81%→57%). Causa: **114,5 GB de Docker build
cache com ZERO ativo**. Puro desperdício acumulado.

## (b) Corrigir o viés do subsample — ✅ (e foi o que destravou tudo)

O harness usava `head -n` — as **primeiras** N linhas de um dataset ordenado por tempo. Fatia temporal
estreita, sem valores TOAST, cardinalidades de `GROUP BY` artificialmente baixas: **o regime que mais
favorece o pushdown**. A amostragem inflava os nossos próprios números. Corrigido para amostragem
sistemática (1-em-K, varre o arquivo inteiro), com 7 testes e o custo declarado. Commit `65d848e`.

**Esta correção é a raiz de tudo o que veio depois** — foi ela que trouxe valores TOAST para a amostra e
expôs o #190.

## (c) Escala progressiva com gates — ✅ o fix, mais 2 bugs descobertos

### O que foi corrigido: #190 (materialização de TOAST na ingestão)

Inserções com texto > 2 KB abortavam no colunar (`cannot fetch toast data without an active snapshot`).
Ciclo completo com rigor: discover implícito → plano v1.1 (plan-confidence **89**, edge-case review com 3
MUST FIX absorvidos) → implement TDD. Correção **mais robusta** (materializar na ingestão, não emprestar
snapshot no flush — decisão do usuário). Commit `5228b0d`. Gates: build 0, fmt sem drift, clippy 0 erros,
crash-safety preservada, **não-vacuidade PROVADA** (o 1º harness que escrevi era vácuo — passava com o fix
desligado; só o teste de não-vacuidade revelou; corrigido).

### O que foi descoberto (bugs pré-existentes, mascarados pelo #190)

| Issue | Severidade | O quê | Atribuição |
|---|---|---|---|
| **#191** | HIGH | `TRUNCATE` + re-INSERT no colunar **corrompe a tabela** (`bad metapage magic`, fica ilegível) | **Pré-existente** — provado por build A/B: reproduz idêntico no binário pré-#190 (`9940da0`) |

Encontrado pela fase T2.1 do plano (que existia para investigar o "estado ilegível pós-abort"). É o T2.1
cumprindo seu papel: separar causa de consequência com evidência, em vez de dar por resolvido só porque o
INSERT parou de falhar.

### O número que o gate revelou (`clickbench-1m-postfix-2026-07-24.md`)

1M linhas, amostra sistemática: **42/43 completadas byte-idênticas, 1 timeout, 0 erros de correção.** Mas:
**só 6/43 queries engajam o pushdown vetorizado** (geomean 0,476 s); as outras 36 rodam pelo executor
row-based a ~47 s cada (geomean geral **24,5 s**). O 1,90× do M131 vinha da amostra enviesada — em dados
representativos, o colunar não é competitivo em escala para as 36 queries sem pushdown. **Não é falha do
fix; é o estado real do pilar, agora medível.**

### Trade-off do fix (T3.1 — R2, registrado, não silenciado)

INSERT de 1M sem TOAST: pré-fix ~8,6 s, **pós-fix ~10,4 s → +21%**. Marginalmente acima do teto de 20% que
o plano definiu como gatilho de reavaliação. É o preço de materializar **todo** varlena na ingestão (a
decisão EC-2, para cobrir datums expanded sem risco de use-after-free). O teste é adversário (2 colunas
text puras); numa tabela mista o overhead relativo é menor. **Follow-up disponível e mensurável:**
materializar só `VARATT_IS_EXTERNAL || VARATT_IS_EXTERNAL_EXPANDED` (não os inline) reduziria o overhead
mantendo a robustez — decisão do owner, pois muda o fix e invalidaria este benchmark.

## (d) AWS — ❌ NÃO executado, deliberadamente

Rodar o `c6a.4xlarge` (~$9) produziria dois resultados ruins: (1) reproduziria o **#191** (TRUNCATE
corrompe — qualquer re-run bate nisso), e (2) o número (24,5 s geomean, 6/43 pushdown) **não é
competitivo** para o leaderboard. Gastar AWS agora seria queimar dinheiro para publicar um número fraco de
um pilar com um bug de corrupção aberto. **Sequência correta:** corrigir #191 → melhorar cobertura do
pushdown → só então AWS.

## Previsão de budget — consolidada

### Custo REAL medido nesta sessão

| Item | Custo |
|---|---|
| Droplets efêmeros (gate inicial + fix + toolchain), c-8 @ $0,25/h, ~total 4–5 h somadas | **~$1,10** |
| Droplet criado com chave SSH errada, destruído em minutos | ~$0,02 |
| AWS | **$0,00** (não usado) |
| **Total da sessão** | **~$1,12** |

Contexto: fatura DO do mês (parcial) = **$213,33** (72% são as 2 máquinas fixas; a `theo-ci-runner` de
$14,79/mês segue ociosa). Detalhe em `clickbench-official-budget.md`.

### Previsão para CONCLUIR o programa (revisada pelos achados)

| Etapa | Pré-requisito | Custo |
|---|---|---|
| Corrigir #191 (TRUNCATE) | — | ~$1 (droplet de dev, padrão desta sessão) |
| Melhorar cobertura do pushdown | investigação própria | indeterminado (é engenharia, não infra) |
| AWS `c6a.4xlarge` para o número público | #191 fechado + pushdown competitivo | $6,82–$9,55 |

**A previsão original ($6–9 para o AWS) continua válida — mas há trabalho de engenharia antes dela que os
gates baratos revelaram.** Foi exatamente para isso que o de-risking na DO existiu: gastar ~$1 para
descobrir que os $9 do AWS seriam prematuros.

## Máquinas ao fim

`doctl compute droplet list`: apenas `theo-e2e-runner` e `theo-ci-runner` (as 2 permanentes). **Zero
efêmeras** (tag `ephemeral-bench` = 0). AWS: zero instâncias, zero volumes órfãos. Guardrail cumprido.

## Issues abertos nesta sessão

- **#190** (TOAST no flush) — **corrigido**, commit `5228b0d`, pronto para `/review`.
- **#191** (TRUNCATE corrompe) — aberto, HIGH, atribuição provada, próximo alvo.

## O que me deixa confiante — e não é o verde

O 1º harness do #190 **passava com o fix desligado**. Só descobri porque rodei o teste de não-vacuidade.
Se tivesse aceitado o verde, teria entregue um fix "provado" por um teste vazio. Mesma disciplina achou o
#191 (build A/B para não atribuir a mim um bug alheio) e mediu o overhead de 21% em vez de omiti-lo. O
valor desta sessão não foi chegar a um número bonito — foi **descobrir, com ~$1, que o número ainda não
está pronto**, e por quê.
