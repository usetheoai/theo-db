#
## 2026-08-12

**b035 — a comparação que só existe com recall casado.**

Acrescentado `benchmarks/b035-theodb-vs-pgvector-pg18.md` — primeira corrida do VectorDBBench com cliente
próprio, contra pgvector 0.8.6, os dois em PostgreSQL 18.4, num droplet `g-16vcpu-64gb` (o `16c64g` que é o
rótulo de referência do upstream), destruído ao fim.

**A recall casado (~0,983) o pgvector faz +16% de QPS.** A leitura ingênua da mesma tabela — `ef_search=64`
dos dois lados, que *parece* a comparação justa — diria TheoDB +26%, porque nesse ponto o TheoDB entrega
recall 0,96 contra 0,9835. Ele é mais rápido porque procura menos.

Registro isto por acréscimo e com destaque porque o defeito é sedutor: o parâmetro estava igual dos dois
lados, e mesmo assim o ponto de operação não estava.

**Não contradiz o [m72](benchmarks/m72-qps-multiclient.md)**, que mede +11% para o índice próprio a 1M × 128d
e recall ~0,91. São regimes diferentes (50K × 1536d, recall ~0,983 aqui) e o M72 já se declarava num regime
favorável. O que a corrida mostra é que aquele resultado **não generaliza** — não que fosse falso.
# 2026-08-12

## 2026-08-13

**b045 — a paridade lexical passa de observada a demonstrada.**

Teste de permutação pareada sobre **6.980 consultas**: TheoDB vs Elasticsearch **p=0,477**, vs OpenSearch
**p=0,466**, com IC 95% de [−0,0011, +0,0025] em NDCG e `d_z` de 0,009. **6.484 consultas empatam
exatamente**; as 496 restantes se dividem 233 a 263.

**É a espécie certa de não-significância** — e a ferramenta distingue as duas explicitamente, porque tratá-las
como iguais é como se afirma paridade sem tê-la medido: `p` alto com IC **estreito** em torno de zero é
evidência de equivalência; `p` alto com IC **largo** é falta de poder.

O dado por consulta veio **por fora do arnês**: ele computa métrica por consulta em `serial_runner.py` e a
descarta no `return`, e persistí-la lá atravessaria o núcleo que a Política de Fork manda não tocar. O
avaliador reusa a porta `VectorDB.search_documents` e as funções de métrica do próprio arnês — idênticas por
construção — e a média por consulta foi **verificada contra o agregado publicado** antes de qualquer `p` sair.

**O guard do [[B-041]] disparou de verdade** durante a execução: apontei o avaliador para um nome de coleção
errado e o cliente recusou buscar, em vez de devolver 6.980 zeros que virariam NDCG 0.

**O que segue sem teste:** os 4,3× de QPS (QPS não tem valor por consulta — o caminho é N corridas
repetidas) e o +5,6% do stemming (os arrays do lado sem stemming não foram preservados).

## 2026-08-13

**b047 — a comparação lexical real, e a terceira variação do mesmo erro.**

TheoDB × Elasticsearch 9.1.2 × OpenSearch 2.17.1, MS MARCO 100K, mesma máquina, mesma corrida. **Com o
analisador casado, a qualidade é paridade** (NDCG 0,7351 contra 0,7343 e 0,7344 — terceira casa decimal) e o
**Elasticsearch faz 4,3× o nosso QPS**, com p99 2,3× menor. Somos **2× mais rápidos na carga**.

**A tabela que eu quase publiquei dizia +6,4% de NDCG para nós.** Era product-default: o mapeamento que o
arnês configura para Elastic e OpenSearch usa o analisador `standard`, que **não stemiza**. Dando `english`
aos dois, o NDCG deles sobe de 0,6908 para 0,7343 e a vantagem some inteira.

É a terceira variação do mesmo erro em três ciclos: no b035 o **parâmetro** era igual e o ponto de operação
não; no b044 o **rótulo** era igual e a máquina não; aqui a **configuração padrão** era a de cada um e o
pré-processamento não. O ADR-0061 passa a exigir também o pré-processamento casado.

As duas rodadas ficam publicadas, porque respondem perguntas diferentes: product-default diz o que o usuário
recebe ao instalar; analisador casado diz qual motor ranqueia melhor.

**Dois defeitos de ferramenta no caminho:** o cliente OpenSearch do arnês era inrodável (`"30m"` passado como
timeout numérico — corrigido no fork, candidato a PR upstream), e o `elasticsearch-py` não-pinado instala 9.x,
que contra servidor 8.15 dá um 400 opaco na criação do índice.

## 2026-08-13

**b044 — stemming entra, e o controle na mesma máquina desfaz uma conclusão minha.**

A/B controlado (mesma máquina, mesmo caso, mesmo dataset em cache, só a imagem muda): NDCG@10 **+5,6%**,
recall **+5,5%**, MRR **+5,5%** — e **QPS +10,9%**, com p99 10% menor. Remover stopwords encurta as listas
de postings mais do que o stemmer as alonga. O único custo é o build: +1,03 s sobre 100.000 documentos.

**Minha primeira leitura dizia −31,8% de QPS e estava errada.** Eu comparara a corrida com stemming (Xeon
8168) contra a corrida sem, feita antes noutro droplet (Xeon 8358). NDCG/recall/MRR são independentes de
hardware; QPS e latência não são. Refeito na mesma máquina, o sinal inverte.

É o erro do b035 num eixo novo: lá o parâmetro era igual e o ponto de operação não; aqui o rótulo era igual
e a máquina não. O ADR-0061 já exigia mesma máquina para concorrentes — vale igual para antes-e-depois do
mesmo motor, e o ADR passa a dizer isso.

