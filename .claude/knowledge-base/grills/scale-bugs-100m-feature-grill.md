---
slug: scale-bugs-100m
generated_by: roadmap-feature
date: 2026-07-29
new_milestone_id: M169
status: completed
---

# Grill — bugs de escala a 100M

## Desvios do contrato da skill, declarados

**Colisão de ID.** O parse do `ROADMAP.md` dá `max=167 → M168`, mas **M168 já está em voo** como
`.claude/knowledge-base/plans/m168-streaming-topk-plan.md` (spike que fecha o item 2b do DoD do M167,
em revisão na 10ª rodada). O M168 nunca entrou no `ROADMAP.md` porque é spike, não milestone. Usar
M168 aqui criaria dois trabalhos distintos com o mesmo ID. **Milestone criado como M169.**

**Âncora estrutural ausente.** A skill manda inserir imediatamente antes de
`## State-of-the-art references` e abortar se a seção sumiu. Este `ROADMAP.md` foi evoluído à mão ao
longo de 96 milestones e não tem essa seção — termina com `## Sequência e paralelismo`,
`## Gate de dependências`, `## Relação com o v1`. O ponto de inserção **não é ambíguo** (logo após o
bloco do M167, antes do primeiro `##` não-milestone), então segui em vez de abortar num tecnicismo
que não se aplica. Registrado aqui porque a skill pede que desvios sejam explícitos.

**Advisory da própria skill:** o roadmap tem 96 milestones, muito acima do limiar de 15 que sugere
cortar um `ROADMAP-v2.md`. Não bloqueia; fica registrado.

## Step 3 — cross-check de "Fora de escopo"

Itens declarados fora de escopo no v2:

- "Reescrever o engine PostgreSQL"
- "Reescrever HTTP/serde/crypto/parser genérico"

**Sem sobreposição.** Este milestone conserta um teto de formato Arrow (`i32` offsets) no nosso
próprio código colunar e liga uma máquina que já existe (`ColumnarChunkStream`, M168) a um segundo
caminho. Nada disso toca o engine do PostgreSQL nem reimplementa utilitário maduro.

## Q1 — O que é, e por que agora?

Corrigir os **bugs de escala** medidos no M162 a 100M linhas — que é a escala do ClickBench oficial
(99,99M). Não é trabalho de performance.

**O que mudou:** o owner declarou explicitamente que a performance atual basta (geomean 4,53× na
classe roteada, 20/43 no alvo de ≤3×) mas que **bugs de escala são inaceitáveis**. A medição do M162
mostra que a 100M apenas **19 das 43** consultas completam: 5 falham de vez e a própria corrida foi
OOM-killed em 24/43, enquanto o ClickHouse serve as 43 em sub-segundo a 10 s.

**A descoberta que motivou o recorte:** ao investigar as 5 falhas, duas coisas apareceram —

1. **q20** (`SELECT COUNT(*) … WHERE URL LIKE '%google%'`) estoura `byte array offset overflow`. Causa
   confirmada no código: o caminho agregado chama `decode_to_batch` → `decode_columns_v2`, que
   decodifica a **relação inteira** numa única array Arrow, e a coluna é `DataType::Utf8`
   (`df_executor.rs:301`) — offsets `i32`, teto de 2 GB por array. É teto de formato, não lentidão.
2. **A correção já existe neste repositório.** O M168 construiu `ColumnarChunkStream` +
   `plan_columnar_scan` para decodificar **por chunk-group de 10.000 linhas** no caminho do top-k.
   Uma array de 10.000 URLs nunca chega perto de 2 GB. Falta ligar a mesma máquina ao caminho
   agregado.
3. **q23** (`SELECT * … WHERE URL LIKE … ORDER BY EventTime LIMIT 10`) é **literalmente** a consulta
   do M168, cujo maior bloco decodificado caiu de 772 MiB para 17,9 MiB. O OOM que ela deu no M162 é
   de antes do M167 (roteamento) e do M168 (streaming). Hipótese testável, não fato.

## Q2 — Dependências

**M167 `[x]`** — o roteamento do top-k de projeção.

Dependência não-roadmap declarada: o milestone **reusa a máquina do M168** (`ColumnarChunkStream`,
`plan_columnar_scan`), que está em revisão e ainda não mergeada. O passo 1 (medição) não depende
dela; o passo 2 (conserto do q20) depende. Se o M168 não fechar, o passo 2 precisa portar a máquina
ou esperar.

## Q3 — Definition of done

Decisão do owner (2026-07-29): **só os bugs reais**. Os 3 timeouts (q17, q21, q22) ficam de fora —
não são defeitos, são consultas que não roteiam e caem no executor de linha. Serão declaradas como
limitação conhecida de performance no artefato, com a nota de que a 1B ficariam 10× piores.

Ver o bloco do milestone no `ROADMAP.md` para o DoD final.

## Q4 — Riscos novos

1. **Ligar o streaming ao caminho agregado pode mudar resultado ou perfil das consultas que HOJE
   funcionam.** O caminho agregado atende 35 das 43; um conserto que reduza memória mas altere um
   byte é regressão pior que o bug. Mitigação: gate A/B byte-idêntico (`diverged=0`) sobre a suíte
   inteira a 1M antes de medir a 100M, mais o harness de cobertura por tipo do M163.

2. **A caixa não é a canônica, e o número pode ser lido como se fosse.** O owner escolheu droplet
   DigitalOcean equivalente (32 GB / 16 vCPU) em vez da `c6a.4xlarge` da AWS. Toda alegação tem de
   dizer "caixa equivalente, não canônica" — senão o número entra em comparação direta com a tabela
   publicada do ClickBench, que é exatamente o tipo de falso-verde que este projeto passou dez
   rodadas de review combatendo no M168.

## Step 5 — SOTA delta

Não. O acervo já tem `arrow-rs`, `datafusion` e `parquet-format`, que são as referências para offsets
`i64`/`LargeUtf8` e para decode em lotes. Nenhum peer novo necessário.

## Decisões do owner

| Pergunta | Resposta |
|---|---|
| Incluir os 3 timeouts? | **Não** — só os bugs reais |
| Qual máquina? | **Droplet DigitalOcean equivalente** (não a c6a.4xlarge canônica) |
| 1 bilhão entra? | **Não** — depois de 100M passar limpo |
