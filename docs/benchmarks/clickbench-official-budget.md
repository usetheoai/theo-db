# Previsão de budget — ClickBench oficial (número publicável)

**Data:** 2026-07-24 · **Preços consultados via API** (`doctl invoice`, `aws pricing get-products`), não estimados.

## Custo atual da infraestrutura (DigitalOcean, medido)

Fatura do mês corrente (parcial, até 24/07): **$213,33**.

| Item | Valor | Horas | Observação |
|---|---|---|---|
| `theo-e2e-runner` (s-8vcpu-32gb-amd) | **$138,00** | 552 | runner do theo-db + k3s + containers + DEV_HOST |
| `theo-ci-runner` (s-2vcpu-2gb) | **$14,79** | 552 | **load 0,00 há 24 dias** — serve theo-cloud/lens/memory |
| `theo-m98-pgrx19` (c-8) | $14,43 | 57 | efêmero de implementação |
| Spaces (250 GiB) | $4,11 | 552 | |
| ~50 droplets efêmeros (bench/impl) | ~$42 | 5–17 cada | `theo-m57-bench` = $3,95/15 h |
| **Total** | **$213,33** | | 58 itens |

Histórico: maio **$1.106,20** · junho **$250,03** · julho **$213,33** (parcial). Tendência de queda.

**Duas fixas = 72% do custo.** A `theo-ci-runner` ($14,79/mês) está ociosa enquanto a de $138 satura
(load 29 observado). Realocar o trabalho entre as duas é o maior ganho disponível sem gastar nada.

## Custo do programa de benchmark

### (c) Validação na DO — droplet efêmero dedicado

| Item | Preço | Uso estimado | Custo |
|---|---|---|---|
| `c-8` (8 vCPU dedicado, 16 GB, 100 GB) | **$0,250/h** | 4–6 h | **$1,00 – $1,50** |

O padrão M57 (criar → medir → destruir) já é usado no projeto e custa centavos: `theo-m57-bench`
gastou $3,95 em 15 h. A alternativa — rodar na `theo-e2e-runner` — é "grátis" mas contamina a medição
com o ruído de CI/k3s/containers e satura a máquina (foi o que ocorreu nesta sessão).

### (d) Número oficial na AWS — box canônico do ClickBench

O ClickBench publica resultados no `c6a.4xlarge` com 500 GB gp2; só nessa configuração o número é
comparável ao leaderboard.

| Item | Preço (us-east-1, on-demand) | Uso estimado | Custo |
|---|---|---|---|
| `c6a.4xlarge` (16 vCPU, 32 GiB) | **$0,6120/h** | 10–14 h | $6,12 – $8,57 |
| EBS gp2 500 GB | **$0,10/GB-mês** (≈$0,069/h) | 10–14 h | $0,69 – $0,97 |
| Egress (artefatos JSON) | $0,09/GB | < 0,1 GB | ~$0,01 |
| **Subtotal (d)** | | | **$6,82 – $9,55** |

**Por que 10–14 h:** o gargalo não são as 43 queries (minutos), é carregar **99.997.497 linhas × 105
colunas** no armazenamento colunar. O download são ~14 GB comprimidos; descomprimidos, ~70 GB de TSV.
A janela larga é honesta — não temos medição prévia do load em escala completa, apenas em 100 k linhas.

### Total do programa

| Cenário | Custo |
|---|---|
| Otimista (c: 4 h, d: 10 h) | **$7,82** |
| Realista | **~$9** |
| Pessimista (c: 6 h, d: 14 h, 1 re-run) | **~$18** |

Menos de $20 para o primeiro número de ClickBench do TheoDB citável por terceiros — **desde que as
máquinas sejam destruídas ao fim**. Uma `c6a.4xlarge` esquecida ligada custa **$440/mês**.

## Guardrails de custo adotados

1. **Toda máquina efêmera é registrada** em `EPHEMERAL_DROPLET.txt` (ID + IP + timestamp) antes de
   qualquer trabalho, para que a destruição não dependa de memória.
2. **Tag `ephemeral-bench`** nos droplets — permite varredura por tag e destruição em lote.
3. **Destruição imediata** ao fim da coleta, não ao fim da análise: os artefatos JSON são copiados para
   o repositório antes de destruir.
4. Um droplet criado inacessível (chave SSH errada) foi **destruído em minutos**, não deixado ligado —
   custo do erro: ~$0,02.

## Nota sobre o que NÃO está no orçamento

- **Repetições para significância.** Um único run não dá intervalo de confiança. O ClickBench roda cada
  query 3× (cold + 2 hot) e reporta o melhor hot, o que mitiga mas não elimina a variância entre runs.
  Se quisermos barra de erro entre execuções, multiplique (d) pelo número de repetições.
- **Submissão ao leaderboard.** É um PR ao repositório do ClickBench — custo zero, mas exige que o
  número tenha sido produzido na configuração canônica, sob pena de ser rejeitado.