**Desenho que tornou tudo isso barato:** o analisador é registrado sob nome próprio (`theodb_en`), nunca
redefinindo `default`. O Tantivy serializa o nome no schema de cada índice, então índice antigo continua
respondendo igual, sem migração — provado por teste que constrói um índice legado e verifica que ele NÃO
stemiza.

## 2026-08-13

**ADR-0061 — todo pilar mensurável tem benchmark oficial público.**

Decisão do owner depois das duas primeiras corridas: arnês de terceiros (não caseiro), concorrentes na mesma
máquina e na mesma corrida, métrica de qualidade ao lado de toda métrica de velocidade, e ponto de operação
casado — não parâmetro casado.

**A decisão diz o próprio limite:** não é mecanizada (nenhum hook verifica) e ainda não inclui significância
estatística. Enquanto o B-045 estiver aberto, toda diferença publicada é observada, não demonstrada.

**Uma correção de número publicado, feita no mesmo dia.** O artefato do b035 dizia "pgvector constrói o
índice 2,7× mais rápido", citando `load_duration` — que soma inserção e construção. Decomposto: a inserção
está em paridade (18,80 s contra 19,66 s) e o **build é 3,6× mais lento** (35,09 s contra 125,09 s). A
redação anterior atribuía à carga um custo que é do build.

## 2026-08-13

**b040 — o pilar lexical entra no arnês, e o handicap vem antes do número.**

Acrescentado `benchmarks/b040-theodb-fts-msmarco.md`. MS MARCO 100K, num droplet `g-16vcpu-64gb` destruído
ao fim: **NDCG@10 0,6962, recall@10 0,8025, MRR 0,667**, 1.616 QPS de pico, p99 serial 4,8 ms.

O artefato abre declarando que o TheoDB **não faz stemming**, não tem operadores de consulta e não expõe
`k1`/`b` — porque Elasticsearch e OpenSearch, os motores da mesma tabela, stemmizam por padrão. Um número de
NDCG lido sem isso atribui ao motor de ranqueamento uma diferença que é de pré-processamento.

**Não há comparação com outro motor neste artefato**, e isso é deliberado: citar os números publicados do
leaderboard ao lado destes compararia corridas em máquinas, versões e datas diferentes — o mesmo erro que o
b035 documentou no eixo vetorial.

Correção medida a caminho: a nota do B-004 dizia que "a superfície não expõe busca multi-termo". **Falso como
escrito** — multi-termo funciona com scores acumulados corretos. O que falta são operadores e stemming.

**B-030/B-031 — uma extensão, e o que a wiki teve de deixar como estava.**

`CREATE EXTENSION theodb CASCADE` deixou de funcionar: o umbrella `theodb` foi absorvido pelo
`theodb_rs` e não existe mais como extensão instalável. **13 linhas em 12 arquivos** foram atualizadas
para `theodb_rs` — os guias e as páginas de feature que *instruem* o leitor a instalar. Um guia que manda
rodar um comando que falha é pior que nenhum guia.

**Três ocorrências ficaram deliberadamente intactas**, e a distinção é a mesma que o bundle já aplica em
outros pontos: elas não instruem, elas **registram**.

* `wiki/decisions/0029-m70-drop-pgvector.md` (duas) — o ADR descreve uma decisão **como ela foi tomada**,
  num momento em que `CREATE EXTENSION theodb` era o comando correto. Reescrevê-lo faria o registro
  afirmar que a decisão de 2026 previa um mundo que só passou a existir agora.
* `wiki/benchmarks/m184-pilares-superficie-medida-verdict.md` — o veredito registra **o comando que foi de
  fato executado** na medição. Trocá-lo transformaria um relato de execução em ficção, exatamente o
  `cobertura-alegada-sem-execucao` que este acervo documenta como classe de defeito.

**Correção registrada por acréscimo.** Durante a análise que originou o B-031, afirmei — citando o
cabeçalho de `theodb_rs/sql/schema_snapshot.sql` — que o oráculo da cadeia de upgrade não cobre ACL.
**Está errado.** O arquivo tem um segundo bloco que faz snapshot de `proacl` e declara fechar a lacuna; o
cabeçalho é que ficou obsoleto quando a lacuna foi fechada. O problema real é outro e permanece: quem
comparava aquele snapshot contra uma baseline era um script removido em `8605677`, de modo que a cobertura
existia como SQL e não como verificação. Fica por acréscimo porque a leitura errada chegou a ser
comunicada ao owner e baseou a redação inicial do item de backlog.

## 2026-08-11

* **Creation**: `benchmarks/b015-cinco-contadores-em-zero-duas-causas.md` — a medição do B-015. Registra
  duas coisas que só a medição separou: os cinco testes com contador em zero não tinham causa comum (três
  eram o fixture do `pg_test`, dois eram instrumentação perdida num caminho de scan que virou default), e a
  **hipótese de paralelismo que o item carregava está refutada** — o `Custom Scan` colunar não paraleliza, e
  o próprio seed já desligava paralelismo desde que foi escrito.

  A hipótese refutada fica escrita no conceito em vez de ser apagada, porque foi ela que sustentou a
  prioridade do item durante um dia. Mesmo critério dos honest-negatives já no acervo.

  Não vira conceito, e o skip é declarado: `B-018` (o planner na junção) **não reproduziu** em seis cenários
  e continua aberto — não há medição conclusiva para registrar, só um espaço de busca reduzido, que vive no
  `BACKLOG.md`. Registrar "não achei" como `Measurement` inflaria o acervo com a ausência de resultado.

## 2026-08-08

