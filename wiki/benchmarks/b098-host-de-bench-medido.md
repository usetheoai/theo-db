---
type: Measurement
title: b098 — provisionar um host de bench custou ~70 min por capacidade ausente, e dois perfis do arnês estavam mortos por construção
description: Host limpo leva ~17 min até a primeira medição, snapshot leva ~2 min, e o cache de camadas sobrevive ao snapshot. Nove defeitos achados executando, nenhum por inspeção. Três alegações minhas sobre o teto de veredito foram derrubadas por medição, e nightly passou de inalcançável a VALID.
tags: [b-098, arnes, provisionamento, snapshot, portao, honest-negative, retratacao, metodo]
item: B-098
generated: { by: claude-code/opus-5, at: 2026-08-21T22:40:00Z }
---

Peças: [runbook do droplet](../runbooks/droplet-de-medicao.md) — o procedimento;
[b058](b058-crossover-colunar.md) — a medição que motivou tudo isto.

# O que foi medido

Provisionamento de um host de bench do zero, e a partir de um snapshot. Mesmo ref (`3253f86`), mesmo
tamanho (`g-16vcpu-64gb`, nyc1), medido de ponta a ponta em 2026-08-21.

| etapa | host limpo | do snapshot |
|---|---|---|
| provisionamento | 57 s | **9–18 s** |
| build da imagem | **15 min 40 s** | **11–16 s** — todas as camadas `CACHED` |
| até a primeira medição | ~17 min | **~2 min** |

**O cache de camadas do Docker sobrevive ao snapshot** — era a incógnita que sustentava ou derrubava
seu custo de ~US$ 1,24/mês. Equilíbrio em ~6 corridas/mês; abaixo disso o argumento é confiabilidade,
não dinheiro.

# O custo de não ter portão: ~70 min de host pago, zero medições

Antes de qualquer conserto, uma sessão inteira foi gasta descobrindo capacidades ausentes **uma por
vez, e sempre depois do trabalho caro**:

| falha | custo |
|---|---|
| `docker.io` do Ubuntu 24.04 não traz `buildx`; `COPY <<EOF` falha no **passo 26 de 28** | 40 min ociosos |
| `pip install` sem o extra `[postgres]` → adapter recusa no bootstrap | 3 s, após 18 min de build |
| `pip install` de tarball não leva `schemas/` → arnês invalida o bundle **no fim** | 3 s |
| `psql` de proveniência com nome de extensão obsoleto, sob `set -e`, derruba a corrida | 29 min ociosos |

Todas a mesma forma: **capacidade presente na máquina de desenvolvimento, ausente num host limpo,
descoberta depois do trabalho caro.**

# Nove defeitos, e nenhum apareceu por inspeção

O sistema foi escrito, revisado e considerado pronto. `bash -n` passava nos três scripts. **Executar
contra droplets reais achou nove defeitos** — e um décimo suspeito que a medição absolveu:

1. O orquestrador nunca enviava o arnês: de host limpo **jamais funcionaria**.
2. Corrida de largada com o `apt` — `cloud-init` segura o lock do dpkg numa VM nova, o `apt-get`
   falha e o script **engolia a falha**, passando em 7 s com docker ausente. Num droplet menor
   funcionava, porque o boot já terminara: bug intermitente disfarçado.
3. `trap ... RETURN` no topo do script **não dispara** (só vale ao sair de função) — o diretório
   temporário vazava enquanto o comentário afirmava que limpava.
4. A coleta ficava **depois** da medição, e o `trap` destruía o droplet de qualquer jeito.
5. Um `scp` bem-sucedido de arquivo **vazio** contava como colheita.
6. A coleta varria `res-*` e trazia resultados de corridas anteriores — inclusive os que vieram
   **dentro do snapshot**. Colher velho junto com novo é pior que não colher: parece completo.
7. O diretório de Parquet não existia, e o arnês reportava **`sut_alive` FAIL** — culpando o servidor
   por uma falha que foi de uma consulta, com o servidor `healthy` o tempo todo.
8. `/var/run` é **tmpfs**: foi a única de nove capacidades que não sobreviveu ao snapshot.
9. `systemd` lê `48G` como 48 GiB e o parser lia como 48 GB — o mesmo texto, dois significados, e o
   cgroup ficava mais frouxo que a declaração.

**Absolvido:** um `RC_FINAL=0` numa corrida falha parecia defeito de propagação de código de saída.
Medido isoladamente: era artefato do `| tail` do próprio teste. Consertar o que não estava quebrado
teria adicionado complexidade e escondido a causa real.

# Dois perfis do arnês estavam mortos por construção

Este é o achado que vale mais que os nove, e ele **não é de hardware**:

- **A CLI nunca construía um `IsolationPlan`.** `nightly` e `release` declaram `isolation_required`,
  o que torna `cpu_limit` e `memory_limit` obrigatórios — e `RunRequest.isolation` ficava sempre no
  default vazio. **Dois dos cinco perfis eram inalcançáveis pelo próprio ponto de entrada do arnês,
  em qualquer máquina.**
- **`apply_isolation` nunca marcava `memory_limit_applied = True`.** Os dois ramos devolviam ausência,
  e um aconselhava *"run under an externally created cgroup instead"* — conselho que o arnês nunca
  verificava.

Corrigidos com TDD (9 testes RED antes da implementação; suíte em 1093 passando). Resultado medido:

```
VEREDITO: VALID   (perfil nightly)
  repetitions_completed  PASS  required=True
  process_containment    PASS  required=True
  cpu_limit              PASS  required=True
  memory_limit           PASS  required=True
  clean_source_tree      PASS
```

# Três alegações minhas, derrubadas por medição

| eu afirmei | o que a medição mostrou |
|---|---|
| "o teto de veredito exige uma deploy key do GitHub" | `clean_source_tree` valida a árvore **do arnês**; um `git bundle` a entrega limpa, **sem credencial e sem rede** |
| "o teto restante é o hardware" | o hardware barra só `release`; `nightly` estava barrado por **software**, e foi destravado |
| "`48G` são 48 GB" | `systemd` lê como 48 GiB, e o desalinhamento custou uma corrida |

A primeira chegou ao owner como recomendação de compra de acesso. Fica registrada por isso.

# O teto que resta é físico

`release` exige `cpu_governor`, e o `cpufreq` não é exposto ao hóspede numa VM. Medido num
`g-16vcpu-64gb` com swap desligado e `cpupower` tentado:

```
N/A * cpu_governor    unavailable: cpufreq governor not exposed
Host may NOT run a 'release' benchmark. Blocking: cpu_governor
```

**Nenhum número medido em droplet DigitalOcean pode ser `publishable` pelas regras do próprio arnês —
inclusive os já publicados**, que saíram todos de droplets. Isso não os invalida como evidência:
significa que a palavra correta para eles é `EXPLORATORY` ou `research`, e que chamar qualquer um de
`release` seria falso. Para claim público, bare metal com controle de `cpufreq`.

# O método que passou a ser exigido

Uma lista de armadilhas não impede a próxima; ela registra as que já doeram. O que impede é um
**portão de capacidades executável, rodado antes de qualquer trabalho caro**, e um **smoke barato**
antes do sweep:

```bash
theodb-bench/ops/provision.sh --verify   # reprova em ~2 s, ou libera
```

E a regra que organiza os executores, escrita depois de duas mortes de script: **o que MEDE aborta em
erro; o que apenas REGISTRA nunca aborta.**
