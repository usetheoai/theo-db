---
type: Decision
title: ADR 0059 — O fail-open do agregado cobre também a falha de spill, e o que isso custa
description: Duas queries do ClickBench regrediram a 100M porque o streaming removeu a pool generosa que era efeito colateral do batch — e o recuo que as salva re-materializa exatamente o consumo que o milestone existia para remover.
resource: git:f7c7b93:docs/adr/0059-m169-fail-open-cobre-falha-de-spill.md
tags: [adr, datafusion, spill, descritores, fail-open, clickbench, honestidade, m169]
adr_id: "0059"
adr_status: Accepted
decision_date: 2026-07-31
milestone: M169
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0059
    resource: git:f7c7b93:docs/adr/0059-m169-fail-open-cobre-falha-de-spill.md
    title: ADR-0059 — O fail-open do agregado cobre a falha de SPILL
    last_modified: 2026-07-31
---

O ADR mais desconfortável do repositório, e por isso um dos mais valiosos: ele registra que a correção
adotada **é pior do que o plano previa**, e nomeia exatamente o que ficou aberto.

# Contexto

Um ADR interno de plano autorizara um fail-open **condicional** no caminho agregado: quando o braço em
streaming falhasse por esgotamento de recursos **e** um pré-check garantisse que o caminho eager
poderia ter sucesso, a consulta recuaria para o eager em vez de virar erro duro.

A corrida completa das 43 consultas do ClickBench a 100M mediu uma **regressão** que o plano não
previu: duas consultas de `COUNT(DISTINCT …) … GROUP BY` saíram de sucesso para erro.

# A causa — e ela não é defeito novo

No caminho eager, a pool de memória do [DataFusion](/technologies/datafusion.md) era dimensionada pelo
**batch decodificado**, o que a 100M dava ~2,5 GB, e o agregado nunca derramava.

O streaming removeu o batch O(N) — **que é o objetivo declarado do milestone** — e **com ele foi embora
a pool generosa, que era seu efeito colateral**. O agregado passou a derramar, e o derrame **falhou**:
o limite de arquivos por processo, dentro do `ulimit` do sistema, deixava folga quase nula para
arquivos abertos fora do gerenciador de descritores do PostgreSQL.

Havia 205 GB de disco livre. **Não é disco, não é memória: é descritor.**

E a rede de segurança do plano não pegou porque a falha chega classificada como erro de execução
genérico, não como esgotamento de recursos — a criação do arquivo é embrulhada pela biblioteca numa
classe diferente.

# Decisão

Ampliar a classe do fail-open para **incluir a falha de criação de arquivo de spill**, mantendo intacto
o pré-check. O predicado virou função pura testável.

**Erro genuíno do braço streaming NÃO recua**: recuar nele daria a resposta certa pelo caminho errado e
esconderia defeito nosso — travado por um teste dedicado.

# Alternativas rejeitadas

| Alternativa | Por que não |
|---|---|
| Desabilitar o gerenciador de disco do agregado | A pressão sairia como esgotamento de recursos, que o predicado antigo já casava — solução tipada e elegante. Mas **mata o spill**, e é o spill que faz outra consulta completar. Trocaria duas consultas por uma. |
| Restaurar o piso da pool no streaming | Reintroduz exatamente a acoplagem entre pool e batch que o milestone remove. |
| Casar a classe de erro inteira | Recuaria em erro genuíno, mascarando defeito do streaming. |
| Subir o limite de descritores da máquina | É configuração de ambiente compensando código que passou a criar arquivos temporários que o PostgreSQL não contabiliza, não limita e não limpa no crash. Trataria o sintoma na máquina, não o contrato no código. |

# Consequências — e a que dói

**As duas consultas voltam a completar, mas pelo caminho eager** — ou seja, **com o consumo O(N) que
este milestone existe para remover**. Provado, não suposto: duas linhas de marcador no log do servidor,
uma por consulta, cada uma com o erro de "arquivos demais abertos". Uma ocorrência por consulta, e não
duas, significa que o eager **não** falhou também.

**Isso é honestamente pior do que o plano previa.** O pré-check que autoriza o recuo cobre o teto de
offsets **e apenas ele** — o próprio código declara que uma relação larga de largura fixa pode esgotar
memória mesmo com o pré-check passando trivialmente. Para essas consultas, o pré-check passa (não há
coluna de texto) e o recuo re-materializa ~2,5 GB.

**Consequência aberta, registrada e não resolvida:** **não foi medido** se essas consultas passariam
**pelo streaming** com um orçamento de descritores maior. Se passassem, a classe ampliada seria defesa
em profundidade em vez de ser **a razão pela qual duas consultas completam** — e essa distinção
pertence ao artefato. Fica como trabalho declarado, não como pendência esquecida.

**Concorrência:** a medição é de corrida serial, com uma conexão. N backends recuando ao mesmo tempo
pagam N × O(N). **Não medido.**[^adr0059]

A evidência está em [m169 — delta](/benchmarks/m169-t41-delta.md) e
[m169 — t41](/benchmarks/m169-t41.md).

[^adr0059]: ADR-0059 — O fail-open do agregado cobre também a falha de SPILL