* **Update**: `benchmarks/m184-symqg-profile-simbolos-verdict.md` — fechado o regime que faltava: a **busca**, onde os 2,6–3,9× do e2 foram medidos. O contraste é o achado: no HNSW **nenhuma função do `theodb_rs.so` passa de 1,3%**, enquanto no SymQG **`gather_symqg_candidates` concentra 18,23%**. Build e busca têm gargalos **distintos**, ambos em funções específicas do caminho SymQG. E um achado de planner que explica duas falhas anteriores de perfilar a busca: **sem `enable_seqscan=off` E `enable_sort=off` o planner escolhe `Sort` + `Seq Scan` em vez do índice vetorial** — eu perfilava um caminho que não passava pelo índice. Registrado como pergunta aberta maior que o SymQG: se o planner também despreza o `theodb_hnsw` em produção, isso não foi investigado.

* **Creation**: `benchmarks/m184-symqg-profile-simbolos-verdict.md` — fecha o limite que o perfil anterior declarou como seu maior. **E corrige o diagnóstico daquele limite**: eu atribuíra a falha de símbolos a "release sem debuginfo", e é **falso** — o `.so` tem **86.191 símbolos estáticos** com nomes Rust. O `perf` do host falhava por **resolução de caminho** no namespace do container; a correção foi `--pid=host` mais um `docker cp` do `.so` para o mesmo caminho. Concluir "precisa rebuildar com debug" teria custado um build inteiro para resolver uma cópia de arquivo. Com símbolos: **`ambuild_symqg` domina 39,27% do build** e o HNSW não tem função análoga; os dois **compartilham o mesmo kernel SIMD** (`l2_sq`, `l2_dist_from_bytes`), de modo que o SymQG não é mais lento por calcular distância pior — é mais lento por gastar 39% em outra coisa.

* **Creation**: `benchmarks/m184-symqg-profile-verdict.md` — responde a pergunta que os três artefatos do e2 deixaram: **por que** o SymQG é mais lento. Nenhum deles investigou o mecanismo (verificado por grep — *mecanismo*, *gargalo*, *perf*, *profil* não aparecem). O perfil **contradiz a hipótese registrada** na feature: se o custo fosse o "imposto de página, WAL e MVCC", o SymQG passaria mais tempo no kernel; ele passa **menos** (18,7% contra 27,9% do HNSW) e mais em `theodb_rs.so` (76,5% contra 66,0%). O tempo de kernel em ambos é **escalonador**, sem nenhum símbolo de I/O, página ou WAL. **É compute-bound, e o custo mora no nosso código.** Limite grande e declarado: os símbolos do `.so` **não resolvem** (release sem debuginfo), então a atribuição é por objeto e não por função — nenhuma otimização específica pode ser proposta a partir deste artefato.

* **Update**: `benchmarks/m184-pilares-superficie-medida-verdict.md` — **retratada a terceira divergência, no mesmo dia**. Eu havia concluído que os opclasses documentados não existiam, porque `USING theodb_hnsw (v vector_l2_ops)` falha. O comando falha e a conclusão não se sustenta: há **dois caminhos coerentes** — nativo (`theodb_hnsw` + `theodb_hnsw_l2_ops`) e compat pgvector via [shim do ADR 0058](/decisions/0058-pgvector-compat-shim.md) (`hnsw` + `vector_l2_ops`) —, e eu cruzei um com o outro sem instalar o shim antes de concluir. Verificado depois: `CREATE EXTENSION vector; CREATE INDEX ... USING hnsw (v vector_l2_ops)` **funciona**. Sobra um resíduo real e menor: a mensagem de erro não sugere o opclass correto nem menciona o shim. **Sexta retratação da sessão, e a primeira em que o defeito de instrumento foi não-instalar-a-peça-que-o-documento-citava.**

* **Creation**: `benchmarks/m184-pilares-superficie-medida-verdict.md` — primeira entrega do M184, e ela **já achou uma divergência**: o `theodb_symqg` está registrado como access method **no binário default**, contra a nota 1 que o classificou como experimental lendo `feature_status`. *Não recomendado* não é *ausente*, e a nota foi atribuída como se fosse. Isso **agrava** o M176: não é código atrás de flag, é superfície pública medida como 2,6–3,9× mais lenta. Confirmações também registradas: lexical com **zero** funções expostas (nota 2 correta, agora por catálogo). Limite de método declarado: contar `pg_extern` no fonte **subestima** — o `api.rs` é facade único (ADR 0009) e há `extension_sql!` declarativo; o catálogo é a fonte de verdade.

* **Creation**: `benchmarks/m177-adr0007-backends-verdict.md` — fecha a última pergunta aberta desta área, e é a **primeira medição do milestone com o PostgreSQL no laço**. O mecanismo que o ADR 0007 registrou em junho é real (backends ficam ativos durante a chamada, e crescem com a concorrência), mas o custo **não está onde o ADR temia**: a 16 clientes o pico foi de **8 backends — 8% de `max_connections`**, enquanto a p99 subiu 6,3×. O gargalo medido não é esgotamento de conexões, é latência — o que muda a conclusão sobre a fila assíncrona: ela resolveria o backend preso, que não é o que dói primeiro. Achado colateral registrado: o guard de SSRF do M134 **recusou o endpoint em runtime** e apontou a saída correta, evidência de execução de que a defesa opera fora do teste.

## 2026-08-07

