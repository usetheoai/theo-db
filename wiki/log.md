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
* **Creation**: `benchmarks/m177-hop-vs-residencia-verdict.md` — primeira medição do M177 (fase 1, parcial). Registra também, deliberadamente, um **artefato enviesado preservado**: a primeira coleta do custo do hop produziu valor **negativo** com significância, o que é fisicamente impossível, e a causa foi orçamento de CPU desigual entre os braços (`taskset` no cliente, servidor livre). O JSON errado fica em `benchmarks/artifacts/m177/hop-cost-biased-run1.json` em vez de ser descartado — um resultado impossível que sobreviveu a duas coletas diz mais sobre a régua do que o número correto que veio depois. O conceito declara em seção própria o que **não** mede: qualidade (nenhum nDCG foi coletado), português especificamente, e empacotamento de pesos.
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
