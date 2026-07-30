# OKF Knowledge Base

Source of Truth for a base de conhecimento operacional em **Open Knowledge Format** —
quando ela **tem** de ser lida, quando ela **tem** de ser escrita, e o que é verificado
por máquina em vez de por boa vontade.

O bundle vive em `knowledge-base/okf/`. O validador é `scripts/check_okf.py`.

## § 1 — Por que este contrato existe

O conhecimento que este projeto pagou para aprender estava espalhado por 67 arquivos de
memória, 110 blueprints, notas de implementação e mensagens de commit. **Espalhado, ele não
morde no momento em que seria útil.** O resultado foi repetir classes de erro já pagas: numa
única sessão do M169, seis diagnósticos caíram por medição, e nenhum era novo em espécie —
todos tinham precedente registrado em algum lugar que não disparou.

Um bundle que ninguém lê é pior que nenhum: ele produz a sensação de cobertura sem a
cobertura. Por isso este contrato trata **leitura** e **escrita** como obrigações separadas,
com mecanismos separados.

## § 2 — A taxonomia (LOCKED)

Cinco `type`, escolhidos pela **pergunta que cada um responde no momento de uso** — não por
afinidade temática. É isso que torna o bundle recuperável por um agente que não sabe o que
procura.

| `type` | A pergunta | Diretório |
|---|---|---|
| `Failure Mode` | "estou prestes a cometer isto?" | `okf/failure-modes/` |
| `Technique` | "qual é o método certo aqui?" | `okf/techniques/` |
| `Invariant` | "a plataforma permite isso?" | `okf/invariants/` |
| `Measurement` | "isso já foi medido?" | `okf/measurements/` |
| `Honest Negative` | "isso já foi tentado e refutado?" | `okf/honest-negatives/` |

Mais os dois reservados do OKF: `Index` (um por diretório) e `Log` (`okf/log.md`).

Acrescentar um `type` exige ADR — a taxonomia é a interface de recuperação, e uma sexta
categoria mal escolhida degrada as cinco existentes.

## § 3 — LEITURA obrigatória (§ SEMPRE)

### 3.1 Injeção por turno (mecanismo)

`hooks/userpromptsubmit-inject.sh` injeta um **ponteiro terso** para o bundle em todo prompt,
ao lado da parsimony ladder. É ponteiro, nunca conteúdo: o hook dispara a cada turno e a
`additionalContext` fica no histórico — inlinar conteúdo aqui foi documentado como causa
dominante de bloat de contexto neste repo.

### 3.2 Gatilhos em que a leitura é obrigatória (instruction-grade)

Ler o índice da categoria **antes** de:

| Vou fazer isto | Leia antes |
|---|---|
| montar qualquer medição ou benchmark | `failure-modes/index.md` · `techniques/index.md` · **`measurements/index.md`** (o número pode já existir) · **`invariants/index.md`** (4 invariantes já invalidaram medições aqui) |
| **aceitar um verde como evidência** — gate que não reclamou, script `rc=0`, suíte passando, oráculo com 0 divergências | **`failure-modes/index.md`** — quatro conceitos servem exatamente este caso |
| publicar qualquer número | `measurements/index.md` — o número pode já existir |
| propor uma aposta técnica / novo milestone | `honest-negatives/index.md` — pode já ter sido refutada |
| mexer em storage, FFI, `unsafe`, recovery, **build** ou branch compartilhado | `invariants/index.md` |
| **rodar processo longo em máquina remota, ou escolher entre duas APIs da plataforma** | `invariants/index.md` |
| abrir um issue de produto | `techniques/medir-antes-de-filar.md` |

> **Os dois gatilhos em negrito foram acrescentados em 2026-07-30**, depois de o review de recuperabilidade
> medir que os cenários "lancei um build remoto via ssh" e "o gate não reclamou, está tudo certo?" **não eram
> roteados por nada** — o segundo servido por quatro `failure-mode` e alcançável só por varredura espontânea.

**Limite honesto:** nenhum hook consegue provar que eu li. A injeção é o mecanismo mais forte
disponível, e os gatilhos acima são instruction-grade — a mesma classe dos degraus 2, 3 e 5 da
parsimony ladder, que também não são auto-detectáveis. Fingir que isto é mecanizável seria a
mesma desonestidade que o bundle documenta em `cobertura-alegada-sem-execucao`.

## § 4 — ESCRITA obrigatória (§ atualização constante)

### 4.1 O que **tem** de virar conceito

| Evento | `type` | Por quê |
|---|---|---|
| um número publicado em `docs/benchmarks/**` | `Measurement` | número fora do bundle é número que será re-medido |
| uma alegação minha derrubada por medição | `Failure Mode` (a classe) + `Measurement` (o número) | é o material de maior valor da série, e o mais fácil de esquecer |
| uma aposta medida e refutada | `Honest Negative` | sem registro, ela volta a cada planejamento parecendo novidade |
| uma propriedade de plataforma aprendida por falha | `Invariant` | crash, segfault, truncamento, unwinding, licença |
| um método que passou a ser exigido | `Technique` | e ele deve linkar o `Failure Mode` que o originou |