* **Creation**: `benchmarks/m177-qualidade-ptbr-verdict.md` — fecha o **único item da fase 1 do M177 que nunca teve número**. Sem corpus pt-BR com qrels no repositório, a saída honesta foi derivar relevância de um corpus real do próprio projeto: os 250 conceitos desta wiki, com a `description` do frontmatter como consulta e o conceito como alvo (known-item, relevância ground-truth **por construção**, não julgamento meu). O resultado **corrige a leitura otimista** que o artefato de camadas havia deixado: o modelo mais rápido não basta — perde **37% de MRR** para o melhor, e qualidade anda junto com latência neste corpus. Cinco dos oito candidatos são **dominados** (existe outro mais rápido *e* melhor), incluindo um que a medição anterior de custo elogiara por ser "3,3× mais rápido" sem saber que recupera 42% pior. O conceito declara que os valores absolutos são **otimistas** por construção do corpus, e que o que ele mede com validade é a **ordem** entre modelos.
* **Creation**: `references/embedding-em-cloudnativepg-2026-08.md` — restrição de plataforma informada pelo owner: o banco roda sob **CloudNativePG**. Não muda nenhum número medido, muda o que eles significam — em Kubernetes o `limits.memory` do pod é rígido (OOMKill, não swap), então os 1,7 GB por processo do modelo multilíngue passariam a ameaçar **o pod do banco**. Declarado no topo como prior art e leitura de documentação, **não medição**: nada foi executado contra um cluster.
* **Update**: `benchmarks/m177-embed-concurrency-verdict.md` — **segunda retratação, e ela derruba a "maior alavanca do milestone"**. O ganho de 9,4× da configuração de thread **não existe em CPU dedicada**: 1,00× a um cliente, 0,98× a oito. Era contenção do começo ao fim. Registrado porque, na conversa imediatamente anterior à medição, este foi apontado como o número mais frágil do conjunto — a suspeita estava certa, e sinalizá-la não substituiu medi-la. **Quinta conclusão do M177 derrubada por defeito de instrumento.**
* **Update**: `benchmarks/m177-stress-colapso-verdict.md` — **retratação por máquina dedicada**. As duas patologias reportadas (colapso de throughput e vazamento de 43× na memória) **não se reproduzem** num droplet DigitalOcean `c-8` de CPU dedicada: o throughput fica **plano em ~195 rps** de 8 a 128 clientes, e o RSS cresce **16 MB** em vez de 6,8 GB. O mecanismo real é de segunda ordem — sob contenção de CPU cada pedido demora 3× mais, então mais conexões ficam simultaneamente abertas, então mais arenas de ONNX são alocadas de uma vez. A explosão era *consequência* da lentidão, não causa. Sobrevive apenas o limite de aceitação de conexão (13% de recusa a 128 clientes mesmo no dedicado). **Terceira de quatro conclusões graves deste milestone derrubada por defeito de instrumento** — e a primeira que exigiu gastar dinheiro para descobrir.
* **Creation**: `benchmarks/m177-stress-colapso-verdict.md` — o primeiro teste de **stress** do milestone, e o que muda o veredito operacional do pilar. Os testes anteriores usavam carga curta e paravam na saturação; empurrado além dela, o servidor **não degrada, colapsa**: o throughput cai de 65 rps (pico, 32 clientes) para 17,1 rps a 128 clientes, com **19,7% de recusa de conexão**, e o RSS cresce de 161 MB para **6 932 MB — sem voltar** depois que a carga cessa. Mecanismo verificado no processo vivo: `ThreadingHTTPServer` cria uma thread por conexão sem limite, e o ONNX Runtime aloca arena por thread. **A lição de método é a mais transferível**: um relatório baseado só no teste de carga teria declarado o componente pronto — nenhuma das duas patologias aparece abaixo de 32 clientes. O conceito também registra o erro de medição que o script evita: sob sobrecarga, medir só a latência dos pedidos bem-sucedidos *melhora* o número enquanto o sistema quebra.
* **Creation**: `benchmarks/m177-camadas-python-http-verdict.md` — decompõe o pedido por camada e **fixa o teto** de toda a discussão de transporte deste milestone: servidor Python + HTTP + TCP custam **0,849 ms sobre 16,649 ms — 5,1%**, porque os outros 94,9% são o ONNX Runtime, que é nativo e continuaria executando igual sob qualquer arquitetura. Registra um caso didático de **significância sem relevância**: o Unix domain socket vence o TCP loopback com p=0,0000 e ci95 [0,026–0,039] — e ganha **33 microssegundos**, 0,2% do pedido. Reportar só o p seria verdadeiro e enganoso. A primeira coleta do UDS saiu *mais lenta* que o TCP porque a variância do modelo (dp 8,0 ms) afogava uma diferença de microssegundos; o veredito veio da re-medição **sem modelo no laço**, e a coleta contaminada fica declarada no conceito.
* **Update**: `benchmarks/m177-embed-concurrency-verdict.md` — **retratação no mesmo dia**. O teto de ~20 rps publicado horas antes era artefato da configuração do medidor: o servidor rodou com `OMP_NUM_THREADS=1`, flag herdada do experimento do hop (onde equalizar threads era *necessário* para a comparação ser justa) e carregada indevidamente para um teste de concorrência. Sem a restrição, o mesmo servidor faz **32,9 rps a 1 cliente contra 3,5 — 9,4×**, e satura em ~61 rps, não ~20. A forma da curva sobrevive; os valores absolutos, não. Acrescentado ao conceito o flamegraph (`py-spy`, 4 091 amostras): **98,6% do tempo de requisição é `InferenceSession.run`** e ~1,4% é HTTP+JSON+tokenização — não há overhead a cortar no servidor. E a verificação que o ganho exigia: os vetores das duas configurações são **byte-idênticos** (diferença máxima 0,000e+00), então o 9,4× não custa qualidade. Terceira retratação consecutiva do M177 — todas por defeito de instrumento, nenhuma por defeito do sistema medido.
* **Creation**: `benchmarks/m177-embed-concurrency-verdict.md` — mede sob concorrência o gargalo que o ADR 0007 registrou em junho e explicitamente deixou para medir depois. Segundo artefato consecutivo do M177 a registrar um **artefato de medição que quase virou achado**: a primeira coleta do keep-alive deu a conexão reutilizada 40× mais lenta que a nova, assinatura do temporizador de *delayed ACK* com Nagle; corrigido com `TCP_NODELAY`, e o valor errado **não** foi publicado. O conceito declara que mede o **servidor**, não o banco — o efeito de backends bloqueados contra `max_connections`, que é o coração do ADR 0007, continua não medido.
* **Creation**: `benchmarks/m177-hop-vs-residencia-verdict.md` — primeira medição do M177 (fase 1, parcial). Registra também, deliberadamente, um **artefato enviesado preservado**: a primeira coleta do custo do hop produziu valor **negativo** com significância, o que é fisicamente impossível, e a causa foi orçamento de CPU desigual entre os braços (`taskset` no cliente, servidor livre). O JSON errado fica em `git:7cd157d^:benchmarks/artifacts/m177/hop-cost-biased-run1.json` em vez de ser descartado — um resultado impossível que sobreviveu a duas coletas diz mais sobre a régua do que o número correto que veio depois. O conceito declara em seção própria o que **não** mede: qualidade (nenhum nDCG foi coletado), português especificamente, e empacotamento de pesos.
* **Creation**: `references/embedding-local-como-extensao-2026-08.md` — primeiro conceito deste bundle **não derivado da árvore `docs/`**, e o primeiro cujo `resource` aponta para fora do repositório. Levanta o prior art de gerar embeddings localmente como extensão PostgreSQL instalável, em resposta a uma proposta do owner. Duas notas de proveniência que importam para quem consumir: **(a)** o conteúdo é prior art e o documento diz, no topo, que **não é evidência** — o acervo recusa "o projeto X faz assim" como justificativa de trabalho, e um conceito que não declarasse isso seria lido como medição nossa; **(b)** as fontes são páginas web capturadas nesta data, não arquivos versionados — se uma delas mudar ou sair do ar, o conceito registra o que era verdade em 2026-08-07 e não se auto-corrige. As licenças citadas (`pg_gembed` Apache 2.0, PostgresML MIT) foram lidas na fonte; as de `pg_infer` e `pg_onnx` **não** foram verificadas e estão marcadas como tal, em vez de omitidas.
* **Update**: a árvore de origem `docs/` foi **removida do repositório** depois da conversão — este bundle passou a ser a única documentação viva do projeto. Os 517 arquivos permanecem recuperáveis no histórico git, no commit `f7c7b93`, e todo campo `resource` deste bundle foi reescrito para a forma `git:f7c7b93:docs/…`, que resolve com `git show`. Nenhum ponteiro de proveniência ficou pendurado.
* **Update**: a remoção também apagou da árvore de trabalho os **253 artefatos brutos** que o skip abaixo declarava — ver a correção registrada no próprio item. Os runners de benchmark que os escreviam foram reapontados para `benchmarks/artifacts/`, fora deste bundle: dado bruto de medição não é conceito, e despejá-lo aqui poluiria o acervo.
* **Update**: dois gates do repositório que liam a documentação de origem passaram a ler este bundle (`scripts/docs-features-lint.sh` → `features/`, `scripts/migrate-doc-check.sh` → `guides/minimal-migration.md`). **Ambos falharam na primeira execução e expuseram lacunas reais desta conversão**: o conceito do colunar não registrava a ressalva de que o seqscan plano é ~16–26× mais lento que heap, e o guia de migração havia perdido os comandos literais `USING hnsw` / `USING ivfflat`. As duas lacunas foram preenchidas contra a fonte antes de ela ser apagada. O registro fica porque um bundle que passou no validador e ainda assim perdeu conteúdo é exatamente o limite que o validador não cobre.
* **Creation**: bundle criado a partir de `docs/` do repositório theo-db. 282 conceitos: 264 derivados dos 264 arquivos `.md` da árvore de documentação (cobertura 1:1, verificada contra o inventário de descoberta) mais 18 conceitos da entity pass (17 tecnologias + glossário).
* **Creation**: mapeamento de origem para destino — `docs/adr/` (59) + `docs/decisions/` (1) → `decisions/`; `docs/benchmarks/` (169, incluindo `archive/` e os artefatos por milestone) → `benchmarks/`; `docs/features/` (18) + `docs/analytics/` (1) → `features/`; raiz (5) + `docs/ops/` (2) + `docs/migration/` (1) → `guides/`; `docs/ops/vector-scan-diagnostics.md` → `runbooks/`; `docs/handbook/`, `docs/research/`, `docs/spikes/`, `docs/packaging/`, `docs/security/` (7) → `references/`.
* **Creation**: entity pass — 17 conceitos em `technologies/` para os nomes que os documentos usam sem explicar (AlloyDB, ScaNN, pgvector, HNSW, pgvectorscale, pg_duckdb, pgrx, Tantivy, RaBitQ, RRF, DiskANN, DataFusion, BEIR, BM25, Arrow, Parquet, Go), mais um `glossary.md` para os termos cuja história cabe em uma ou duas frases.
* **Creation**: índices gerados para a raiz e para cada diretório; todos os 260 alvos de link interno verificados como resolvendo.

### Decisões de conversão registradas

* **Idioma preservado.** O bundle é em pt-BR, como a fonte. Traduzir introduziria drift de terminologia num acervo cujo valor está na precisão das ressalvas.
* **Skip declarado — 253 arquivos não-`.md`.** Os JSON, CSV e logs brutos sob `docs/benchmarks/**` são **dados de medição, não unidades de conhecimento**. Registrar o skip é deliberado: sem isso, a contagem de cobertura pareceria incompleta.

  **Correção (mesma data).** Este item afirmava que os artefatos "permanecem no repositório de origem". Isso deixou de ser verdade quando `docs/` foi removida: eles saíram da árvore de trabalho junto com ela, e hoje existem apenas no histórico git (`git show f7c7b93:docs/benchmarks/…`). A frase original fica registrada acima em vez de ser reescrita, porque ela foi a base de uma decisão — o skip só foi aceitável enquanto o dado continuava acessível, e quem ler este bundle precisa poder ver que a premissa mudou depois.
* **Drift de documentação sinalizado, não corrigido silenciosamente.** Vários documentos de origem envelheceram em relação às decisões posteriores — `quickstart.md`, `minimal-migration.md`, `unification-1-vs-2-systems.md`, `packaging-and-tuning.md` e `columnar-htap.md` descrevem extensões removidas no ADR 0029 ou caminhos superseded. Os conceitos correspondentes **reproduzem o conteúdo e marcam o trecho obsoleto**, com link para a decisão que o superou. Reescrever a fonte seria inventar; copiá-la em silêncio propagaria erro.
* **Correção de rota aplicada durante a conversão.** Dois conceitos (`m36-scan-optimization`, `m49-cosine-ip-opclasses`) foram inicialmente escritos na raiz de `benchmarks/` quando a origem está em `benchmarks/archive/`; foram movidos e os links que apontavam para eles, corrigidos.