### 4.2 O que **NÃO** deve virar conceito

Escrever entrada de enchimento para satisfazer um gate é pior que não escrever — treina o
hábito e dilui o sinal. **Não** viram conceito:

- rastro de execução (o que foi feito, por quem, quando) — isso é `knowledge-base/` do ciclo;
- decisão de arquitetura — isso é ADR em `docs/adr/`;
- um bug corrigido sem lição generalizável;
- repetição de algo que já é conceito. **Atualize o existente**, com data.

### 4.3 Regra de atualização

Um conceito é **vivo**: quando a mesma classe reaparece, o arquivo existente ganha a nova
ocorrência e o `timestamp` sobe. Criar um segundo arquivo para a mesma classe fragmenta a
recuperação — é o inverso do que o bundle existe para fazer.

Toda entrada nova ou revisão substantiva registra uma linha em `okf/log.md`.

## § 5 — Verificação (o que é MÁQUINA, e o que não é)

### 5.1 Determinístico — `scripts/check_okf.py`

Cinco checagens, todas com superfície de falso-positivo **zero**:

| # | Checa | Por quê é hard gate |
|---|---|---|
| C1 | todo conceito declara `type` | é o único campo obrigatório do OKF v0.1 |
| C2 | todo link markdown interno resolve | o bundle prega "citação que não resolve não entra" — ele tem de cumprir |
| C3 | cada `index.md` lista **exatamente** os conceitos ao lado | índice que deriva **esconde** conceito, e esconder é pior que faltar |
| C4 | `index.md` e `log.md` existem na raiz | são a porta de entrada e a história |
| C5 | o **valor** de `type` está no conjunto fechado do § 2 | C1 só checa presença; um sexto tipo exige ADR — e a porta de entrada tinha exatamente esse defeito (review 2026-07-30) |

Códigos de saída: `0` válido · `1` achado estrutural · `2` erro de invocação.

O validador **não** julga qualidade de conteúdo, deliberadamente. Um checker que fingisse
graduar prosa seria exatamente o `cobertura-alegada-sem-execucao` que o bundle documenta.

### 5.2 Hard gate no Stop — `hooks/stop-validation.sh`

| Condição | Ação |
|---|---|
| qualquer arquivo de `knowledge-base/okf/**` mudou **e** `check_okf.py` sai ≠ 0 | **BLOCK** |
| `docs/benchmarks/**` mudou **e** nenhum `okf/measurements/**` foi tocado | **BLOCK** |

O segundo gate é o coração do contrato: **um número publicado que não está na base é
exatamente a falha que a base existe para corrigir.** Ele é determinístico (dois caminhos de
arquivo), então bloquear é seguro.

Escape documentado: `STOP_VALIDATION_WARN_ONLY=1` — para reorganização em massa, com o
racional registrado. Usá-lo para pular a escrita de um conceito é violação do contrato, não
uso da válvula.

### 5.3 O que fica advisory, e por quê

Não há gate mecânico para "este conceito é bom" nem para "você leu antes de medir". Um BLOCK
falso sobre heurística é pior que um WARN — é a mesma decisão que `reference-provenance.md`
§ 2 tomou para o detector de vazamento, pelo mesmo motivo: casamento exato é evidência forte,
não prova.

## § 6 — Anti-patterns

- **Escrever conceito de enchimento para destravar o gate.** O gate existe para capturar
  conhecimento real; enchimento o transforma em cerimônia e envenena a recuperação.
- **Publicar número em `docs/benchmarks/` e adiar o `Measurement`.** "Depois eu registro" é
  como o conhecimento se dispersou da primeira vez.
- **Criar um segundo arquivo para uma classe que já existe.** Atualize o existente.
- **Tratar o bundle como arquivo morto.** Se um conceito está errado, corrija-o — um conceito
  errado é pior que ausente, porque será citado.
- **Citar um conceito sem abrir o arquivo.** É o `diagnostico-aceito-sem-reproduzir` aplicado
  ao próprio bundle.
- **Dispensar `check_okf.py` por ADR quando ele pode rodar.** Ele não tem dependência: é
  Python puro sobre arquivos locais. Não há ambiente em que ele "não dá para rodar".

## Cross-references

- Bundle: `../knowledge-base/okf/index.md`
- Validador: `../scripts/check_okf.py`
- Gate no Stop: `../hooks/stop-validation.sh`
- Injeção por turno: `../hooks/userpromptsubmit-inject.sh`
- Regra irmã que também é deliberação instruction-grade: `parsimony-ladder.md`
- Precedente da decisão BLOCK-vs-WARN em heurística: `reference-provenance.md` § 2
- Disciplina de rigor de medição que o bundle operacionaliza: `discover-phd-rigor.md` (R3, R3.1)