### Limites honestos deste bundle

* **Nenhum conceito carrega `verified`.** Todo o conteúdo foi gerado por agente a partir dos documentos de origem; nenhuma confirmação humana ocorreu. Semear esse campo inflaria o tier de confiança que um consumidor calcula.
* **A entity pass é fechada sobre os nomes que os conceitos ligam.** Nomes que aparecem apenas dentro dos conceitos de entidade (por exemplo, bibliotecas citadas dentro de `technologies/`) permaneceram como prosa, conforme a regra de recursão de um anel.
* **O acervo de referências primárias do projeto não foi convertido.** O catálogo de papers e repositórios clonados citado no `CLAUDE.md` está fora do versionamento e fora do escopo desta conversão.

## 2026-08-13

**B-036 — `m` e `ef_construction` deixam de ser constantes de compilação.**

`features/02-indice-hnsw.md` **atualizado** (não duplicado): a seção "A armadilha dos parâmetros de build"
descrevia como verdade permanente algo que era estado — `WITH (m = …)` falhava com `unrecognized parameter`.
Virou "Parâmetros de build", com a tabela de faixas, os quatro caminhos que honram o valor, e o teto de `m`
derivado do page layout (39, não o 100 do pgvector, cujo teto de nível é outro).

Acrescentado à seção de qualidade do grafo o honest-negative que o knob torna relevante: **`ef_construction`
maior não é sempre melhor** — o M57 mediu 64→200 *piorando* o recall a 100k–500k, e `m` 16→32 idem (0,952).
O knob existe para ser varrido por medição, não subido no escuro; sem essa linha ao lado da tabela, a tabela
convida exatamente ao erro que o projeto já pagou.

## 2026-08-16

**O veredito vetorial mediu a biblioteca; o concorrente é um índice do PostgreSQL.**
`decisions/0035-m73-northstar-vector-verdict.md` recebeu acréscimo — nunca sobrescrita — registrando que o gap
de ~25× foi medido contra o **ScaNN OSS**, e que o produto expõe `CREATE INDEX ... USING scann`, um access
method que paga o mesmo imposto de MVCC/WAL que nós. Como o próprio ADR atribui o gap a "AH-LUT **+ não pagar o
imposto**", metade da causa não se aplica ao AM.

O veredito **não** foi invalidado: a vantagem do AH-LUT é real e medida. O que mudou é o que se pode afirmar
sobre o *produto* — e o AlloyDB Omni, que traz o ScaNN e o colunar sem GCP, tornou a comparação do ADR-0061
possível. Virou [[B-057]] (vetorial) e [[B-058]] (colunar).

Gatilho: avaliação independente publicada em 2026-08-15, que mediu o índice ScaNN 30× menor que o ivfflat com
build 7–9× mais rápido, e **não conseguiu estabelecer a recall** — 0,15 obtido, com a causa identificada
(`scann.num_leaves_to_search` sem efeito sem `LOAD`, e sem aviso). Registramos a **não-reprodução**, não uma
refutação: o avaliador declara não confiar no número e não o publica.

---

## 2026-08-17

**A comparação do ADR-0061 foi feita, e três configurações erradas vieram antes da certa.**

**O gap contra o access method foi medido, e ele colapsa.** `benchmarks/b057-scann-am-headtohead.md`:
a recall casado no SIFT-128 a 100k, na mesma máquina e no mesmo arnês, o `scann` AM é **1,2–1,6×** o
`theodb_hnsw` — não os ~25× que o ADR-0035 registrou contra a biblioteca. A entrada de 2026-08-16 acima
previu exatamente isto; a medição a confirmou.

**Duas correções ao que eu mesmo escrevi, ambas por acréscimo no ADR-0035.** A primeira comparou
`theodb_hnsw` — grafo puro, sem quantizador — contra o `scann` com AH e rescore: nosso grafo contra o
IVF-quantizado deles, comparação real e pergunta errada. O TheoDB **tem** a receita, e o arco no código
chama-se `pg_scann` (`ann/ivf_aqah.rs` no M75, `am/scan.rs::scan_ivf_aq` no M77): `pq_subspaces` é o
quantizador anisotrópico, `pq_bits=4` a largura LUT16, `aq_threshold` o T, `soar_lambda` o SOAR,
`separate_storage=1, refine=1` o rescore exato. A recall casado ≈ 0,957, o `pg_scann` faz **476,5 QPS**
contra **438,8** do `scann` — um ponto, não uma fronteira.

A segunda: `benchmarks/b061-columnar-crossover.md` preserva a retratação de que o `theodb_columnar` seria
14–20× mais lento que o heap sem crossover. Medi no default, e `theodb.enable_columnar_agg` vem **`off`** —
1407 ms com `Seq Scan` contra 108 ms com `Custom Scan (theodb_columnar_agg)`, mesma tabela, mesma query.
Com o pushdown ligado o crossover existe entre 10 mil e 100 mil linhas.

**A classe que atravessa tudo virou guia.** `guides/instrumento-reporta-o-pedido.md`: quatro instrumentos
respondem o que foi *pedido* em vez do que está *em vigor* — `current_setting` contra `pg_settings`,
`g_columnar_columns` contra `Memory Used`, residência contra o plano (e por query), e o default contra o
flag verificado. Nenhum falha; três produziram bundle `VALID` com fronteira plausível.

A assimetria é o que torna isto sério e está escrita lá: medir-nos num default aleijado custa um número;
**medir o concorrente num default aleijado produz alegação falsa que nos favorece**. O resultado que estava
na mão era "o scann teto em 0,66 de recall enquanto o nosso chega a 0,9956" — e o teto era o rescore
desligado, não o índice.

**`technologies/alloydb.md` e `technologies/scann.md` deixaram de ser escritos por documentação.** O Omni
foi executado: query layer sem storage desagregado, imagem em PostgreSQL 17.9, nada instalado por default,
opclasses `cosine`/`dot_product`/`l2`, e um engine colunar com quatro estados dos quais três caem para heap
em silêncio.

**Limite honesto:** três destas medições saíram de script direto contra os adapters, não de bundle do arnês —
corretas e **não reproduzíveis por terceiros**. Virou [[B-069]], e a regra de que toda medição publicável sai
do `theodb-bench` passou a ser invariante do repositório do arnês.

**Teto de escala do arnês, medido.** `benchmarks/harness-scale-ceiling.md` registra 10 000 000 de vetores
carregados em 155 s com **1,16 GB de RSS** contra 5,1 GB de corpus — o corpus nunca residente. A escada até
lá: `executemany` **122 s** → COPY texto **75 s** → COPY binário **16,8 s** por milhão, e o degrau do meio é
o que justificou o terceiro (dos 75 s, **72 eram `repr()` em Python**, 128 milhões deles).

**O bilhão está quantificado e não alcançado:** 520 GB de tabela, ~780 GB com HNSW, 4,3 h de carga — contra
**284 GB livres** no host medido. A capacidade está entregue e verificada a 10M; a corrida exige outra máquina,
e construir HNSW sobre 1B é trabalho de dias. Registrado porque um benchmark cujas alegações de escala
ultrapassam suas medições é pior que um que declara seus limites, e este vai ser publicado.

**A escala de referência passou a ser 20M, escolhida por medição e não por ambição.** `1,27 GB por milhão`
medido no host (558 MB de heap + 724 MB de `theodb_hnsw` para 1M × 128, m=16) põe 20M em **25,4 GB — 9% do
disco**. 100M cabe e transforma o build em horas; **~200M é o teto físico** e não deixa margem para
`maintenance_work_mem` nem para spill de ordenação. Um bilhão são 1,27 TB e **não cabe** — o que
`benchmarks/harness-scale-ceiling.md` já registrava por outra conta.

O corpus é **real**: os primeiros 20 000 000 registros do BIGANN (`bvecs`, SIFT uint8 do TEXMEX), verificados
por checksum. Repetir o SIFT1M vinte vezes exercitaria os mesmos bytes e produziria recall sem sentido — a
ressalva que a medição de 10M carrega e que esta remove.

Três peças tiveram que existir. Um **leitor** de `bvecs` que checa a dimensão declarada por registro (ela se
repete; ler plano reinterpretaria todo vetor seguinte no deslocamento errado). Um **oráculo em streaming**,
exato porque a ordem `(distância, id ascendente)` é total e o top-k de uma ordem total se recupera fundindo
top-k por partição — um top-k corrente que guardasse "os k menores por valor" **não** seria, porque num chunk
com mais empates que k a escolha é arbitrária e os ids menores morrem antes da fusão. E **uma abstração no
lugar de dois `isinstance`**: o benchmark faz exatamente duas coisas com o corpus, e nenhuma delas tem
interesse em qual forma ele tem.

**A verdade fundamental (`ground truth`) é computada, nunca lida.** O BIGANN publica ids de vizinhos para o
bilhão inteiro; contra um prefixo de 20M eles nomeiam linhas que não existem. O arnês **recusa**, em vez de
descartar — descartar **aumentaria** o recall, removendo exatamente os vizinhos que o sistema não achou.

**E a carga de 20M abortou, por defeito nosso.** `guides/orcamento-que-limita-a-coisa-errada.md` registra:
`COPY bench_vectors, line 4569000`, cancelado pelo `statement_timeout` de **consulta** aplicado a uma
**carga**. O arnês classificou certo — `budget_exceeded`, com a frase *"the system under test did not
fail"* — e essa distinção é o valor inteiro: colapsá-la publicaria *"o TheoDB não aguentou 20M"* quando o
que não aguentou foi um timeout que nós escrevemos. Build e carga agora compartilham um mecanismo só, porque
são a mesma classe de trabalho: não medido, em massa, duração proporcional ao tamanho.


**E a escala de 20M produziu o achado que corrige a própria recomendação que a escolheu.** O
`CREATE INDEX … USING theodb_hnsw` sobre 20M foi **morto pelo OOM killer** — `anon-rss:10033724kB`, com o
`DETAIL` do PostgreSQL nomeando exatamente esse comando. `benchmarks/build-hnsw-teto-de-ram.md` registra a
curva medida isoladamente: **250k → 606 MB**, **1M → 1871 MB**, ou **~1687 MB por milhão** — e
`maintenance_work_mem` estava em 64 MB o tempo todo.

**O dimensionamento que eu publiquei horas antes olhava o disco.** 1,27 GB/milhão dizia que ~200M cabiam;
pela RAM, o host de 16 GB comporta **~5,8M**. As duas contas estão certas sobre o que mediram, e escolher
20M com base só na primeira foi a omissão. A correção entrou **por acréscimo** em
`benchmarks/harness-scale-ceiling.md`, não por reescrita.

Causa-raiz localizada e filada como **#230**: `am/build.rs:403` chama `collect_corpus` incondicionalmente
no `ambuild_hnsw`, enquanto a rota de memória limitada do M96 (`should_stream`, que já lê o GUC certo) fica
atrás de um gate de opções do **IVFFlat**. E a **#221** registra a mesma classe no colunar — dois
componentes ignorando o mesmo orçamento é ausência de contrato de memória, não dois bugs.

**O que a escala de 20M de fato entrega, dito inteiro:** carrega (20 000 000 de linhas, 11 GB) e consulta
por varredura exata; **não** constrói índice de grafo neste host. Reportar só a primeira metade seria a
omissão que o acervo existe para impedir.
**E a projeção que eu tinha acabado de publicar foi contradita pela medição direta, por 2,6×.** Rodei o
build de 20M numa máquina de 64 GB em vez de extrapolar: o consumo privado real é **~13,0 GB**
(`RssAnon`), ou **0,65 GB por milhão**, contra os 1,73 GB/milhão que o ajuste de 250k–2M dava. **O consumo
é sublinear.**

O detalhe que vale guardar é que **o ajuste era bom** — o terceiro ponto (2M) foi previsto pelos dois
primeiros com 2% de erro. Ele era bom **onde foi ajustado**, e eu o usei uma década de escala adiante.
Confirmar um modelo dentro da faixa medida não licencia extrapolá-lo para fora dela, e o intervalo de
confiança de uma extrapolação não é o do ajuste.

As grandezas foram verificadas antes de afirmar a contradição: o host antigo tinha `shared_buffers` de
128 MB, então o `VmRSS` de lá é essencialmente `RssAnon`; os dois têm `max_parallel_maintenance_workers=2`
e o build corre com um backend só. A diferença é real, não de instrumento.

**O defeito não muda** — 13 GB privados para indexar 11 GB de corpus continua sendo o corpus
materializado, continua ignorando `maintenance_work_mem`, e o OOM veio com `anon-rss:10033724kB`, **logo
abaixo** dos 13 GB necessários. Muda só o teto: ~13M num host de 16 GB, não os ~4,7M projetados.

A correção entrou na issue #230 e no conceito **por acréscimo**, com o número errado riscado e não
apagado, porque ele foi publicado e citado.

## 2026-08-21

**O arnês media, e ninguém media o arnês.**

Novo conceito: [b098 — provisionar um host de bench](benchmarks/b098-host-de-bench-medido.md), e uma
atualização por acréscimo em [b058](benchmarks/b058-crossover-colunar.md).

O [[B-097]] foi entregue e medido: o planner passou a ver a contagem real (`rows=1` → `rows=200000`) e
a forma do plano do `GROUP BY` mudou nos seis pontos da faixa. **O QPS não mudou, e foi publicado como
nulo.** E uma alegação minha caiu: eu havia escrito no `CHANGELOG` e no commit que isso fechava o
[[B-095]]. Não fechou — o agregado vetorizado continua ausente. **Vi a forma do plano mudar e supus o
resto**, que é o erro que o portão de caminho analítico existe para pegar.

Perseguir essa medição expôs algo maior. Nove defeitos no ferramental de medição, **nenhum encontrado
por inspeção** — todos por execução contra hosts reais, depois de os scripts passarem em `bash -n` e
serem considerados prontos. Entre eles: um `scp` de arquivo vazio contando como colheita; o `trap`
destruindo o droplet antes de colher; e o arnês reportando `sut_alive` FAIL — *"o sistema sob teste
caiu"* — com o servidor `healthy` e um diretório faltando.

E o achado que vale mais que os nove: **dois dos cinco perfis do arnês estavam mortos por
construção.** `nightly` e `release` exigem isolamento declarado, e a CLI nunca construía um
`IsolationPlan`; além disso `apply_isolation` nunca marcava `memory_limit_applied = True`, apenas
aconselhava rodar sob cgroup externo sem jamais verificar se alguém o fizera. Não era limitação de
máquina. Corrigido com TDD, e `nightly` foi de inalcançável a `VALID`.

**Três alegações minhas sobre o teto de veredito foram derrubadas por medição**, e a primeira delas
chegou ao owner como recomendação de comprar acesso git que não era necessário. Estão registradas no
conceito em vez de apagadas.

O teto que resta é físico: `cpufreq` não é exposto ao hóspede numa VM, então **nenhum número medido em
droplet pode ser `publishable` pelas regras do próprio arnês — inclusive os já publicados.** Isso não
os invalida como evidência; invalida chamá-los de `release`.

## 2026-08-22 — b102: o que foi verificado tem de sair no artefato

Quinta ocorrência de [o instrumento reporta o pedido](guides/instrumento-reporta-o-pedido.md), na forma
espelhada: o arnês **verificava** `theodb.enable_columnar_agg` e **descartava** a resposta. Medido —
`count(*)` a 2M: **911 ms** no default do produto contra **74 ms** com a GUC ligada, **12×**. O
`system.json` de uma corrida publicada traz 14 GUCs de servidor e nenhuma de sessão; 3 de 53 conceitos
colunares mencionavam a GUC. Novo conceito: [b102](benchmarks/b102-configuracao-nao-declarada.md).
Guia atualizado por acréscimo. Conserto no arnês com 4 testes; artefatos anteriores **não** foram
reconstruídos, e a página diz isso.
