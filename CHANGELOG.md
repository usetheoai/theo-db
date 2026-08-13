# Changelog

Todas as mudanças notáveis deste projeto são documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/),
e este projeto adere ao [Semantic Versioning](https://semver.org/).

> Nota: as entradas anteriores a `[0.158.0]` não citam número de ticket. A nota que
> ocupava este lugar dizia que o projeto estava "em fase inicial de design (pré-código,
> sem release)" e que o tracker "ainda não está configurado" — as duas coisas deixaram
> de ser verdade há muitas versões, e a nota permaneceu. O tracker está ativo; a partir
> de `[0.158.0]` toda entrada que tenha issue correspondente o cita.

## [Unreleased]

### Added
- **`m` e `ef_construction` viram opções de índice de verdade** (B-036): `CREATE INDEX … USING theodb_hnsw (e) WITH (m = 32, ef_construction = 200)` passa a ser aceito **e honrado** — até aqui devolvia `unrecognized parameter "m"`, e o build era fixo em 16/64. É a sintaxe de build do pgvector, que o ADR-0029 § D2 promete como drop-in
- O valor pedido é honrado nos **quatro** caminhos que constroem grafo, não só no `CREATE INDEX`: build inicial, índice vazio, INSERT posterior e o fold do VACUUM. Um índice criado com `ef_construction=200` deixaria de honrá-lo a cada linha inserida depois, e um VACUUM o reconstruiria com o default — em silêncio nos dois casos, porque `ef_construction` não é persistido em lugar nenhum (B-036)
- Fora de faixa é **recusado nomeando a opção** (`m` em 2..39, `ef_construction` em 4..1000), não truncado em silêncio. O teto de `m` é derivado do page layout — no pior caso um nó ocupa `HNSW_MAX_LEVEL·m + m0 = 34m` slots e a tupla de vizinhos tem de caber em 8.168 bytes, o que dá 39; não é o 100 do pgvector, cujo teto de nível é outro (B-036)
- **Teste de significância pareada volta ao projeto** (`benchmarks/significance/`, B-045): permutação pareada de Smucker/Allan/Carterette recuperada do histórico, mais o avaliador por consulta que nunca existiu para o VectorDBBench. 22 testes, sem banco e sem rede
- A **paridade lexical do b047 deixa de ser observada e passa a ser demonstrada**: p=0,477 contra Elasticsearch e p=0,466 contra OpenSearch sobre **6.980 consultas**, com IC 95% estreito em torno de zero ([−0,0011, +0,0025] em NDCG) — evidência de equivalência, não falta de poder. 6.484 das 6.980 consultas empatam exatamente (B-045)
- **Primeira comparação lexical contra concorrentes reais**, na mesma máquina e na mesma corrida (`wiki/benchmarks/b047-lexical-headtohead.md`): TheoDB × Elasticsearch 9.1.2 × OpenSearch 2.17.1, MS MARCO 100K. **Com pré-processamento casado a qualidade é paridade** — NDCG@10 0,7351 contra 0,7343 e 0,7344 — e o **Elasticsearch faz 4,3× o nosso QPS**. Somos 2× mais rápidos na carga (B-047)
- **Stemming e remoção de stopwords no pilar lexical** (B-044). Índices novos usam o analisador `theodb_en` — SimpleTokenizer + RemoveLong + LowerCaser + StopWordFilter + Stemmer (Snowball inglês) — e a consulta `jumping` passa a casar documentos com `jumps`. Medido em A/B na mesma máquina, MS MARCO 100K: **NDCG@10 +5,6% (0,6962 → 0,7351), recall +5,5%, MRR +5,5% — e QPS +10,9%**, porque remover stopwords encurta as listas de postings mais do que o stemmer as alonga. Custo: +1,03 s no build de 100.000 documentos
- Suporte a **full-text (BM25)** no cliente `theodb` do VectorDBBench — quatro métodos na classe existente, zero pontos de registro novos. É o **primeiro cliente PostgreSQL com FTS** no arnês (B-040)
- Primeira medição do pilar lexical em arnês público (`wiki/benchmarks/b040-theodb-fts-msmarco.md`): MS MARCO 100K, **NDCG@10 0,6962, recall@10 0,8025, MRR 0,667** a 1.616 QPS de pico, p99 serial 4,8 ms. **Sem stemming, sem operadores de consulta e `k1`/`b` não configuráveis** — declarado antes da tabela, porque os motores comparáveis stemmizam por padrão (B-040)
- Cliente `theodb` para o **VectorDBBench**, em fork de diff mínimo — `pip install "vectordb-bench[theodb] @ git+https://github.com/usetheoai/VectorDBBench@theodb"` — devolvendo ao projeto um arnês de benchmark reproduzível e multi-sistema, ausente desde a remoção de `benchmarks/` (B-035)
- `benchmarks/vectordbbench/` — compose e runner que fixam a **mesma versão de PostgreSQL** nos dois motores comparados e recusam a corrida se divergirem (B-035)
- Primeira medição comparativa publicada em `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md`: **a recall casado (~0,983) o pgvector faz +16% de QPS**, e constrói o índice **3,6× mais rápido** (a inserção está em paridade — 18,8 s contra 19,7 s; a diferença é toda da construção do grafo). A leitura ingênua da mesma corrida — `ef_search` igual dos dois lados — diria o oposto, porque ali o TheoDB entrega recall 0,96 contra 0,9835 (B-035)

### Changed
- O cliente do **VectorDBBench** deixa de recusar `m`/`ef_construction` por uma constante espelhada de `build.rs` e passa a **perguntar ao servidor** (B-046): um `CREATE INDEX` com as opções pedidas, numa tabela temporária dentro de um savepoint sempre revertido, antes da carga do dataset. Uma constante descreve o motor contra o qual o cliente foi escrito, não aquele para onde ele aponta — decidir por ela mede o rótulo em vez da capacidade, que é o erro que a corrida lexical do b047 documentou
- **A variável de ambiente `THEODB_HNSW_EF_CONSTRUCTION` foi removida** (B-036). Quem a usava em varredura de benchmark passa a usar `WITH (ef_construction = N)`, que é estritamente melhor: vale por índice em vez de por servidor, e é alcançável por uma sessão de cliente sem reiniciar nada. Duas fontes para o mesmo knob obrigariam uma regra de precedência que ninguém pediu — o custo de descobrir isso tarde foi o que o B-034 pagou
- **Índices existentes não mudam** (B-036): sem a opção declarada, `rd_options` é nulo e o acessor devolve exatamente o default de hoje (`m=16`, `ef_construction=64`), que por coincidência é o mesmo do pgvector. Nenhuma medição publicada (b035, b040, b044, b047) precisa ser refeita
- A estimativa de custo do planner passa a ler o `m` **declarado no índice** em vez da constante 16 (`am/cost.rs`, B-036). Lê de `rd_options`, nunca do meta de página: uma segunda leitura de página ali seria uma superfície de erro nova dentro do `amcostestimate`, que não pode abortar plano nenhum
- Índices lexicais **já construídos não mudam de comportamento e não precisam de migração**: o Tantivy serializa o nome do tokenizer no schema de cada índice, então os antigos continuam resolvendo `default`. Para ganhar stemming, rode `bm25_build` de novo (B-044)

### Fixed
- A migração **pgvector → TheoDB** volta a ter oráculo (B-029): `migrate-smoke.sh`, `migrate-doc-check.sh` e `migrate-smoke-selftest.sh` foram restaurados. Eles testam a promessa drop-in do ADR-0029 § D2 — dado e índices preservados por `pg_dump`/`pg_restore` padrão — e não a cadeia de upgrade que o B-031 removeu. **O oráculo restaurado reprova**: ele semeia a origem com um índice `ivfflat` e o TheoDB responde `access method "ivfflat" does not exist`, que é o B-037 quebrando a migração real
- A esteira de CI volta a ter **capacidade de reprovar** (B-029): 10 invocações em 3 workflows apontavam para 6 scripts removidos em `8605677`. Um job que falha por arquivo ausente é indistinguível de uma regressão real — a mesma classe que o B-027 acabou de eliminar do outro lado
- **`scripts/check-workflow-paths.sh`** passa a recusar qualquer workflow que invoque caminho inexistente, e roda junto do `actionlint`. Ele valida sintaxe de workflow; não abre o arquivo que um `run:` manda executar — e por isso passou verde sobre as 10 invocações mortas no dia anterior (B-029)
- O smoke do produto foi **reescrito** para as extensões que existem (B-029). O anterior fazia `CREATE EXTENSION theodb` três vezes — o umbrella que o B-030 removeu —, então restaurá-lo daria um vermelho acusando o lugar errado. O novo verifica instalação, o operador `<=>`, os **dois** nomes de access method (`theodb_hnsw` e o alias `hnsw` que toda app pgvector escreve) e que a busca devolve linha, porque zero resultado sobre 200 linhas é a classe do B-041
- O detector de dependências não usadas passa a rodar **onde o ambiente de build existe** (B-039). No host o erro real nunca foi permissão: é `cargo pgrx init` que nunca rodou ali. Cinco ciclos declararam `auditor_unavailable_cargo-udeps` como limitação de ambiente; o veredito do `/code-quality` sobe de `FAIL_SOFT` (70) para `PASS_WITH_CAVEATS` (89)
- `bm25_search` **deixa de engolir erro do parser de consulta**. Antes devolvia zero linhas, indistinguível de "nada casou" — um tokenizer desconhecido faria uma corrida de benchmark publicar NDCG 0 como se fosse medição (B-044)
- O cliente do benchmark **recusa** `m` e `ef_construction` em qualquer valor que o TheoDB não construa (16 e 64, `am/build.rs:22-23`), em vez de aceitá-los e ignorá-los. Uma corrida que reportasse `ef_construction=200` sobre um índice construído com 64 seria medição errada com aparência de certa (B-035)
- O cliente **recusa** um endereço que responde mas não é TheoDB. O pgvector satisfaz toda sondagem que o cliente faz — tem o tipo `vector`, um access method `hnsw` e `vector_l2_ops` —, então sem essa checagem os números de outro motor seriam publicados sob o rótulo TheoDB (B-035)

## [0.160.0] - 2026-08-12

> **Nota de versionamento — por que MINOR e não MAJOR.** Esta versão traz uma seção `Removed`
> substancial (o umbrella `theodb`, as cadeias de atualização, `benchmarks/` e `scripts/`), e a regra de
> derivação do `cycle-release` daria MAJOR — `v1.0.0`. Cortada como MINOR seguindo o **precedente
> documentado na `[0.159.0]`**: [SemVer §4](https://semver.org/) estabelece que em `0.y.z` nada é
> declarado estável, então remoção não força major. A escolha entre `v1.0.0` e `v0.160.0` é
> posicionamento de produto e cabe a quem aprova o PR de release; o corte como MINOR é o que preserva a
> decisão anterior em vez de reverter uma escolha do owner sem que ele tenha pedido.
>
> **Ressalva que acompanha o corte:** a esteira de integração contínua está vermelha (B-029) — os fluxos
> referenciam `benchmarks/` e `scripts/`, removidos nesta versão. Enquanto isso durar, **este corte não
> tem CI que o valide**; o que existe é a suíte 446/0 executada localmente e a imagem construída e
> exercitada. Está registrado aqui para que a aprovação seja informada, não presumida.


### Fixed
- **Os parâmetros de ajuste de busca que aplicações pgvector usam passaram a ter efeito — antes eram aceitos e ignorados em silêncio.** `SET hnsw.ef_search` e `SET ivfflat.probes` são os comandos que toda aplicação pgvector emite para trocar precisão por velocidade. O TheoDB lia esses valores sob outros nomes, e o PostgreSQL **aceita um parâmetro de prefixo desconhecido sem reclamar**: o comando funcionava, o valor era guardado, e nada o lia. **Esta era a pior forma de incompatibilidade possível**, porque o índice já aceitava a sintaxe de criação da outra extensão — o usuário criava o índice com sucesso, ajustava a precisão com sucesso, e media outra coisa. O sintoma não é erro: é um gráfico de precisão contra velocidade **plano**, indistinguível de "esse parâmetro não importa para estes dados". Os dois nomes agora valem, aparecem na lista de parâmetros do banco, e há regra documentada para quando os dois forem usados na mesma sessão: **o nome próprio do TheoDB vence**, escolhido por ser a única ordem que não altera o comportamento de quem já usa o produto. **Uma consequência precisa ser dita, e ela tem duas formas — medidas, não presumidas.** Um valor fora da faixa aceita, como `SET hnsw.ef_search = 99999`, hoje passa em silêncio. Depois da mudança: se o comando vier **depois** da primeira consulta vetorial da sessão, ele é **rejeitado com erro** e o valor anterior permanece; se vier **antes** — que é o mais comum, porque aplicações costumam configurar no início da conexão — o banco emite um **aviso** no momento em que carrega a extensão e **volta ao valor padrão**. Nos dois casos o valor sempre foi inválido e nunca teve efeito; o que muda é que agora o usuário fica sabendo. **Valores válidos configurados antes da primeira consulta são preservados** — verificado. (#B-034)

- **Comparar e ordenar vetores voltou a funcionar — cinco padrões comuns de aplicação deixaram de falhar.** O tipo `vector` tinha os três operadores de distância, mas nenhum de igualdade ou ordenação. Na prática isso quebrava `WHERE embedding = …`, `SELECT DISTINCT embedding`, `GROUP BY embedding`, `ORDER BY embedding` e chave única sobre a coluna — só a busca por similaridade funcionava. Como a mensagem vinha do PostgreSQL e não citava o TheoDB, **quem migrava de outra extensão não tinha como saber que a causa era a troca de implementação**, e a documentação prometia migração sem mudança de código. A semântica adotada é idêntica à da extensão de origem, verificada no código-fonte dela e não de memória: compara elemento a elemento e, em caso de empate no trecho comum, o vetor mais curto vem antes. Duas alternativas foram descartadas por motivo registrado — ordenar primeiro por número de dimensões produziria ordem **diferente** da original, consertando uma incompatibilidade ao criar outra; e igualdade por aproximação quebraria a propriedade que o índice exige para funcionar, fazendo buscas deixarem de encontrar linhas existentes, sem erro visível. A busca por similaridade não muda, e isso deixou de ser afirmação para virar verificação: há um teste que confere o plano de execução e reprova se a consulta de similaridade parar de usar o índice próprio — o risco de acrescentar operadores de comparação não é erro visível, é a consulta silenciosamente escolher um caminho mais lento. (#B-033)

- _(interno, sem efeito para quem usa o banco)_ **As verificações novas da superfície instalada nasceram com três defeitos e foram corrigidas antes de valerem como prova.** (1) Uma delas colapsava "a consulta falhou" e "não encontrei nada" na mesma resposta, e por isso acusou como ausente uma função que estava instalada — o mesmo engolir-erro-em-silêncio que o projeto proíbe, cometido dentro da verificação escrita para exigir rigor. Só não virou diagnóstico errado porque outra verificação a contradisse. (2) Outra conferia como o catálogo do PostgreSQL **escreve** o nome de um procedimento, e não a propriedade que importa: ser procedimento é o que permite confirmar cada lote durante uma importação grande, e uma conversão descuidada para função tornaria a importação tudo-ou-nada sem erro visível. Agora a propriedade é asserida diretamente. (3) A consulta comparava e lia colunas de catálogo sem conversão de tipo, o que a fazia falhar sempre — o defeito que o item (1) mantinha invisível, e que apareceu com mensagem exata no instante em que o erro parou de ser engolido. (4) Uma quarta verificação, esta da igualdade de vetores, afirmava esperar uma falha usando a forma comum de checar erro — que não funciona neste ambiente, porque um erro do banco interrompe a execução em vez de retornar um valor. A verificação reprovou **com o produto correto**, exibindo no registro exatamente a violação que ela pretendia provar. O padrão certo já era usado sete vezes no mesmo arquivo. Precisou de **duas** correções: a primeira trocou a forma de asserir, a segunda descobriu — lendo o código da biblioteca de testes, em vez de supor — que a comparação da mensagem é por igualdade exata e não por conter. O índice passou a ser nomeado explicitamente, para que o texto do erro fique sob controle da própria verificação em vez de depender da convenção automática de nomes do banco. As três correções tiveram a mesma causa: supor o comportamento de uma ferramenta em vez de ler o que ela faz. (#B-030, #B-033)

- _(interno, sem efeito para quem usa o banco)_ A verificação automática do caminho de atualização da extensão parou de declarar sucesso quando um dos seus quatro cenários não chegou a rodar. Ela imprimia "todos os cenários passaram" quatro linhas depois de avisar que o teste de idempotência havia sido **pulado** — o aviso dizia, com todas as letras, que aquilo não era um pass. A causa era um caminho de arquivo montado à mão, sem o número da versão do PostgreSQL, de modo que o arquivo procurado nunca existia. Agora o caminho vem da própria ferramenta do banco, e uma execução com cenário pulado termina como **incompleta**, com código de saída de falha. Vale registrar por que isso importa mais que o conserto: essa verificação foi criada justamente depois de dois resultados falsos anteriores, para que não houvesse um terceiro — e este era o terceiro. (#B-028)

- **O diagnóstico de busca vetorial deixa de responder zero quando o índice não serve a consulta.** `theodb.explain_scan` sempre mede com o operador de similaridade por cosseno, mas o tipo de índice criado por padrão responde a outra métrica (distância euclidiana) — e nesse caso a consulta caía numa varredura completa e o diagnóstico devolvia **zero páginas lidas**, número indistinguível de "usou o índice e não precisou ler nada". Agora ele **recusa com erro explícito**, dizendo qual índice recriar e como. Um diagnóstico que reporta silenciosamente os números de uma varredura completa sob o nome do índice é pior do que um que se recusa a responder. (#B-021)

- **O diagnóstico de busca vetorial passa a enxergar índices criados pela sintaxe do pgvector.** `theodb.explain_scan` respondia `(no theodb_hnsw index on this table)` para tabelas que **têm** índice, sempre que ele tivesse sido criado com `USING hnsw` — a forma que aplicações escritas para o pgvector usam, e a única que o produto de RAG do próprio time usa. A causa: o diagnóstico procurava o índice pelo *nome* do método de acesso, e a sintaxe compatível registra um segundo nome apontando para o mesmo código nosso. Agora ele procura pelo **código que serve o índice**, o que cobre as duas sintaxes — e seguirá cobrindo qualquer apelido futuro. Quem diagnostica latência de busca vetorial numa aplicação pgvector-compatível deixa de receber uma resposta que parecia dizer "não há índice". (#B-021)

- _(interno, sem efeito para quem usa o banco)_ Removido o último resíduo do índice experimental SymQG: uma opção de índice (`degree_bound`) que constava da tabela de leitura mas **nunca chegou a ser registrada**, e portanto sempre foi rejeitada — verificado no binário entregue, `WITH (degree_bound = 32)` responde `unrecognized parameter`. Como nunca foi aceita, nenhum índice existente pode tê-la gravada, e removê-la não afeta backup nem restauração. (#B-026)

- _(interno, sem efeito para quem usa o banco)_ A imagem de build do projeto passa a trazer as duas ferramentas de verificação de qualidade do código, que faltavam nela — quem tentasse rodar as verificações fora da integração contínua recebia um erro de ferramenta ausente facilmente confundido com "o código reprovou". No caminho, foi corrigido um descompasso de versão que tornava o conserto inócuo: a imagem declarava uma versão do compilador e o projeto exigia outra, e a do projeto vence — de modo que as ferramentas seriam instaladas na versão errada. Efeito colateral do descompasso, não notado até agora: a atualização de compilador feita em uma versão anterior nunca chegou a valer. (#B-025)

- _(interno, sem efeito para quem usa o banco)_ A execução de testes na esteira deixa de poder colidir com sobras de execuções canceladas: o contêiner passa a ter nome único por execução, o que elimina a possibilidade de conflito em vez de apenas limpá-lo depois. Sobras antigas também passam a ser varridas. (#B-027)

- _(interno, sem efeito para quem usa o banco)_ A medição que compara a rotina vetorial otimizada (SIMD) com a versão simples parou de acusar falsa lentidão. Ela media cada uma **uma única vez e sempre na mesma ordem**, então qualquer variação da máquina entre as duas medições era atribuída ao código. Num notebook isso acontece de verdade: instruções AVX reduzem a frequência do processador, e o efeito cresce conforme a máquina esquenta ao longo da suíte — medido, o resultado piorava de forma consistente com a carga acumulada (passava sozinho, 0,78× depois dos outros 439 testes, 0,66× com mais carga ao lado). A medição passa a **alternar** as duas versões por cinco rodadas e comparar **medianas**, de modo que o aquecimento afete as duas igualmente. **A tolerância não foi afrouxada** — o que melhorou foi a qualidade da medição que ela julga. (#B-023)

### Added
- **A superfície instalada passou a ser verificada por teste automático.** Três verificações novas, que rodam com a suíte normal do projeto, respondem a perguntas que antes dependiam de um script externo já removido: *todo objeto público existe depois de instalar?*, *nenhuma função que faz chamada de rede está liberada para todos os papéis do banco?* e *os invólucros públicos são conferidos no momento da criação?*. A segunda importa mais do que parece: a superfície de IA faz chamada HTTP a partir do servidor, e uma atualização que perdesse uma revogação de permissão liberaria isso para qualquer papel — antes, a checagem existia como fotografia de esquema que alguém precisava comparar à mão. (#B-030)

- Documento de reconstrução do harness de medição, em `.claude/knowledge-base/benchmarks-reconstruction-spec.md`. Descreve o que existia em `benchmarks/` com detalhe suficiente para reconstruir: o contrato público de cada módulo, a superfície de linha de comando, a integração com a esteira de integração contínua e a ordem sugerida de reconstrução. O núcleo do documento são **23 invariantes de integridade de medição** — cada um correspondendo a um defeito real já descoberto pelo projeto (recall medido por limiar de distância e não por sobreposição de identificadores; o oráculo exato enxergando a mesma precisão que o banco armazena; o parâmetro de partição do índice derivado do tamanho real do corpus; significância estatística por teste pareado). Estão registrados porque reimplementar sem eles produz números plausíveis e errados, e essa falha é silenciosa. (#B-029)

### Removed
- **Removido o diretório `benchmarks/` por completo — 268 arquivos.** Com ele saem o harness de recall@k, a suíte de testes de medição (65 arquivos), os roteiros de reprodução por marco e os artefatos brutos já commitados. Saíram junto `scripts/m143-removal-validate.sh` e `.gitsafety.yml`. **As consequências ficam registradas, porque são reais:** (a) os 20 marcos com medição publicada em `wiki/benchmarks/` perdem o caminho de reprodução na árvore de trabalho — os números continuam válidos e o código continua recuperável pelo histórico git, o que se perde é o acesso imediato; (b) o mandato de *medir antes de afirmar*, que o `CLAUDE.md` define como pré-requisito de qualquer alegação de performance, fica sem implementação enquanto isto durar; (c) a esteira de integração contínua referencia o diretório em quatro pontos e **falhará na próxima promoção para `develop`** até ser ajustada ou o diretório reconstruído; (d) sem `.gitsafety.yml`, o verificador de segredos volta a acusar credencial de rede interna como achado — falso-positivo que aquela configuração existia para calar. A recomendação técnica registrada era restaurar e depois medir o que faltava; a remoção foi decisão explícita do owner (2026-08-12) após a ressalva. Nada é definitivo: `git restore -- benchmarks` recupera tudo. (#B-029)

- **Removido o diretório `scripts/` por completo — 17 arquivos — e com ele a rede de verificação que protege a atualização de versão.** Esta remoção é de natureza diferente da anterior: a de `benchmarks/` tirou o que *comprova* as alegações do projeto; esta tira o que *verifica que o produto sai correto*. Três fluxos da esteira perdem dez chamadas diretas, e cada uma era um portão: o teste de fumaça executado por **todos** os quatro trabalhos que constroem a imagem; as três verificações do caminho de migração; o **portão de desvio de esquema**, que compara a superfície SQL antes e depois de uma mudança; e a verificação de segurança de SQL com as asserções do servidor ligadas. Saem também duas peças estruturais que a esteira não chama, mas o produto depende: o **gerador dos scripts de atualização da extensão** — os arquivos que ele produz estão versionados e trazem no cabeçalho "GERADO (…) NÃO editar à mão", de modo que o caminho documentado para produzir a atualização da próxima versão deixa de existir — e o **oráculo da cadeia de atualização**, consertado dois commits atrás (#B-028) precisamente porque já havia declarado "todos passaram" com um cenário pulado, duas vezes. **A consequência que mais pesa, dita sem rodeio:** a esteira não passa apenas a falhar por arquivo ausente; ela perde a capacidade de **reprovar** uma atualização quebrada, porque o oráculo que reprovaria foi removido. Para um banco de dados, atualização quebrada é a falha mais cara que existe — quem atualiza perde dado ou fica com a extensão inutilizável, e descobre em produção. A recomendação técnica registrada era restaurar e tratar o assunto em separado; a remoção foi decisão explícita do owner (2026-08-12) após a ressalva. Nada é definitivo: `git restore -- scripts` recupera tudo. (#B-029)

- **Instalar o TheoDB passou a ser um comando só: `CREATE EXTENSION theodb_rs`.** A extensão guarda-chuva `theodb` deixou de existir e sua superfície inteira — geração de texto, linguagem natural, registro de modelos, importação de vetores e a camada analítica — foi absorvida pela extensão principal. Antes eram duas extensões escrevendo objetos **nos mesmos schemas**: quem desinstalasse uma ficava com metade da API `ai` de pé. A divisão era resíduo de uma migração que terminou (a reescrita das funções para Rust), não um desenho. **Ganho concreto para quem usa:** `ai.generate` e `ai.summarize` deixam de ser funções de ligação tardia e voltam a ser conferidas no momento da criação — um erro de assinatura passa a aparecer na instalação, não na primeira chamada do usuário. O shim de compatibilidade `vector` **permanece separado de propósito**: o nome é o contrato que ferramentas como drizzle, alembic e prisma emitem no boot. (#B-030)

- **Removidas as cadeias de atualização de versão — 10.283 linhas.** Eram 13 arquivos que migravam instalações existentes entre versões, com 86% de conteúdo repetido entre si. Foram removidos porque não há instalação em campo a migrar: a imagem do produto nunca chegou a ser publicada — verificado, as dez últimas execuções da publicação falharam, a mais recente hoje. O gerador desses arquivos e o portão que os exigia já haviam sido removidos, de modo que a cadeia não podia sequer ser estendida. **A troca fica registrada:** a cadeia é do tipo em que um elo não criado hoje não pode ser criado depois; se surgir instalação em campo, a primeira versão pós-mudança passa a ser a base. (#B-031)

- _(interno, sem efeito para quem usa o banco)_ Removidos dois corpos-fonte do SQL da extensão que **não continham uma única instrução**: 32 linhas de comentário que sobraram quando as funções de embedding e de busca híbrida foram reescritas em Rust (M17/M19). Ambos ainda afirmavam criar os schemas `theodb` e `ai` — que na verdade são criados pela extensão Rust —, de modo que um leitor procurando onde os schemas nascem era mandado para o lugar errado. O conteúdo útil deles (o contrato das variáveis de sessão `theodb.embedding_*`) já está documentado em `wiki/guides/sql-embeddings.md`, em `wiki/guides/self-host-quickstart.md` e no próprio código Rust. (#B-030)

### Changed
- _(interno, sem efeito para quem usa o banco)_ **A auditoria de qualidade ganhou o analisador de código que lhe faltava.** Sem ele, a verificação que confere se o código referencia símbolos inexistentes não rodava, e a auditoria era rebaixada. Uma versão precisou ser fixada: a biblioteca de gramáticas ainda usa uma interface que o analisador mudou na versão seguinte, e sem o pino o import funciona mas a análise estoura — o que a auditoria reportaria como "ferramenta ausente", escondendo que a causa era versão flutuante, não ausência. (#B-033)

- _(interno, sem efeito para quem usa o banco)_ **A auditoria de qualidade do código passou a de fato auditar.** Ela estava configurada com **todas as linguagens comentadas**, e a consequência não era "nenhuma verificação": era a auditoria devolver **aprovado sem ter examinado nada** — visualmente idêntico a uma aprovação real. É a terceira vez que este ciclo encontra a mesma classe de defeito, depois da verificação de atualização que declarava sucesso com cenário pulado e da imagem de teste que compilava código antigo. Rust foi habilitado e a ferramenta que faltava foi instalada na imagem, em vez de a lacuna ser dispensada por decisão registrada. Resultado medido: aprovado, com Rust auditado e nenhum achado. (#B-030)

- _(interno, sem efeito para quem usa o banco)_ **O shim de compatibilidade virou uma unidade autocontida, e o diretório `sql/` de topo deixou de existir.** O arquivo de declaração da extensão ficava na raiz e o script de instalação dela num diretório `sql/` de topo — nome genérico que fazia sentido quando aquele diretório abrigava duas extensões e oito arquivos-fonte, e que virou incoerência quando sobrou **um arquivo só**, de uma extensão cuja declaração estava noutro lugar. Os dois passaram a viver juntos em `vector/`. Verificado com o banco em execução: instalação limpa sem erro, e o comando que aplicações pgvector emitem no boot continua funcionando num banco novo. **Corrigidas junto 20 citações em comentários do código** que apontavam para os arquivos SQL nos endereços antigos; seis delas referenciavam dois arquivos que **já não existem** — e essas passaram a citar o comando que recupera o conteúdo do histórico, verificado como funcional. Uma citação que não resolve é pior que a ausência dela: manda o leitor procurar onde não há. (#B-030)

- _(interno, sem efeito para quem usa o banco)_ **A imagem de construção foi separada em duas, e a de teste deixou de conter código-fonte.** Antes, a imagem usada para rodar a suíte trazia o código copiado dentro dela. Montar o repositório por cima não substituía essa cópia — aninhava —, e a compilação usava a versão antiga: medido em 2026-08-12, isso produziu **440 testes verdes que não testaram o código sob mudança**, visualmente idênticos a um resultado legítimo. A imagem de teste agora não tem fonte alguma, de modo que uma montagem errada para com "arquivo de projeto não encontrado" em vez de compilar outra coisa em silêncio — a falha deixa de ser possível, em vez de ser detectada. Dois desperdícios saem junto: o job de teste pagava uma compilação completa em modo release que **nunca reaproveita** (perfil e opções diferentes não compartilham artefato), e instalava um pacote pela rede a cada execução, o que fazia a suíte depender de um repositório externo estar no ar para conseguir rodar. A imagem de teste ficou em 2,49 GB contra 4,7 GB da anterior. (#B-030)

- _(interno, sem efeito para quem usa o banco)_ **A ordem de carga do SQL da extensão passa a ter uma definição só.** A lista de corpos-fonte concatenados para gerar o script de instalação existia enumerada à mão em **dois** lugares — o `Makefile` e o `Dockerfile` —, e os dois produzem artefatos diferentes (a imagem de produção usa um; uma imagem de teste usa o outro). Acrescentar um corpo novo exigia lembrar dos dois: esquecer o `Dockerfile` embarcava uma imagem sem a superfície nova, **sem erro nenhum**. As duas pontas passam a derivar da mesma regra — o prefixo numérico do nome do arquivo, que já era a convenção. Os comandos de instalação também deixam de enumerar cada script de atualização, o que exigia editar a lista a cada versão. Verificado: o script de instalação gerado continua com as mesmas 471 linhas de código, byte a byte. (#B-030)

- _(interno, sem efeito para quem usa o banco)_ O cabeçalho gravado no script de instalação gerado declarava três dependências — pgvector, pgvectorscale e plpython3u — e **nenhuma das três é verdadeira** desde M69/M70 (ADR-0029) e M19. Passa a declarar a dependência real, `theodb_rs`. Era comentário, não quebrava nada; mentia para quem lesse o arquivo gerado. (#B-030)

- _(interno, sem efeito para quem usa o banco)_ A esteira de integração contínua deixou de rodar a cada envio num pull request e passa a rodar **uma vez por promoção** — nos merges para `develop` e `main`. Motivo: a máquina que executa a esteira é única e atende um job por vez, e cada envio disparava a esteira inteira; o custo passou a ser alto demais para o retorno. Oito fluxos foram ajustados, e todos continuam podendo ser executados sob demanda quando valer a pena. **A troca é real e fica registrada:** sem verificação no pull request, um defeito só aparece depois de já estar em `develop`, e quem descobre é a próxima pessoa a integrar. A proteção não desaparece — muda de lugar, e passa a depender de rodar as verificações localmente antes de promover; o comando exato está no comentário de cada fluxo.

## [0.159.0] - 2026-08-11

> **Nota de versionamento — por que MINOR e não MAJOR.** Esta versão contém duas seções `Removed` (o
> access method `theodb_symqg` e a árvore `docs/`), e a regra de derivação do `cycle-release` deriva
> MAJOR de um `Removed` não-vazio — o que daria `v1.0.0`. Cortada como MINOR por decisão do owner
> (2026-08-11), por duas razões que se somam: (a) [SemVer §4](https://semver.org/) estabelece que em
> `0.y.z` nada é declarado estável, então remoções não forçam major — a regra do projeto não distingue
> `0.x`, e essa é uma lacuna dela; (b) `v1.0.0` violaria o hard cap #2 do `dogfood-golden-rule`
> (`anchor_not_running`): o cenário-âncora `theo-rag-sobre-theodb` está `planned`, não `running`, e o
> README declara explicitamente que o projeto não é production-ready. Cortar 1.0 aqui seria alegar por
> acidente de regra o que o projeto se recusa a alegar por evidência.

### Added
- A suíte de testes passa a rodar na integração contínua a cada push que toca a extensão. Até aqui nenhum job da esteira a executava — foi assim que 20 falhas, uma delas um defeito de recall, ficaram invisíveis por meses. O gate reprova quando o número de falhas **aumenta**, não quando é maior que zero. (#B-013)

- Backlog B-013 (a suíte de testes precisa rodar no CI — hoje depende de alguém lembrar) e B-014 (`bm25_search` aceita um termo por chamada, e nenhuma consulta real de usuário é um termo só). (#B-013 #B-014)

- Posicionamento do produto declarado e assinado: cinco eixos de atratividade, cada um com a medição que o sustenta (ADR 0060). Diz também o que o TheoDB **não** promete — superar o ScaNN/AlloyDB em throughput vetorial, que a própria medição do projeto declara inalcançável para uma extensão PostgreSQL permissiva. (#B-002)

- Backlog B-002..B-010 — um item por pilar mais o item do objetivo, todos justificados por medição própria: atratividade medida (B-002), teto de build vetorial (B-003), qualidade lexical contra BEIR (B-004), significância da fusão híbrida (B-005), ClickBench completo (B-006), grafo sem baseline (B-007), escala do lakehouse (B-008), robustez do egress de IA (B-009), uso real (B-010). (#M186)

- Busca lexical BM25 disponível na instalação padrão, e medida em dois corpora públicos com julgamento humano: entrega qualidade de recuperação superior à do `ts_rank_cd` nativo do Postgres em ambos, com significância estatística — 2,08× no BEIR SciFact e 1,35× no BEIR NFCorpus. A margem varia com o domínio. Até aqui o pilar existia, era medido e era robusto, mas ficava fora do binário entregue — quem instalava o TheoDB não tinha `bm25_build`/`bm25_search`. Passa a acompanhar a extensão sem nenhuma flag de compilação. As funções de andaime do spike (`lexical_spike_*`) continuam fora da superfície pública, por opção. (#M186)

- **theodb:** **cenário-âncora de dogfood declarado** (M175) — `theo-rag-sobre-theodb`: o produto de RAG do
  próprio ecossistema passa a usar o TheoDB como vector store em vez do pgvector. A escolha é medida, não
  arbitrária: `theo-rag/package.json` e `theo-memory` declaravam `docker compose up -d pgvector`, ou seja, **os
  produtos de IA do time usavam a extensão de um terceiro e não o banco que o time constrói**. O golden rule
  saiu do estado de template — o § 1 ainda tinha `<anchor-slug>` como placeholder — e o manifesto foi criado
  com status **`planned`**, o único que a evidência sustenta. O drop-in foi **verificado executando** a
  sequência exata do theo-rag contra a imagem (extensão, `vector(1536)`, INSERT de 500 vetores, índice HNSW por
  sintaxe pgvector, busca por `<=>`), e o PR está aberto em usetheoai/theo-rag#206. **Nenhuma evidência de
  dogfood foi fabricada:** as execuções desta sessão são carga sintética de benchmark, que o § 1 exclui em texto
- **theodb:** backlog **B-001** — `cargo pgrx test` não executa neste repositório: o binário de teste morre com
  `symbol lookup error: CurrentMemoryContext`. **310 testes e nenhum roda localmente** pelo caminho
  documentado. Confirmado pré-existente rodando um teste que existe desde antes (`sq8`) com todas as mudanças
  da sessão revertidas. O item registra **três hipóteses já testadas e refutadas** — flag de link em
  `.cargo/config.toml` (muda o sintoma de link-time para runtime, não resolve), desalinhamento de `cfg` no
  bootstrap `pg_test`, e `crate-type` sem `"lib"` — para que ninguém pague de novo os ciclos de build que elas
  custaram
- **theodb:** perfilada também a **busca** do SymQG, fechando o regime onde os 2,6–3,9× do e2 foram medidos. O
  contraste com o HNSW é o achado: naquele, **nenhuma função do `theodb_rs.so` passa de 1,3%** — o custo está
  espalhado e majoritariamente fora do nosso código —, enquanto no SymQG **`gather_symqg_candidates` concentra
  18,23%**. Build e busca têm gargalos **distintos**, ambos em funções específicas do caminho SymQG. Achado
  colateral de planner: **sem `enable_seqscan=off` e `enable_sort=off`, o planner escolhe `Sort` + `Seq Scan`
  em vez do índice vetorial** neste dataset — foi a causa de duas tentativas anteriores de perfilar a busca não
  produzirem amostra. Se o mesmo vale para o `theodb_hnsw` em produção, é problema maior que o SymQG, e **não
  foi investigado**
- **theodb:** **o gargalo do SymQG tem nome** — `ambuild_symqg`, **39,27% do build**, medido com símbolos
  resolvidos em CPU dedicada. O HNSW **não tem função análoga**: o build dele se distribui entre o kernel de
  distância e o paralelismo. E os dois **compartilham o mesmo kernel SIMD** (`l2_sq`, `l2_dist_from_bytes`), de
  modo que o SymQG **não é mais lento por calcular distância pior** — é mais lento por gastar 39% em outra
  coisa. Isso não o torna promovível, mas muda o M176 de "problema estrutural do ambiente" para "39% do custo
  numa função nomeada", que é alvo e não muro. **Correção de método junto:** o perfil anterior atribuiu a falha
  de símbolos a "release sem debuginfo" — falso, o `.so` tem 86.191 símbolos estáticos; o `perf` falhava por
  resolução de caminho no namespace do container, resolvido com `--pid=host` e um `docker cp`
- **theodb:** **perfilado o SymQG para responder *por que* é mais lento** — pergunta que os três artefatos do e2
  deixaram aberta (nenhum menciona mecanismo, gargalo ou profiling). O resultado **contradiz a hipótese
  registrada na feature**, que atribuía a lentidão ao "imposto de página, WAL e MVCC": se fosse isso, o SymQG
  passaria **mais** tempo no kernel, e ele passa **menos** — 18,7% contra 27,9% do HNSW, com 76,5% em
  `theodb_rs.so` contra 66,0%. Os símbolos de kernel em ambos são de **escalonador**, sem nenhum de I/O, página
  ou WAL. **É compute-bound: o custo mora no nosso código.** Isso não o torna promovível, mas muda a natureza
  do trabalho que uma promoção exigiria — de estrutural para algorítmico. Limite grande e declarado: os
  símbolos do `.so` não resolvem (release sem debuginfo), então a atribuição é por objeto, não por função

### Changed
- _(interno, sem efeito para quem usa o banco)_ Cinco testes que reprovavam **porque o produto estava certo** voltaram a passar, sem que nenhuma verificação fosse afrouxada. Dois comparavam a mensagem de erro por um pedaço, enquanto a ferramenta de teste exige o texto inteiro — passaram a declarar a mensagem completa, o que de quebra transforma o texto do erro em contrato. Três apontavam para um endereço local que a proteção contra requisições internas (SSRF) recusa antes mesmo de tentar conectar: dois passaram a usar um domínio reservado que não existe, e o do disjuntor de circuito passou a exercitar a máquina de estados diretamente, sem rede nenhuma. **A proteção não foi tocada** — relaxá-la para fazer teste passar seria abrir exatamente o buraco que ela fecha. (#B-016 #B-022)

- _(interno, sem efeito para quem usa o banco)_ A esteira de testes deixa de reprovar por lixo de execuções canceladas. Quando um envio novo cancelava o anterior, sobrava um contêiner com nome fixo na máquina de build, e a execução seguinte falhava por conflito de nome — **antes de rodar um único teste**, mas aparecendo como "os testes falharam". Um ciclo inteiro foi gasto investigando uma regressão que não existia. (#B-027)

- _(interno, sem efeito para quem usa o banco)_ Os dois verificadores automáticos de qualidade do código voltaram a passar. Ambos estavam reprovando havia tempo — o de formatação desde antes desta versão, o de análise estática por causa dos resíduos descritos em Fixed — e um verificador que reprova sempre deixa de barrar o próximo defeito de verdade. O código foi reformatado pela ferramenta oficial do projeto (7 arquivos, apenas quebras de linha, nenhuma mudança de comportamento). (#B-025 #B-026)

- Hook de validação de comandos ficou mais rápido: ~6 processos por chamada de ferramenta em vez de ~50, sem mudança de comportamento
- **theodb:** o **M176 (SymQG) muda de natureza** com a medição do M184. Ele fora escrito assumindo "código
  mantido sem consumidor" — dívida interna barata. A medição desmente: o `theodb_symqg` está **registrado como
  access method no binário default**, com opclass exposto, e um usuário pode escrever `USING theodb_symqg`
  hoje e receber um índice **3,5× mais lento no build** e **2,6–3,9× mais lento na busca**. Não é dívida
  interna, é **superfície pública cara**, e nada no produto avisa — "não recomendado" no `feature_status` não
  é "indisponível". O leque de saídas ganhou uma que não estava previsto e é a mais barata: **esconder atrás
  de feature flag**, o mesmo tratamento do lexical, que preserva o código para estudo sem expor quem instala

### Fixed
- Limpos **quatro resíduos** deixados pela retirada do índice experimental SymQG. O que mais importa: uma função que nunca era chamada e que, se algum dia fosse, poderia derrubar a conexão do banco — ela checava se um ponteiro era nulo, não fazia nada com o resultado, e usava o ponteiro na linha seguinte. As duas funções vizinhas do mesmo arquivo tratam o caso corretamente; só nela o tratamento se perdeu. Nenhum caminho do produto a alcançava, então **não houve impacto para quem usa o banco**. Os outros três eram documentação órfã de código que não existe mais e um trecho que o verificador recusava por estilo (o código estava correto). Todos juntos mantinham o verificador de qualidade reprovando — e um verificador que reprova sempre deixa de barrar o próximo defeito de verdade. (#B-026)

- Três testes do pilar colunar acusavam que a poda por zone-map não acontecia, e a medição mostrou o contrário: **a poda sempre funcionou** — o cenário de teste é que nunca chegava a criar o que ela poda. Os dados ficavam num buffer de escrita que só vira bloco durável ao encerrar a transação, e o teste nunca encerrava a sua. Corrigido no cenário, não na verificação: o teste passa a produzir blocos reais e volta a medir o que diz medir. Medido na mesma sessão: dentro da transação, 0 blocos examinados; com o buffer dimensionado como em uso real, 31 blocos e 30 podados. (#B-015)

- O diagnóstico de busca vetorial voltou a reportar o custo real das consultas. `theodb.explain_scan`, `theodb.scan_stats` e as estatísticas acumuladas mostravam **zero páginas lidas e zero candidatos** para o tipo de índice mais usado do produto, mesmo com a consulta usando o índice e devolvendo as linhas certas — e o recomendador automático de `ef_search` decidia sobre esses zeros. A causa: a busca ganhou um segundo caminho interno (retomada de varredura, M118) que virou o padrão e não reportava o que media. Medido antes e depois na mesma consulta: onde antes vinha `0`, agora vêm as páginas e os candidatos reais. Quem usa `explain_scan` para diagnosticar latência passa a ver números; quem depende do autotune passa a recebê-lo alimentado com dados verdadeiros. (#B-015)

- **A imagem do TheoDB está publicada** em `ghcr.io/usetheoai/theo-db:latest` e `:0.140.0` — pela primeira vez. `docker pull`, o primeiro comando do README, funciona. A causa de nunca ter sido publicada: o workflow de publicação apontava para uma organização que não existe. O workflow de publicação apontava para uma organização que não existe, então **toda** execução falhava — no push de `develop` e na tag `v0.158.0` — e por isso `docker pull ghcr.io/usetheoai/theo-db:latest`, o primeiro comando do README, respondia `manifest unknown`. (#B-010)

- Metade das falhas da suíte era classificação errada de teste, não defeito do banco: 10 dos 20 testes vermelhos passaram a verde sem uma linha de mudança no comportamento do produto. Os testes da busca lexical BM25 voltam a ser executados. Eles existiam e falhavam desde sempre por um detalhe de registro — o módulo não declarava seu schema, então o PostgreSQL nunca recebia as funções de teste e a chamada falhava com "função não existe". O pilar tinha sido promovido à instalação padrão com seus 6 testes vermelhos. (#B-012)

- **Retratação da entrada anterior:** o vector-join do HNSW **não tinha defeito**. Ao medir, a perda vinha da premissa dos próprios testes — eles pediam ao índice uma garantia que o algoritmo não dá (o parâmetro de busca limita a exploração do grafo, não o tamanho do resultado). Os testes foram corrigidos com o alvo intacto e passam. Nenhuma mudança no comportamento do banco. (#B-011)

- A suíte de testes do projeto volta a executar localmente — os unitários puros **e** os que precisam de um servidor PostgreSQL. O binário de teste morria no carregamento com `symbol lookup error: CurrentMemoryContext`, antes do primeiro teste — **nenhum dos 439 rodava**. Eram três bloqueios empilhados — link, carregamento e instalação da extensão — e resolver um só não fazia nenhum teste rodar. A receita completa está no runbook `wiki/runbooks/rodar-a-suite-de-testes.md`. Quem contribui com o TheoDB deixa de depender do CI para descobrir uma regressão em lógica pura. (#B-001)

- Índices vetoriais voltam a ser escolhidos pelo planner. A 20 mil linhas × `vector(1536)` o TheoDB respondia em 182 ms fazendo varredura sequencial, tendo um índice capaz de responder em 2 ms — porque o custo de partida do índice era estimado 10× acima do real e ele nunca vencia a comparação. Quem criava um índice recebia um índice que não era usado, sem nenhum erro que denunciasse. Vale para `theodb_hnsw` e `theodb_ivfflat`. (#M175)

- **theodb:** o módulo `parquet.rs` (lakehouse) tinha **zero testes próprios** — lacuna medida pelo M184 contra
  uma nota de maturidade que exigia "testado". Acrescentados **6 testes**: as duas representações de string do
  Arrow que o leitor de Parquet devolve conforme o schema do arquivo (tratar só uma quebraria em arquivo real,
  e silenciosamente), o **caso negativo** de tipo errado exigindo erro tipado com o tipo real na mensagem —
  porque um `Vec` vazio ali viraria "categoria sem linhas" rio abaixo, dado errado em vez de erro —, a borda de
  array vazio (Parquet com zero linhas é válido), o pool de memória do DataFusion não-degenerado, e o **REVOKE
  de PUBLIC** das três primitivas de I/O de arquivo, que vive num `extension_sql!` e sumiria em silêncio num
  refactor. **Compile-verificados** no builder construído a partir do próprio `Dockerfile` (`cargo check --features pg_test`
  limpo). **Não executados**: `cargo pgrx test` falha no link do binário de teste standalone com `undefined
  symbol: FreeErrorData / FlushErrorState / pfree`, vindos do `pgrx-pg-sys` — e a falha é **pré-existente**,
  reproduzida com um teste que existe desde antes (`sq8`) e com as mudanças desta sessão revertidas. É um
  defeito de infraestrutura de teste do repositório, não destes testes
- **theodb:** **retratada uma das três divergências que o M184 alegou.** Eu havia concluído que os opclasses
  documentados não existiam, porque `USING theodb_hnsw (v vector_l2_ops)` falha. O comando falha, a conclusão
  não: existem **dois caminhos coerentes** — nativo (`theodb_hnsw` + `theodb_hnsw_l2_ops`) e compatibilidade
  pgvector via shim do ADR 0058 (`hnsw` + `vector_l2_ops`) — e eu cruzei um com o outro sem instalar o shim
  antes de concluir. Verificado: com `CREATE EXTENSION vector`, a sintaxe pgvector **funciona**. Sobra um
  resíduo real e bem menor: a mensagem de erro não sugere o opclass correto nem menciona o shim, o que é
  melhoria de diagnóstico

### Added
- **theodb:** **M184 — crash-safety EXECUTADA**, fechando o quarto eixo: `kill -9` no postmaster, restart e
  comparação de checksum. **Verde nos quatro pilares** — vetorial preserva 5 000 linhas e o índice continua
  servindo busca pós-recovery; colunar, heap de controle e grafo com checksum idêntico. Resultado colateral: o
  `theodb_columnar` produz **md5 byte-idêntico ao heap** sobre 50 000 registros, antes e depois do crash — a
  propriedade que o m128 mediu em 43 queries do ClickBench, reconfirmada por outro caminho. Os 17 scripts de
  `isolation/` seguem sem rodar (exigem instalação pgrx do fonte), mas a propriedade que protegem foi
  verificada por execução direta
- **theodb:** **M184 — três divergências medidas** entre a tabela de maturidade e a execução, em CPU dedicada.
  **(1)** O `theodb_symqg` está registrado no binário default (nota 1 dizia "experimental") **e** tem build
  **3,5× mais lento** que o HNSW no mesmo dataset — somado aos 2,6–3,9× de busca do e2, é superfície pública
  cara em dois eixos, o que agrava o M176. **(2)** O `parquet.rs` tem **zero testes próprios** contra uma nota
  3 que exigia "testado" — crash-safety está coberta por script de isolation, cobertura unitária não.
  **(3)** Os opclasses documentados **não existem** com o nome do pgvector: `vector_l2_ops` falha, o real é
  `theodb_hnsw_l2_ops`, e quem seguir a nomenclatura herdada recebe erro. Confirmações registradas com a mesma
  clareza: vetorial (60 testes, busca 3,7 ms) sustenta a nota 4; colunar (87 testes) sustenta a 3; lexical
  (zero funções expostas) sustenta a 2
- **theodb:** primeira entrega do **M184**, e ela já achou uma divergência da tabela de maturidade: consultando
  o catálogo do PostgreSQL no binário default (CPU dedicada, imagem 0.139.0), o **`theodb_symqg` está
  registrado como access method** — contra a nota 1 que o classificara como "experimental, não recomendado
  como default" a partir de leitura de documentação. *Não recomendado* não é *ausente*, e a nota foi atribuída
  como se fosse: um usuário pode escrever `USING theodb_symqg` hoje e receber o índice medido como 2,6–3,9×
  mais lento. Isso **agrava o M176**. Confirmado no mesmo método que o pilar lexical expõe **zero** funções
  (nota 2 correta). O artefato declara que cobre **um** eixo da régua — presença — e que performance,
  qualidade e crash-safety seguem sem verificação
- **theodb:** criado o **`BACKLOG.md`** deste repositório, destravando o `/backlog-item` — que havia recusado
  quatro invocações nesta sessão porque o registro não existia, e cujo bootstrap (`/backlog-init`) está
  bloqueado no umbrella por falta de `CHANGELOG.md` lá. **A divergência do `cycle-backlog.md` está declarada no
  próprio arquivo**: a regra desenha um registro único no umbrella e recusa backlog per-repo, mas
  `knowledge-base-location.md` estabelece que cada projeto é dono dos próprios artefatos de ciclo, e o custo da
  escolha — um achado que cruze repositórios não tem onde morar — fica dito em vez de silenciado. O `domain` do
  schema passa a ser **o pilar** (vetorial, colunar, lexical, ai-surface, engine-pgrx…), cada um roteado para o
  especialista correspondente. Semeado **sem itens**, por desenho
- **theodb:** **M184 — medir cada pilar com rigor e apurar quanto a avaliação de maturidade errou.** A tabela
  0–5 por pilar registrada em 2026-08-07, que motivou os M170–M176, **não era medição**: saiu de ler
  `feature_status`, contar testes por diretório e aplicar uma régua proposta na hora, sem executar nada. O que
  justifica reabri-la é concreto — o M177 derrubou **cinco conclusões medidas por defeito de instrumento**, e
  se a medição direta errou cinco vezes, uma avaliação por leitura de documentação merece desconfiança maior.
  **A entrega não é uma nota nova, é a divergência**: saber onde leitura e execução discordam calibra toda
  avaliação futura feita do mesmo jeito. O DoD exige a divergência nomeada (contra viés de confirmação, já que
  quem mede escreveu a tabela), perfil onde o número surpreender, e declarar por pilar o que **continuou sem
  medir**
- **theodb:** **M182 — VectorDBBench: rodar, perfilar e publicar internamente, nesta ordem**, e **M183 — o claim
  público**, que é gate separado. Nenhuma das quatro bancadas já usadas cobre o caminho de embedding ponta a
  ponta (ClickBench é colunar, ANN-Benchmarks é o índice isolado, BEIR é qualidade, CH-benCHmark é HTAP) — foi
  a lacuna que obrigou esta sessão a escrever seis instrumentos. **A ordem é o milestone**: nenhum número sai
  antes do perfil que explica por que ele é aquele, porque esta sessão produziu cinco retratações, todas por
  defeito de instrumento, e um número de bancada pública sem perfil seria a sexta — com o nome do projeto
  junto. O M183 separa o que depende de terceiro: `public-copy.md` § 4 exige **reprodução independente** para
  comparação pública, a metade que o `m45` declara em aberto desde julho
- **theodb:** **medido o footgun do ADR 0007** com o PostgreSQL no laço — a última pergunta aberta desta área,
  e a primeira medição do milestone que instala a extensão e observa `pg_stat_activity` sob carga. O mecanismo é
  real: os backends ficam ativos durante a chamada e crescem com a concorrência. **Mas o custo não está onde o
  ADR temia** — a 16 clientes concorrentes o pico foi de **8 backends, 8% de `max_connections`**, enquanto a p50
  subiu 4,2× e a p99 **6,3×**. O gargalo é **latência, não esgotamento de conexões**, o que muda a conclusão
  sobre a fila assíncrona: ela resolveria o backend preso, que não é o que dói primeiro — o que dói é a
  capacidade do servidor, endereçada por batching, limite e réplicas. Registrado também que o guard de SSRF do
  M134 **recusou o endpoint em runtime**, evidência de execução de que a defesa opera fora do teste
- **theodb:** o relatório do benchmark passou a registrar, **por consulta**, quantos blocos o caminho streamado
  de fato consumiu. Sem esse número, uma consulta atendida pela rota de recuo — com o consumo de memória
  proporcional ao volume que este milestone existe para eliminar — aparecia no relatório exatamente igual a uma
  atendida pelo caminho novo, porque o sinal anterior vinha do plano de execução e não distingue as duas (#M169)
- **theodb:** oráculo que exercita a **execução** do recuo, e não só a decisão de recuar. Ele força o caminho
  novo a falhar por recurso de forma controlada e então verifica três coisas: que o recuo realmente aconteceu,
  que o resultado é idêntico ao do caminho de referência, e que um resultado deliberadamente corrompido seria
  reprovado — sem esse terceiro gate, "nenhuma divergência" não é evidência de nada (#M169)

### Changed
- **docs:** a documentação do projeto passou a viver em `wiki/`, como um acervo de 282 conceitos em
  [Open Knowledge Format](https://github.com/google/open-knowledge-format). Toda referência do repositório
  — README, ROADMAP, `CLAUDE.md`, scripts, workflows de CI e comentários de código — foi reapontada para lá
- **benchmarks:** os artefatos brutos que os runners escrevem (JSON, CSV, logs, flamegraphs) passaram a ir
  para `benchmarks/artifacts/`, junto do código que os gera, em vez de ficarem misturados à documentação.
  Quem roda um benchmark com `--out` padrão encontra o resultado no diretório novo

### Removed
- **theodb:** **o access method `theodb_symqg` foi removido por completo** (M176 fechado). Ele estava
  registrado no binário default sem feature flag, de modo que um usuário podia escrever `USING theodb_symqg` e
  receber — **sem aviso** — um índice medido como **3,5× mais lento no build** e **2,6–3,9× mais lento na
  busca** a recall casado. O perfil localizou os gargalos (`ambuild_symqg` com 39% do build,
  `gather_symqg_candidates` com 18% da busca), e a decisão foi remover em vez de esconder atrás de flag.
  Saíram 1 810 linhas: o spike, o runner de benchmark, o layout de página, os dois scripts de medição, o
  handler e o opclass, o **kernel FastScan de 1 bit** (`build_sign_lut16`/`sign_estimate_block`, exclusivos
  deste caminho — verificado), GUCs, reloption e testes. **Verificado por compilação:** `cargo pgrx install --release --features pg18` completa sem erro depois da
  remoção — e a primeira tentativa **falhou com dois `E0061`**, que a varredura de referências e a checagem de
  chaves não pegariam: a remoção por linha havia deixado uma chamada de `define_bool_guc` e uma de
  `add_int_reloption` com o esqueleto sem os argumentos do meio. **Migração:** o projeto está em pré-release e não há instalação em campo, mas o upgrade `1.3.0 → 1.4.0` existe
  porque o `schema-drift-gate` bloqueia mudança de superfície sem ele e porque a cadeia é append-only — o elo
  que falta hoje não pode ser criado depois. Ele dropa o AM e **falha alto** se existir índice criado com ele. Os artefatos de medição do e2 e do M184 **permanecem no acervo**: o conhecimento pago fica

- **docs:** a árvore `docs/` foi **removida do repositório**. Seus 264 documentos estão convertidos em
  `wiki/`, e os 517 arquivos permanecem recuperáveis no histórico git, no commit `f7c7b93`. Os **253
  artefatos brutos** de medição que viviam sob `docs/benchmarks/` saíram junto e passam a existir apenas
  no histórico — quem depender de um deles precisa recuperá-lo com `git show f7c7b93:docs/benchmarks/…`

### Added
- **theodb:** decidido no grill com o owner como o servidor de embeddings é entregue: **imagem própria**
  `theodb-embed` com manifesto CloudNativePG de exemplo, e **pesos dentro da imagem** (~520 MB) para que ela
  suba sem rede, funcione air-gapped e prenda a versão do modelo à tag. Sobre o comportamento em sobrecarga, a
  decisão foi **medir antes de escolher**: A/B entre implementar batching e fila no servidor atual e adotar um
  servidor de inferência de CPU pronto. Fundamentação: o padrão de produção (Triton) combina dynamic batching
  com queue policy — de modo que o batching do M178 e o limite de sobrecarga **são a mesma peça**. Registrado
  também que `pgpool` **não se aplica** a esta camada: ele enfileira conexões PostgreSQL, e o gargalo medido
  está no servidor HTTP
- **theodb:** **M180 — empacotar o servidor de embeddings na distribuição**, o item que separa a capacidade
  entregue do produto entregue. O modelo local funciona e o usuário não o recebe: o servidor **não está na
  imagem** (verificado no `Dockerfile`) e o guia manda rodar `python benchmarks/servers/embedding_server.py` —
  uma bancada de teste documentada como caminho de produção. **Destrava o M178**, cujo DoD fala em batching
  "no servidor de embeddings" — enquanto ele for ferramenta de bancada, otimizá-lo melhora algo que ninguém
  recebe. O milestone chega com a evidência já medida para cada opção de entrega, incluindo o default
  recomendado por medição (`nomic-embed-text-v1`) e a restrição do CloudNativePG
- **theodb:** investigado o **M179** antes de implementar, e ele é maior do que o DoD supunha: leitura do
  código do `minreq` 2.14.1 mostra que **não há keep-alive nem pool** — zero ocorrências no `connection.rs`/
  `request.rs`, e `send()` consome `self` chamando `TcpStream::connect` a cada envio. Não existe opção a
  ligar. Sobram trocar de cliente HTTP (dependência nova → gate D1 + `/deps-audit` + `.so` maior + **reescrever
  o guard de SSRF, que hoje espelha o parser do `minreq` de propósito**) ou reinventar um cliente, que a escada
  de parcimônia barra. O item passa a exigir **ADR de dependência antes da implementação**
- **theodb:** **fechado o gate de qualidade da fase 1 do M177** — o último eixo sem número. Medidos oito
  modelos multilíngues permissivos contra um corpus **pt-BR real** (os 250 conceitos da wiki, com a descrição
  do frontmatter como consulta e o conceito como alvo — relevância derivada por construção, não inventada). O
  resultado corrige a leitura anterior: o modelo mais rápido **não basta**. `MiniLM` é 8,5× mais rápido que
  `multilingual-e5-large` e perde **37% de MRR** (0,4946 contra 0,7906); qualidade e latência andam juntas, e
  cinco dos oito candidatos são **dominados** — existe outro mais rápido *e* melhor. Recomendação medida:
  `nomic-embed-text-v1` (Apache-2.0, 53,7 ms, MRR 0,6749) como default, `e5-large` quando qualidade domina. Os
  valores absolutos são otimistas por construção do corpus; o que vale é a ordem entre modelos
- **theodb:** **M178 — dynamic batching no caminho de consulta** e **M179 — reuso de conexão no cliente HTTP**,
  os dois itens do M177 cujo ganho já está medido e que faltam implementar. O batching vale **~2,2×** (7,82 ms
  por texto isolado contra 3,54 ms em lote de 8) e a ingestão já o colhe pelo worker do vectorizer — só a
  consulta não. O reuso de conexão vale ~0,6 ms em loopback mas **~32 ms contra provedor remoto**, porque
  `minreq` abre conexão nova a cada chamada e paga o handshake TLS toda vez. Os outros dois eixos discutidos
  (escolha de modelo e limite de aceitação de conexão) **não viraram milestone**: ainda são medição, não
  implementação, e já são itens abertos da fase 1 do M177
- **theodb:** medido que **processar em lote vale ~2,2×** no caminho de embedding — 7,82 ms por texto isolado
  contra 3,54 ms em lote de 8, saturando por volta de 8–16 textos (medido em droplet de CPU dedicada,
  reproduzido em duas coletas). Metade desse ganho **já está capturada**: o worker do vectorizer agrupa a fila
  numa única chamada, então a ingestão já colhe o benefício. O caminho de **consulta não** — ele embeda um texto
  por vez, de forma síncrona —, e é ali que a oportunidade segue aberta: agrupar pedidos concorrentes numa
  janela curta melhora latência **e** throughput ao mesmo tempo, porque só troca espera de fila por espera de
  janela. Não implementado; o ganho acima é do modelo, não do servidor

- **theodb:** **retratado o ganho de 9,4× da configuração de thread** — ele **não existe em CPU dedicada**
  (1,00× a um cliente, 0,98× a oito, medido em droplet `c-8` ocioso). Era contenção da máquina compartilhada
  do começo ao fim: sob disputa, `OMP_NUM_THREADS=1` deixa o ONNX com uma thread competindo com dez
  containers, enquanto sem o limite ele abre várias e arranca fatia maior do escalonador. Numa máquina ociosa
  não há disputa e uma thread já basta. A "maior alavanca medida do milestone" **não é alavanca**
- **docs:** registrada a restrição de plataforma — o banco roda sob **CloudNativePG**. Isso não muda número
  medido nenhum, mas muda o que eles significam: em Kubernetes o `limits.memory` do pod é rígido, e um pod que
  o estoura é **morto**, não paginado. Os 1,7 GB por processo de um modelo multilíngue somados ao pod do banco
  fariam um pico de concorrência derrubar **o banco**, não o embedding. A rota do serviço separado passa a ser
  o padrão nativo da plataforma, com o teto de ~195 rps por instância virando parâmetro de `replicas`
- **theodb:** **retratado o colapso sob sobrecarga** — re-executado num droplet de CPU dedicada, o servidor de
  embeddings **não colapsa nem vaza memória**: o throughput fica plano em ~195 rps de 8 a 128 clientes (contra a
  inversão para 26% do pico medida localmente) e o RSS cresce **16 MB em vez de 6,8 GB**. A explosão de memória
  era efeito de **segunda ordem** da contenção de CPU: sob disputa cada pedido demora 3× mais, mais conexões
  ficam abertas ao mesmo tempo, e mais arenas do ONNX são alocadas de uma vez. Sobrevive apenas o limite de
  aceitação de conexão — 13% de recusa a 128 clientes mesmo no dedicado. O achado original segue abaixo,
  preservado, porque foi ele que motivou o teste em máquina limpa
- **theodb:** ~~**o servidor de embeddings colapsa sob sobrecarga**~~ (RETRATADO acima), achado do primeiro teste de stress do
  milestone. Além do pico de 65 rps (32 clientes), mais carga produz **menos** trabalho: 26,5 rps a 64 clientes
  e **17,1 rps a 128, com 19,7% de recusa de conexão** — congestion collapse, não saturação. Pior, o RSS do
  servidor cresce de 161 MB para **6 932 MB e não é devolvido** quando a carga cessa: com um cliente e a
  latência já normalizada em 19 ms, os 7 GB continuavam retidos até o processo ser encerrado. O mecanismo foi
  verificado no processo vivo — `ThreadingHTTPServer` cria uma thread por conexão sem limite e o ONNX Runtime
  aloca arena por thread. A capacidade útil é ~32 clientes concorrentes, e a correção é **limitar** a
  concorrência (pool fixo, semáforo, backlog explícito), o oposto do instinto de aumentá-la para escalar
- **theodb:** **fixado o teto de qualquer otimização de transporte no caminho de embed.** A decomposição por
  camada mede que servidor Python + HTTP + TCP custam juntos **0,849 ms sobre 16,649 ms — 5,1%**; os outros
  94,9% são o ONNX Runtime, que é nativo e executaria igual sob qualquer arquitetura. Isso limita **todas** as
  técnicas de eliminação de transporte à mesma fatia: Unix socket, protocolo binário, memória compartilhada e
  até embarcar o modelo no backend disputam 5,1%. Medido também o Unix domain socket contra o TCP loopback,
  pareado e sem modelo no laço: ele vence com **p=0,0000** e ganha **33 microssegundos** — 0,2% do pedido. É o
  caso didático de significância estatística sem relevância prática, e reportar apenas o p seria verdadeiro e
  enganoso. Para comparação, as alavancas fora do transporte já medidas neste milestone valem 9,4× (configuração
  de thread) e 3,7× (escolha de modelo), contra 1,05× de eliminar todo o transporte

- **theodb:** **perfilado o servidor de embeddings, e a maior alavanca de performance é uma flag** — não uma
  reescrita. O flamegraph (`py-spy`, 4 091 amostras sob 8 clientes) mostra que **98,6% do tempo de requisição
  é `InferenceSession.run`**; HTTP, JSON e tokenização somam ~1,4%, de modo que **não há overhead a cortar no
  servidor**. Em compensação, rodá-lo sem `OMP_NUM_THREADS=1` dá **9,4× de throughput** (3,5 → 32,9 rps a um
  cliente) e derruba a p50 de 298 ms para 30,3 ms, com saturação em ~61 rps em vez de ~20. O ganho foi
  verificado como **gratuito em qualidade**: os vetores das duas configurações são byte-idênticos (diferença
  máxima 0,000e+00, cosseno 1,0000000000), o que era obrigatório checar porque paralelismo altera a ordem de
  redução em ponto flutuante
- **theodb:** **medido o gargalo que o ADR 0007 registrou em junho e deixou explicitamente para depois** — o
  caminho de embed sob concorrência. O servidor de embeddings **satura em ~20 rps**: de 8 para 16 clientes o
  throughput sobe 5% enquanto a **p99 quadruplica** (469 → 1 887 ms), atingindo 5,7× de escala num ideal de
  16×. Reportar a média esconderia o achado — a média a 16 clientes é 615 ms, mas um em cada cem usuários
  espera 1,9 s. Medido também que `minreq` **abre conexão nova a cada chamada**: ~0,6 ms sobre loopback, mas
  **~32 ms contra provedor remoto** (handshake TCP+TLS medido), de modo que embedar 10 mil linhas gasta ~320 s
  só abrindo conexões. A referência SOTA para o bloqueio (`pg_net`, Apache-2.0, BackgroundWorker + libcurl)
  resolve **apenas a ingestão**: a consulta precisa do vetor na mesma query e não pode ser assíncrona
- **theodb:** primeira medição do **M177 fase 1** — os dois lados da troca que decide a extensão de
  embeddings local. O hop HTTP local custa **15,55 ms** numa chamada unitária (27,2% do tempo, p=0,0000) e
  **deixa de ser mensurável em lote de 8** (p=0,1457): é custo fixo por requisição. Do outro lado, ter o
  modelo residente custa **1,7 GB por processo** para o melhor multilíngue permissivo (`multilingual-e5-large`,
  MIT) e mais **1,8 GB** para o rerank (`bge-reranker-base`, MIT). **A rota de uma cópia por backend fica
  refutada em multilíngue** — ~3,5 GB por processo, com dois backends esgotando a memória livre da máquina
  de medição. A rota do BackgroundWorker sobrevive. Achado de licença: o único rerank multilíngue do
  catálogo é cc-by-nc-4.0 e está barrado por D1, assim como o `jina-embeddings-v3`. O artefato declara que
  **não** mede qualidade — nenhum nDCG foi coletado, e esse item da fase 1 segue aberto
- **theodb:** **M177 — embeddings locais como extensão instalável, marcado P0** pelo owner. O que recebe
  prioridade máxima é o **gate de medição**, não a construção: marcar como P0 um caminho cuja viabilidade
  ninguém mediu abandonaria o measurement-first que trava o roadmap. A Fase 1 compara modelos open-source no
  nosso corpus (entrega útil sozinha — define o modelo default recomendado), mede quanto do tempo ponta a ponta
  é hop local versus inferência, e levanta o custo de empacotar centenas de MB de pesos. A falsificação está
  escrita antes de medir: hop abaixo de 5% do total não justifica a Fase 2 por latência. A Fase 2 — a extensão
  opt-in — é condicional, e seu primeiro item é o ADR sobre **onde o modelo fica residente**, que o prior art
  aponta como o custo que decide
- **docs:** levantamento de prior art sobre **gerar embeddings localmente como extensão PostgreSQL instalável**
  (`wiki/references/embedding-local-como-extensao-2026-08.md`). A arquitetura já existe sob licença permissiva —
  `pg_gembed` (Apache 2.0) é quase idêntica à proposta, e o PostgresML (MIT) mostra o formato em escala maior —,
  de modo que a licença não é o obstáculo. O custo que decide é outro, e duas fontes independentes convergem
  nele: **memória por conexão**. O `pg_gembed` cacheia o modelo por backend, e o paper NeurStore constrói
  deduplicação, compressão e buffers compartilhados justamente para conter esse overhead. O conceito declara no
  topo que é prior art e **não** evidência
- **docs:** roadmap **v7 — maturidade ≥ 4 em todos os pilares** (M170–M176), derivado de um levantamento do
  estado de cada pilar contra a wiki e o código: vetorial 4; colunar, IA, lakehouse, híbrida e grafo 3; lexical 2;
  SymQG 1. Cobre a cobertura de erro na fronteira de rede da superfície `ai.*` (onde `embed` e `rerank` têm um
  teste cada), a escrita Parquet além de escalares com head-to-head externo, e o **dogfood sustentado** — que hoje
  é o teto de todos os pilares, já que nenhum pode passar de 4 sem evidência de uso real. Três dos sete perguntam
  se o pilar **merece** subir e admitem aposentadoria como conclusão: a híbrida (cujo ganho sobre o vetorial puro
  não é estatisticamente significativo), o BM25 próprio (fora do binário default) e o SymQG (medido mais lento).
  O colunar já tinha o seu — é o M169

### Changed
- **docs:** o roadmap passou a ser **um só arquivo**. `ROADMAP-v3.md` e `ROADMAP-v5.md` viviam em paralelo como
  drafts estratégicos e mostravam **100% dos seus milestones em aberto** enquanto todos os 13 estavam concluídos
  no roadmap ativo. O caso do v5 era o perigoso: ele se apresentava como "fechar o pilar P0 que segue parcial",
  quando esse pilar havia sido fechado com veredito de que a superioridade vetorial sobre o ScaNN é
  **não-alcançável** — quem o lesse reabriria uma aposta já refutada por medição. O conteúdo estratégico dos dois
  foi absorvido no `ROADMAP.md` e os arquivos removidos; permanecem no histórico git

### Fixed
- **theodb:** **retratada a atribuição do custo do hop no M177.** O artefato publicava 15,55 ms como "custo do
  hop"; uma decomposição com o mesmo stack e **sem modelo nenhum** — só o canal — mede o transporte real em
  **1,37 ms** (vetor 384d) e **1,72 ms** (1024d), sobre 0,68 ms de HTTP+JSON vazio. Os ~14 ms restantes eram
  diferença de implementação entre os dois braços, não o canal: o experimento comparava caminhos que diferiam
  em mais de uma variável, e o total foi creditado à única que estava sob investigação. Com o transporte a
  ~1,4 ms, o hop é ~2–3% do pipeline de consulta — **abaixo do limiar de 5%** que o milestone declarou antes
  de medir, de modo que o critério de falsificação foi cruzado. A seção original fica preservada no conceito,
  sob aviso, porque foi ela que fundamentou a primeira leitura
- **docs:** o roadmap ativo declarava, na abertura da seção do v5, um estado que a própria medição já havia
  superado — "M60 aberto", "o pilar segue parcial". A seção passou a abrir com o veredito medido do M73
  (paridade de recall alcançada; QPS multi-cliente competitivo-a-superior vs pgvector; superioridade sobre
  ScaNN/AlloyDB não-alcançável) e com o aviso de que três levers já deram honest-negative, de modo que propor um
  quarto exige dado novo
- **docs:** dois milestones entregues nunca tiveram registro no roadmap, e foram adicionados retroativamente. O
  **M168** publicou verdict e é citado pelo M169 e pelo CHANGELOG (o número que ele moveu: maior bloco do `q23`,
  de 772 MiB para 17,9 MiB). O **M45** é a **régua de paridade** do pilar vetorial — citado dez vezes no próprio
  roadmap e no `CLAUDE.md` — e é o milestone que retratou o claim de "1,7–2,8× mais rápido que pgvector" medido
  em dados degenerados. As demais lacunas de numeração (M23, M24, M27–M29) são intencionais: milestones removidos
  do escopo em 2026-07-03, mantidos riscados para que nenhuma referência histórica mude de alvo
- **docs:** o roadmap não explicava sua própria convenção de status, e um `grep` de caixinhas sobre ele devolvia
  um número enganoso: o status vive no header do milestone, enquanto os 346 bullets `[ ]` restantes são a
  especificação escrita no momento do plano. A convenção passou a estar declarada no topo, junto de onde a
  evidência de conclusão realmente mora (a wiki) e da cobertura medida — 130 dos 149 concluídos
- **docs:** o `CLAUDE.md` declarava `develop` como branch de trabalho, contradizendo a Regra 4 e o
  `git-safety.md`, segundo os quais toda mudança **nasce** em `workspace` e `develop` apenas integra — seguir o
  texto do projeto levaria a commitar exatamente onde o hook local bloqueia. Corrigido, com o fluxo completo
  explícito. Na mesma seção, a contagem de ADRs dizia 12 quando são **60**
- **docs:** a seção do acervo de referências primárias no `CLAUDE.md` manda ler papers e repositórios em
  `knowledge-base/references/` e **citar `arquivo:linha`**, e esse diretório não existe no disco. A seção passou
  a declarar o estado verificado e a instruir o caminho honesto enquanto não for repovoado, em vez de exigir uma
  citação impossível
- **docs:** a seção do acervo de conhecimento no `CLAUDE.md` descrevia um bundle, um contrato, um validador
  e dois hooks que **não existem** — anunciava que a sessão era bloqueada ao publicar um número sem o
  conceito correspondente, quando nenhum hook jamais conheceu o formato. Um documento normativo que promete
  um gate inexistente é pior que não ter gate, porque quem lê confia nele. A seção passou a apontar o acervo
  real (`wiki/`, 282 conceitos), o validador que de fato existe e roda, e a listar explicitamente o que
  **não** é verificado por mecanismo nenhum
- **docs:** o guia de migração mínima voltou a mostrar os comandos literais que recriam os índices
  (`USING hnsw`, `USING ivfflat`) durante a restauração. Sem eles, o leitor não sabia quais índices
  sobrevivem ao `pg_restore` sem inspecionar o dump
- **docs:** a página do analítico colunar passou a registrar a ressalva que faltava — um `SELECT` normal
  numa tabela colunar decodifica **todas** as colunas e é medido em torno de 16 a 26 vezes mais lento que
  a mesma leitura numa tabela heap. O ganho de projeção existe apenas no caminho acelerado, e omitir isso
  levava a esperar velocidade onde o desenho entrega compressão
- **docs:** nove links para benchmarks arquivados apontavam para a raiz do diretório enquanto os arquivos
  estavam sob `archive/` — quebrados desde antes desta mudança, agora resolvidos
- **theodb:** o spill do agregado/top-k passou a viver sob **`pgsql_tmp` do próprio cluster**, com teto derivado
  de `temp_file_limit`, em vez do diretório temporário do sistema. Isso importa porque o M169 tornou o spill um
  caminho de produção (antes dele a pool era dimensionada pelo lote decodificado e nada derramava): no default
  do DataFusion o `temp_file_limit` não se aplicava, `log_temp_files` e `pg_stat_database.temp_bytes` ficavam
  cegos, arquivos vazavam quando o processo morre de forma anormal, e numa instalação com `/tmp` em tmpfs
  "derramar para disco para limitar a memória" na verdade **alocava mais RAM** — desfazendo o próprio teto que a
  operação promete. Falha ao resolver o diretório cai no comportamento anterior em vez de perder o spill (#M169)
- **theodb:** a guarda que decide se o caminho antigo pode atender uma consulta passou a usar o **maior total por
  coluna** em vez da soma entre colunas. O teto de endereçamento do Arrow é de um array — ou seja, de uma coluna
  —, então somar duas colunas de texto grandes recusava o recuo em consultas que o caminho antigo atenderia sem
  problema, transformando em erro duro exatamente o caso que a rede existe para cobrir (#M169)
- **theodb:** o contador que prova "esta consulta passou pelo caminho streamado" passou a ser **zerado na
  entrada das duas rotas**, antes da decisão. Ele cobria só um dos quatro caminhos que terminam no caminho
  antigo, e faltava justamente no top-k — o consumidor primário dele. Nos casos descobertos, um oráculo lia
  "passou pelo stream" sobre uma resposta vinda do caminho antigo, ou sobre as chamadas de uma tentativa que
  falhou (#M169)
- **theodb:** o verificador de byte-identidade passou a **reproduzir a sessão da corrida que ele prova** e a
  recusar a comparação quando o plano não tem o pushdown agregado. Faltava `SET theodb.enable_columnar_agg = on`
  (a GUC é default OFF), então ele compararia colunar vs heap por um caminho que o milestone não toca e
  estamparia "idêntico" com toda a razão e nenhuma relevância. O sintoma que denunciou foi o tempo — 4m45s
  contra 59,5 s da corrida-alvo —, não o resultado. `identical` agora fica indefinido quando não houve prova:
  "não provei" é estado distinto de "está certo" (#M169)
- **theodb:** o mesmo verificador passou a comparar por **ordem total** em vez de remover o `LIMIT`, e a abrir
  **uma conexão por consulta**. Remover o limite desempatava corretamente em consultas pequenas e pedia ~10⁸
  linhas na `GROUP BY WatchID, ClientIP`: 19,5 GB de anon-rss, backend morto pelo kernel e o cluster inteiro
  reinicializado. Acrescentar as colunas de saída como critérios de desempate posicionais resolve o empate sem
  inflar o resultado, e mantém a consulta na forma que o ClickBench define. A conexão por consulta existe porque
  a queda envenenou a sessão e transformou as duas consultas seguintes em `cursor already closed`, que o
  relatório exibia como "não roteou" — artefato lido como medição (#M169)
- **theodb:** o driver de medição de pico passou a ler o SQL por **redirecionamento** em vez de `-f <caminho>`.
  A checagem do bit `x` vale para cada componente resolvido no momento do open, e um caminho absoluto sob um
  diretório 700 falha onde o relativo (a partir do CWD já herdado) funciona — o erro acusa o arquivo, não o
  diretório, e manda o leitor conferir a permissão certa do alvo errado (#M169)
- **theodb:** o recuo do agregado streaming passou a cobrir também a **falha de spill em disco**, não só a pool
  estourada. Medido na corrida completa a 100M: `COUNT(DISTINCT …) GROUP BY …` (q08/q09 do ClickBench) saía de
  `ok` para erro. A causa não é defeito novo — a pool do caminho eager é dimensionada pelo batch decodificado, o
  que a 100M dava ~2,5 GB; removido o batch O(N), a pool passou a derivar de `work_mem`, o agregado começou a
  derramar, e o derrame falha porque o PostgreSQL pode segurar até `max_files_per_process` (1000) descritores
  dentro de um limite de 1024, deixando folga quase nula para arquivos abertos fora do gerenciador dele. O recuo
  continua condicionado à guarda de offsets, e erro genuíno do braço streaming **não** recua — recuar nele daria
  a resposta certa pelo caminho errado e esconderia o defeito (#M169)
- **theodb:** o recuo do agregado streaming para o caminho eager passou a **classificar o erro com `find_root()`**
  em vez de casar a variante nua. O DataFusion embrulha `ResourcesExhausted` em `Context(…)` num caminho vizinho,
  e a forma anterior deixaria o recuo de fora justamente quando ele é necessário — com esta GUC em `on` por
  default, isso viraria erro duro em consultas que a versão anterior servia (#M169)
- **theodb:** o recuo passou a **registrar uma linha de log incondicional**. Sem ela o usuário não tem sinal de
  que a consulta acabou de trocar de perfil de memória e de latência — e a medição de pico não distingue "o
  spill funcionou" de "a pool estourou e re-executamos em silêncio" (#M169)
- **theodb:** o contador de chunk-groups do streaming passou a ser **zerado no recuo**. Ele é a prova de que o
  braço `on` de fato usou o caminho novo; sem o reset, retinha as chamadas da tentativa que falhou e o gate lia
  "passou pelo stream" quando a resposta viera do eager — falso-verde dentro do próprio oráculo (#M169)
- **theodb:** o planejador do scan colunar passou a publicar, sob o flag de trace, o **tamanho do diretório do
  plano** (entradas por chunk-group × coluna, e os bytes que ocupam). É o termo O(N) que o streaming **não**
  remove: sem esse número, "decode O(chunk-group)" leva o leitor a concluir que o consumo total virou
  constante (#M169)
- **theodb:** agregados sobre tabelas colunares passaram a **consumir a relação um chunk-group por vez** em vez de
  decodificá-la inteira num único `RecordBatch` Arrow. É o que remove o `byte array offset overflow` a 100M: uma
  coluna de texto acima de 2 GiB não cabe nos offsets `i32` de um array Arrow único. Vale para os **dois** caminhos
  — escalar e agrupado —, porque das três instâncias medidas do defeito uma é escalar (q20) e duas são agrupadas
  (q33, q34). Nova GUC `theodb.enable_columnar_agg_stream` (default on) (#M169)
- **theodb:** a reconstrução do gêmeo heap de 100M passou a **provar que consegue ler o TSV antes de dropar
  qualquer coisa** — lendo 1 byte como o usuário que de fato lerá. A ordem anterior dropava e recriava a tabela
  primeiro, e o aborto por leitura deixava 0 linhas onde antes não havia tabela nenhuma; 0 linhas lê como "a
  carga perdeu linhas", que manda perseguir um bug de COPY inexistente. O caminho default saiu de `/root`
  (modo 700): um arquivo 644 é inalcançável quando um diretório do caminho não dá `x` (#M169)

### Added
- **theodb:** a varredura de cardinalidade do T3.2 passou a agrupar por **colunas materializadas** em vez de
  `GROUP BY <expressão>`. Módulo não está no whitelist de chave-expressão do admit (`DateTrunc`,
  `ExtractField`, `IntAddConst`, `Const`), então a forma anterior **declinava** e a medição teria reportado
  o pico do executor de linha do PostgreSQL como se fosse o do agregado colunar (#M169)
- **theodb:** o script de recarga passou a documentar que instalar o `.so` exige **rename atômico**, não `cp`.
  `cp` reescreve no mesmo inode e troca as páginas sob os processos que já o têm mapeado — na prática isso
  matou um background worker com SIGSEGV e derrubou o cluster; e o postmaster, ao tentar se recuperar, forka
  filhos que herdam o mapeamento corrompido, então só um `stop` + `start` limpo resolve (#M169)
- **theodb:** o acompanhamento da recarga deixou de chamar `pg_total_relation_size` na tabela em carga. A
  função pega lock de relação e o `ALTER TABLE … SET LOGGED` segura `AccessExclusiveLock`, então o monitor
  ficava **preso na fila do lock** — e a saída vazia se lê como "nada acontecendo", que é o oposto (#M169)
- **theodb:** as guardas de "box ociosa" passaram a ignorar backends `idle`. Um `psql` esquecido por uma
  conexão que caiu não consome CPU nem I/O, mas abortava a medição; `idle in transaction` continua contando,
  porque esse segura locks e snapshot e é justamente o que contamina o número (#M169)
- **theodb:** publicado o limite de cardinalidade do `GROUP BY`, **medido**: o pico de memória cresce
  linearmente com o número de grupos distintos (~92 B cada), e 2 milhões de grupos já consomem **95,4%** de
  uma pool de 192 MiB. É o termo que o consumo por chunk-group **não** reduz — e o que faz uma consulta de
  chave quase-única sobre 100M linhas estourar por estado, não por offsets (#M169)
- **theodb:** driver da medição de pico do `GROUP BY` que encoda a ordem obrigatória em vez de a confiar à
  memória de quem roda: guarda de box ociosa, restart **com** `THEODB_ADMIT_TRACE=1` (o trace resolve num
  `OnceLock` por backend, então só entra por restart do postmaster), medição, e `trap` que restaura o
  servidor **sem** o trace mesmo se a medição falhar no meio. Zero linhas de pico é tratado como falha —
  significa instrumento desligado, não pico baixo (#M169)
- **theodb:** gerador do delta entre as duas corridas de 100M, que **recusa publicar** quando as condições
  não são comparáveis: teto de tempo diferente, box diferente, corpus diferente, ou o **mesmo binário** dos
  dois lados. Separa os ganhos atribuíveis (falhavam COM roteamento agregado) dos não-atribuíveis, e nomeia
  as regressões — a linha que um resumo de "quantas a mais passam?" esconde. Ligado ao script do baseline
  por `COMPARE_TO=<label>`, e uma recusa de publicar falha a corrida em vez de virar silêncio (#M169)
- **theodb:** a atestação da box passou a ligar o pushdown antes de contar as linhas, como o guard da recarga
  já fazia — sem isso ela sozinha custava dezenas de minutos **antes** de a medição começar. A leitura do
  resultado passou a tomar a última linha, porque o `SET` imprime o próprio tag acima do número e lê-lo como
  valor reportaria "servidor inalcançável" (#M169)
- **theodb:** o guard de integridade da recarga do heap passou a ligar `theodb.enable_columnar_agg` antes do
  `count(*)`. **Medido:** 11,4 s com o pushdown contra >948 s sem ele sobre as mesmas 99.997.497 linhas — o
  backend lento fica a 99,9% de CPU com zero wait events, ou seja materialização linha a linha, não I/O. Um
  guard de 35 minutos é um guard que alguém pula. A captura usa `tail -1` porque o tag `SET` sai em linha
  própria, e `PGOPTIONS` não serve: `sudo` limpa o ambiente antes do psql (#M169)
- **theodb:** o caminho default do TSV de 100M saiu de `/root` em **todos** os scripts do M169 (baseline,
  atestação e recarga), não só naquele onde o defeito apareceu. `/root` é modo 700, e um caminho que
  atravessa 700 é inalcançável para qualquer processo não-root — por mais permissivo que o arquivo seja (#M169)
- **theodb:** o trace de pico da pool de streaming passou a se chamar `theodb_stream_pool` (era
  `theodb_topk_pool`). O M169 fez o agregado consumir a mesma função, então o rótulo antigo passaria a
  mentir: quem lesse `topk_pool` numa linha emitida por um `GROUP BY` concluiria a coisa errada sobre qual
  caminho consumiu a memória. Os artefatos do M168 mantêm a string antiga — eles registram o que aquele
  binário emitiu (#M169)
- **theodb:** o harness de cobertura de tipos (M163) rodou contra o caminho agregado streaming: **35/35 casos
  como esperado** com `positive_control diverged=2` — cada classe roteada é byte-idêntica e cada declínio é
  correto, em int2/int4/int8, float4/float8, bool, texto, temporais, colação nomeada, `IN`-list, `const_out`,
  group-expr e top-k. Artefato em `docs/benchmarks/m169-type-coverage.md` (#M169)
- **theodb:** gate de associatividade de `float8` para o agregado colunar — mede `sum`/`avg` **bit a bit**
  entre o caminho eager e o streaming sobre dado adversarial (`0.1` não-representável + `1e17` esparso), com
  controle positivo de 1 ULP. Consumir por chunk-group muda a ORDEM de acumulação, e adição IEEE-754 não é
  associativa; sem esta medição o milestone poderia trocar um defeito barulhento por um silencioso. **Medido:
  idêntico.** O ClickBench não responderia isso — todas as colunas de SUM/AVG dele são inteiras (#M169)
- **theodb:** baseline medido do ClickBench a 100M — **28 das 43 consultas completam** sob teto de 300 s, e o
  artefato separa as falhas pelo discriminador `agg_routed`: **4 no caminho agregado colunar** (q20, q32, q33,
  q34) contra 11 que nem entram nele. O alvo do milestone tem **3 instâncias** de `byte array offset overflow`
  (q20, q33, q34), não uma; a q32 roteia mas falha por pico de estado, que o streaming não corrige (#M169)
- **theodb:** o artefato do baseline agora declara, onde o número é lido, que o `19/43` do M162 **não é base de
  comparação válida** — as duas corridas rodaram em regimes de memória diferentes (corpus maior que a RAM vs
  corpus em page cache), e um delta entre elas misturaria o efeito do código com o efeito da máquina (#M169)
- **theodb:** script de reconstrução do gêmeo heap de 100M que **não toca** na tabela colunar já carregada,
  aborta se a box estiver ocupada ou se o colunar não estiver íntegro, e converte a tabela para permanente
  logo após a carga — deixá-la temporária a faria ser apagada pelo próximo crash, que é o cenário que a
  própria medição provoca (#M169)
- **theodb:** gerador do artefato do baseline, que **recusa emitir** quando a proveniência está incompleta —
  binário não identificável, binário trocado no meio da corrida, ou corrida que não alcançou todas as
  consultas; um relatório que parece completo e não é sobrevive muito depois do contexto (#M169)
- **theodb:** runner de baseline do ClickBench a 100M que mede **quantas consultas completam** sobre as
  tabelas já carregadas — com gate de não-vacuidade que recusa publicar um número quando a corrida não
  alcançou as 43, vocabulário de veredito que nunca chama de OOM o que não tem evidência de kernel, e
  verificação de que as GUCs pedidas realmente existem no servidor (#M169)
- **theodb:** gate executável de atestação da box de benchmark — falha quando a máquina não é a que o
  milestone declarou (CPU/RAM insuficientes, carga concorrente, `unattended-upgrades` ativo, contagem de
  linhas divergente do corpus, disco insuficiente), em vez de deixar os critérios como caixas manuais (#M169)
- **theodb:** varredura dos 173 reviews + 58 ADRs + 110 blueprints para a base OKF — +17 conceitos
  (incluindo dois bypasses de allowlist por regex, um parser-differential que levava ao metadata service,
  e o padrão do DoD cujo primeiro checkbox mata o milestone); bundle vai a 91 conceitos (#okf)
- **theodb:** varredura dos 139 artefatos de benchmark para a base OKF — +14 conceitos (3 invariantes de
  plataforma do porte PG18, o limite de escala a 100M, e a correção de um erro herdado de um ADR que
  comprimiu dois números em um); bundle vai a 76 conceitos (#okf)
- **theodb:** mineração dos transcripts do projeto irmão para a base OKF — 7 conceitos novos e 2 atualizados
  (`setsid` em ssh, fsync do diretório-pai, dados sintéticos degenerados, SBQ, pg_duckdb sobre heap, colação
  em MIN/MAX de texto, precisão do júri); bundle vai a 60 conceitos (#okf)
- **theodb:** auditoria de cobertura da base OKF — 7 lacunas reais fechadas (dogfood vs benchmark, teste que
  passa pela razão errada, fail-open por omissão, snapshot do bgworker, GUC de worker, wrapping de `sum(Int64)`,
  `scanrelid=0` sob pullup); bundle vai a 53 conceitos (#okf)
- **theodb:** contrato de leitura e atualização da base OKF — regra `okf-knowledge-base.md`, validador
  determinístico `check_okf.py` (4 invariantes estruturais) e hard gate no Stop que bloqueia número
  publicado sem conceito de medição correspondente (#okf)
- **theodb:** base de conhecimento operacional em Open Knowledge Format (OKF v0.1) — 46 conceitos
  navegáveis por agente cobrindo modos de falha, técnicas, invariantes de plataforma, medições e
  negativos honestos acumulados de M46 a M169 (#okf)
- Roadmap amendado: adicionado **M169 — Bugs de escala a 100M** (`/roadmap-feature scale-bugs-100m`). Recorte: só as falhas duras (`byte array offset overflow` do q20, OOM da corrida, confirmar q23); os três `statement_timeout` ficam fora por não serem defeitos. 1 bilhão declarado fora de escopo até 100M passar limpo (#219 não relacionado).
- **Top-k colunar com memória limitada:** consultas "as N primeiras linhas por uma coluna" sobre tabelas colunares
  deixaram de precisar de memória proporcional ao tamanho da tabela. Medido a 1 milhão de linhas × 105 colunas, o
  **maior bloco decodificado de uma vez** num `SELECT *` caiu de **772 MiB para 17,9 MiB (43×)** — abaixo do `work_mem` da sessão, e não mais acima
  dele (#215, #218). O consumo total do processo é maior que esse bloco e foi medido em parte: a retenção interna
  do ordenador ficou entre 0,83 e 2,41 MB. A tabela passa a ser decodificada em partes, uma de cada vez, em vez de inteira de uma vez.
  Resultado byte a byte idêntico ao anterior. No tempo, o `SELECT *` largo ficou **~13,6% mais rápido** depois que
  o cache aquece — juntando **seis coletas sobre seis binários**, o caminho novo venceu **72 de 72 comparações
  pareadas**, sem uma única exceção. Nas consultas de projeção estreita **a medição não resolve o tempo** — e isso
  passou a ser provado por experimento, não argumentado: o **mesmo binário**, medido em dois horários do mesmo dia,
  deu **−0,6% e +2,3%**. A variação que a máquina impõe entre duas medições do mesmo código é maior que qualquer
  efeito que essas consultas tenham. Elas economizam 22× a 32× de memória de qualquer forma. A medição foi feita
  numa máquina compartilhada, então o número merece replicação; o desenho pareado é o que o torna defensável
  apesar disso.
  Reversível com `theodb.enable_columnar_topk_stream = off`. Método, números por consulta e ressalvas
  em `docs/benchmarks/m168-streaming-topk-verdict.md`.

### Fixed
- **theodb:** o runner do baseline passou a declarar explicitamente o estado do streaming agregado em vez de
  confiar no default do servidor — o parâmetro nasce ligado, então o braço "antes" de uma comparação
  mediria o caminho novo e concluiria que a mudança não teve efeito (#M169)
- **theodb:** a atestação passou a amostrar o ambiente (carga, CPU, RAM) **antes** das verificações caras de
  dataset — lê-lo depois faria a medição refletir o trabalho da própria atestação e reprovar a máquina por
  uma carga que ela mesma criou (#M169)
- **theodb:** o texto de ajuda do `--quick` da atestação passou a declarar o custo como ordem de grandeza
  ("dezenas de minutos") em vez de um número preciso — as observações disponíveis tiveram janela
  sobreposta com outro processo, e o valor exato não é sustentado por elas (#M169)
- **theodb:** a atestação passou a ler corretamente uma unidade systemd **mascarada** — `systemctl
  is-enabled` sai com código 1 nesse estado, e tratá-lo como falha de execução reprovava o gate pelo
  motivo errado numa box corretamente configurada (#M169)
- **theodb:** segunda varredura do round 3 (categorias nunca auditadas) — BLOCKER de recall `0.033` sem
  artefato, regra de proveniência que nenhum script cumpre, e atribuição de desfecho a milestone em voo (#okf)
- **theodb:** round 3 de review da base OKF — 3 BLOCKER de correção não propagada pelo grafo, 2 HIGH em que o
  bundle prescrevia o defeito que documenta (topo de faixa; `rc != 0` para falha de canal em ssh), e o C6
  restrito a duas bases declaradas (#okf)
- **theodb:** re-review das correções OKF — 3 defeitos que as próprias correções introduziram (1 BLOCKER:
  colunas invertidas no pg_duckdb), 2 omissões de propagação, e o gate C6 que valida `resource:` (#okf)
- **theodb:** review adversarial de 5 agentes sobre a base OKF — 34 achados aplicados (4 BLOCKER de números
  fabricados ou conclusões invertidas), gate C5 (valor de `type` no conjunto fechado), dois gatilhos de
  leitura novos, e correção das origens herdadas em `CLAUDE.md` e no issue #221 (#okf)
- **Cancelar uma consulta colunar longa volta a funcionar, e a conexão sobrevive a isso.** Ao passar a decodificar
  por partes, o top-k colunar passou a segurar a leitura de todas as páginas dentro de uma janela em que o
  PostgreSQL não processa interrupções — na prática, um `Ctrl-C`, um `statement_timeout` ou um
  `pg_terminate_backend` seriam ignorados até o fim do scan. Agora o cancelamento é reconhecido **entre partes**,
  o trabalho interno é encerrado ordenadamente e só então o PostgreSQL levanta o cancelamento. Coberto
  por `benchmarks/m168_cancel_oracle.sql`, que cancela uma consulta de verdade e depois verifica que a sessão
  sobreviveu.
- **O recuo automático deixou de mascarar erros que não são de memória.** Ele foi escrito para um caso — o
  orçamento de memória não caber — mas capturava qualquer falha, incluindo erro de integridade de dados e o
  próprio cancelamento acima. Agora só o estouro de memória aciona o recuo; o resto é reportado.
- **`transaction_timeout` e queda de conexão passam a interromper uma consulta colunar longa.** O reconhecimento
  de cancelamento cobria só `statement_timeout`/`Ctrl-C` e `pg_terminate_backend`; um `SET transaction_timeout`
  sobre uma varredura longa era ignorado até o fim, e o mesmo valia para o cliente ter desaparecido. É a mesma
  falha da entrada acima, num conjunto mais estreito de gatilhos.

### Changed
- **theodb:** o gate de atestação distingue "a tabela não existe" de "não consegui consultar" e emite
  identificadores estáveis por falha, para que uma tolerância explícita (ex.: seguir sem o gêmeo heap) não
  engula silenciosamente uma falha diferente que por acaso menciona a mesma tabela (#M169)
- **theodb:** a atestação da box ganhou `--quick`, que pula as checagens de dataset (~40 min a 100M) e
  mantém as de contaminação — para o cabeçalho de fechamento de uma corrida read-only, que não pode ter
  mudado o dado; "não perguntei" é registrado como estado próprio, nunca confundido com "ausente" (#M169)
- **Recuo automático quando o caminho novo não cabe na memória:** se o top-k colunar por partes não couber no
  orçamento de memória da sessão, a consulta volta sozinha ao caminho anterior em vez de falhar. Quatro cenários foram medidos
  procurando um caso em que o caminho novo falhe e o anterior sirva, e **nenhum foi encontrado**; o oposto sim —
  numa projeção estreita com `LIMIT 400000` o caminho **novo serve e o anterior falha**. O recuo fica como defesa
  contra um caso não observado, e passa a ser registrado no log do servidor sempre que acontecer. Detalhes em
  `docs/benchmarks/m168-streaming-topk-verdict.md` § 3.5.
- **A regressão do M167 passou a ser gravada como artefato pelo coletor de benchmark.** O relatório afirmava
  `rc=0` para os dois oráculos do M167 numa tabela em que todas as outras linhas tinham artefato, mas o coletor
  nunca os executava — a linha era verificada por leitura de código, não por execução. Agora
  `benchmarks/m168_collect_all.sh` os roda e grava `m167-regression.log`.
- **Os números de desempenho do top-k colunar foram revisados para baixo durante a revisão, e a metodologia
  mudou.** Quem leu uma versão anterior desta entrada viu "~18% mais rápido" e "as consultas de projeção estreita
  ficam ~2% mais lentas (p = 0,014)". Os dois números não sobreviveram à auditoria: o primeiro vinha da coleta
  mais favorável de seis (o valor agregado das seis é **13,6%**), e o segundo tratava cada comparação como
  independente quando a unidade de replicação é a **coleta** — corrigido, o custo das projeções estreitas fica no
  limite da resolução da medição, com quatro das seis coletas mostrando custo e duas mostrando ganho. Nenhum
  número de **memória** mudou (43×/32×/22×/32× em seis coletas, idênticos ao dígito). Método, as sete versões
  desta seção e o que permanece não medido em `docs/benchmarks/m168-streaming-topk-verdict.md`.
- **Diagnóstico:** a auditoria automática de código morto não detecta métodos Rust sem chamador dentro do próprio
  crate, então "nenhum achado" no relatório dela significa "o detector não achou", não "não há". Quatro métodos e
  dois campos escritos durante o M168 e nunca usados foram encontrados por conferência manual e removidos. A
  limitação está registrada em `docs/benchmarks/m168-streaming-topk-verdict.md` § 5 para quem for confiar no
  relatório da próxima vez.
- **Diagnóstico de memória do top-k colunar:** com `THEODB_ADMIT_TRACE=1` no ambiente do servidor, o log passa a
  registrar quantos bytes o caminho colunar decodificou para responder à consulta
  (`theodb_decode_batch: rows=… bytes=… work_mem_bytes=…`). Quem opera consegue ver o consumo real em vez de
  inferi-lo do `VmRSS`, que é dominado pelos `shared_buffers` mapeados em todo backend e não isola a consulta.
  A primeira medição com ele já mostrou que o limite de proteção subestima o consumo real num `SELECT *` largo,
  porque compara com bytes comprimidos em disco — os números estão em (#218).

### Deprecated

### Removed

### Fixed

### Security

## [0.158.0] - 2026-07-29
### Fixed
- **Columnar top-k de projeção (M167):** `ORDER BY` sobre uma coluna de texto voltava ao plano lento em bancos cuja colação já é byte-a-byte (`C`/`POSIX`), porque a checagem reconhecia apenas as colações *nomeadas* `C` e `POSIX` e não a colação `default` — que é o que quase toda coluna carrega. Agora a propriedade é verificada na colação efetiva do banco, então `ORDER BY <texto>` usa o caminho rápido onde é comprovadamente seguro e continua recusando onde não é (qualquer colação linguística, mesmo declarada explicitamente na consulta).
- **Ferramental de verificação:** o harness de cobertura por tipo passa a **recusar** rodar contra um banco cujo `hits` tenha forma de ClickBench, em vez de recriá-lo silenciosamente e destruir o conjunto de dados. A recusa explica como prosseguir (apontar `PGDATABASE` para um banco descartável). Durante o M167 essa perda aconteceu duas vezes — a segunda depois de o risco já estar documentado, o que mostrou que aviso em texto não substitui verificação em código.
- **Columnar top-k de projeção (M167):** com o recurso ligado por padrão, o planejador passa a inspecionar o plano de **toda** consulta em busca da forma que ele acelera — inclusive em bancos sem nenhuma tabela colunar. O custo por consulta é pequeno e a inspeção não altera planos que não casam, mas é uma mudança de comportamento em relação à versão anterior, em que a inspeção só acontecia com o recurso explicitamente ativado. Quem precisar do comportamento antigo pode desligar `theodb.enable_columnar_late_mat`.
- **Columnar top-k de projeção (M167):** `ORDER BY` sobre coluna de texto poderia devolver as linhas erradas num banco criado com provedor de colação ICU declarando `LOCALE 'C'` — a checagem olhava só o nome da colação do banco, que nesse arranjo diz "C" enquanto a ordenação real é linguística. Agora o provedor também é verificado, e qualquer arranjo que não seja comprovadamente byte-a-byte volta ao plano anterior.
- **Columnar top-k de projeção (M167):** o limite que protege contra decodificar uma tabela inteira em memória não tinha efeito em tabelas recém-carregadas — ele lia uma estatística que só é preenchida por `ANALYZE`, e uma tabela colunar carregada em massa fica sem essa estatística indefinidamente. Passa a consultar o tamanho real da relação quando a estatística está ausente.
- **Columnar top-k de projeção (M167):** um `SELECT *` sem `WHERE` com `ORDER BY … LIMIT k` sobre uma tabela colunar larga podia decodificar a relação inteira em memória antes de aplicar o limite — com o default ON isso deixaria de ser um caminho opcional e passaria a atingir consultas comuns. O planejador agora recusa a rota quando o tamanho da relação supera de forma desproporcional o `work_mem` da sessão, caindo no plano nativo (que é correto para qualquer entrada). Duas ressalvas honestas: a conta usa o tamanho da relação inteira, ignorando quantas colunas a consulta projeta e quão seletivo é o filtro — então uma projeção estreita sobre uma tabela grande pode ser recusada mesmo cabendo na memória; e como os bytes em disco estão comprimidos, o volume decodificado é **maior** que o estimado. É um teto contra catástrofe, não um limite justo. Tornar o caminho O(k) de ponta a ponta — decodificando por partes em vez da relação inteira — está rastreado em (#215).
- Implementation tooling: the `/implement` halt-loop driver hardcoded a Node/TypeScript stack — it instructed the loop to write `.test.ts` files and run `npm test`, in a repository that is Rust + Python. Any `/implement` run here would have driven an autonomous, commit-making loop against a test runner that does not exist. The RED/GREEN phases now derive the runner from the plan's own TDD command, then the build manifest (`Cargo.toml` / `pyproject.toml` / `package.json` / `go.mod` / `Makefile`), and refuse to fabricate a RED when the test cannot run on the current machine.
- Implementation tooling: `check_wiring.py` searched pillar (a) callers in `.rs`/`.py`/`.ts` but pillar (b) integration tests only in `.ts`/`.py` — so a Rust symbol could never satisfy the integration-test pillar with a Rust test, silently forcing every Rust task down the ADR-DEFER path. Both pillars now share one glob set (`.rs`/`.go`/`.sql` added), and the remediation message no longer names a `.test.ts` path.

## [0.157.0] - 2026-07-27

### Added
- M166: **wide `SUM(int2_col ± const)` aggregate-argument routing** in the columnar-agg CustomScan (ClickBench q29). `SELECT sum(ResolutionWidth + 1), …` declined because the aggregate argument is an `OpExpr` (`col + const`), not a bare `Var`; the new `AggSpec::SumIntAddConst{col,delta}` (kind 9, delta hi/lo-encoded on the wire like the M161 IN-list) admits it, emitting `sum(cast(col→int8) + delta)`. **Fail-closed to a provably-overflow-free class:** only an `int2` base column with an `int4` operator result, and only when the whole int2 domain ± delta stays inside int4 (`±32768 + delta` both fit) — so PG raises no per-row `22003` and the widened int8 sum is exact/byte-identical. An `int4`/`int8` base, an `int2`/`int8` result, a non-additive op, a non-integer const, or an out-of-range delta all decline to the native plan (trace `agg_sum_expr_unsupported`). The `EXPLAIN VERBOSE` deparse rebuilds the nested `Var` against the base rel (M131 `OUTER_VAR` hazard). Type-coverage A/B (`columnar_type_ab.py`) gains SUM-expr cases: `sum(c2±const)` routes diverged=0; int4-base / int8-result / out-of-range decline. **q21/q22 (`MIN(text)`) recorded as correct collation honest-negatives** — DataFusion computes byte-minimum, PG computes collation-minimum; a deterministic collation constrains equality not order, so routing gives an A/B-visible wrong result (safe only under C/POSIX, which ClickBench's default-collation columns are not). **q27 (`AVG(length(URL))`) deferred** — needs a new scalar-func-in-agg mechanism + a UTF-8 encoding gate.


### Fixed
- M166 (review hardening — council-rust-pgrx MEDIUM): the int-arithmetic admit gates (`SUM(int2±const)` and the GROUP BY `IntAddConst` expr) matched the operator by name (`+`/`-`) + result type, without the builtin-operator guard that `classify_text_op` already applies. A user-defined `OPERATOR +(int2,int4)` placed ahead of `pg_catalog` in `search_path` could run arbitrary semantics while DataFusion computes `col + delta` — a silent divergence. Both gates now decline any operator with `opno >= FirstNormalObjectId` (fail-closed, parity with the text-op gate). Very-low reachability (adversarial DDL) but closes the class, not just the instance.

## [0.156.0] - 2026-07-27

### Added
- Roadmap amended: added M165 (GROUP BY multi-chave — q17, q34), M166 (agregados de string + wide SUM(expr) — q21, q22, q27, q29), M167 (top-k de projeção — q23–q26) — cobertura de pushdown para as queries que seguram o geomean geral vs ClickHouse, decompostas por shape a partir da medição fresca de 2026-07-27 (`/roadmap-feature`).
- M165: **projected-constant output column** in the columnar-agg CustomScan — `SELECT <int literal>, col, count(*) … GROUP BY 1, col` (ClickBench q34) now routes. The `SELECT 1` puts a bare `T_Const` in the output target that `classify_target_node` had no arm for; PG eliminates the constant *group key* so the effective grouping is single-key. The new `TargetSlot::ConstOut` admits it as a fixed output cell (layout kind=3, threaded through encode/decode/materialize on the int channel), **fail-closed to the integer class** {int2,int4,int8} + non-NULL — a float const (IEEE), text const (collation) or NULL declines to the native plan (traces `const_out_type_unsupported`/`const_out_null`). The const slot never counts toward `numCols`, so the Agg-swap arity match is preserved. Measured (1M ClickBench, same-box vs ClickHouse 26.8.1): q34 flips non-pushdown → pushdown — **152× → 10.19× vs ClickHouse** (same-engine hot **28.5 s → 1.22 s**, ~23×), 43/43 A/B byte-identical (no regression), type-coverage A/B **26/26** (`docs/benchmarks/m165-const-out-verdict.md`). Discovery finding (blueprint): multi-key GROUP BY already routed (q16 proof); q34 was the only real blocker. Type-coverage A/B (`columnar_type_ab.py`) gains const-out cases (int routes byte-identical; float/text/NULL decline). **q17 stays an honest-negative** — routing an AGG_SORTED text GROUP BY under an unordered `Limit` (no parent `Sort`) would let the GroupAgg's collation-order pathkeys be consumed upstream (merge join / DISTINCT / setop) without a re-sort, a wrong result the LIMIT-stripped A/B is blind to (council-rust-pgrx); the correct fix (collation-ordered executor emission) is a separate deferred capability.

- **CI: gate `actionlint` sobre os próprios workflows (#211).** Nenhum gate do repo olhava para o `.github/` — foi por isso que 4 workflows quebrados passaram 3 dias despercebidos. O `actionlint` reproduz o defeito exato (`both "paths" and "paths-ignore" filters cannot be used for the same event "push". note: use '!' to negate patterns`) e ainda sugere a correção aplicada. Traz shellcheck embutido nos blocos `run:`. Imagem fixada por digest; `.github/actionlint.yaml` declara o label self-hosted `theodb-do` para que 17 achados de ruído não afoguem o sinal.

- **CI: `timeout-minutes` em todo job próprio.** `pgvector-compat` (20), `harness-unit` (15), `image-and-bench` (45), `migration-smoke` (25) e `notify-on-failure` (10) rodavam com o default do GitHub de **360 min**. Num runner serial compartilhado por 5 repositórios, um job pendurado sequestra a fila de todos por 6 horas — e o próprio repo já registrou `harness-unit` levando 999s só para falhar. Os valores são folga sobre a duração observada, alinhados aos jobs que já declaravam timeout.


### Fixed

- **CI: quatro gates não executavam nada desde 24/07 — incluindo o `license-gate` da regra D1 (#211).** `license-gate`, `lint-rust`, `cassert-sql-safety` e `schema-drift-gate` declaravam `paths` **e** `paths-ignore` no mesmo evento, combinação que o GitHub rejeita no parse: os runs eram criados e morriam em `startup_failure`, com zero jobs e 0s de duração. Medido: 95 de 100 runs falhos nos três primeiros e 100 de 100 no `schema-drift-gate`. Na prática, por três dias nada barrou uma dependência AGPL de entrar na distribuição — a regra que o próprio arquivo chama de "a mais inegociável do projeto". A economia de fila que motivou o `paths-ignore` (#187) foi preservada, agora expressa **dentro** do `paths:` via padrão negado (`!**/*.md`), que é um filtro só e não conflita. Os gates voltam a rodar após três dias sem enforcement, então é esperado que algum acuse achado real acumulado.

- **CI: sete exportações redundantes de cache de imagem por run.** Os oito jobs que constroem a imagem declaravam `cache-to: type=gha,mode=max` — cada um exportando **todas** as camadas para o cache do GitHub pela rede, embora ninguém lesse as sete cópias extras. Só o build de `image-and-bench` (tag `theo-db:ci`) publica agora; os demais seguem lendo com `cache-from`, sem qualquer mudança no que constroem. Os builds em si foram **mantidos de propósito**: cada job reconstruir a imagem é o que garante que ele testa o commit atual — trocar por um atalho do tipo "pular se já existe" economizaria ~2 min e reintroduziria risco de falso-verde. Ganho esperado mas ainda **não medido** (o runner está ocupado; a próxima execução completa confirma).

## [0.155.0] - 2026-07-27

### Added
- M164: **three false-green/infra guards** on the ClickBench harness (`benchmarks/run_m128_clickbench.py`), closing retro items B+C of the M160-M162 session (the mechanizable noise that cost hours). (B1) `_ensure_sample` now counts the cached sample's rows (`wc -l`) and **re-materializes on a stale count** — a 1M cache can no longer be served as "100M" (the exact M162 false-`DONE`); the freshness target is clamped to `HITS_TOTAL_ROWS` so a legit `--n 100000000` (the corpus holds only 99,997,497 rows) is NOT re-streamed ~74 GB every run. (B2) the A/B verdict now classifies each query by whether the **agg pushdown specifically** routed — keyed on `theodb_columnar_agg` in the plan, NOT a broad `Custom Scan` (the always-on `theodb_columnar_project` scan renders a `Custom Scan` under every columnar query, which would mask a declined agg): a `--agg` run where the agg pushdown routes nothing is flagged `no_pushdown_exercised`, and a real `DIVERGENCE` outranks that flag so a pushdown bug is never hidden behind "nothing exercised". (C) a **pre-flight sizing** guard `preflight_sizing` **refuses** a load whose full on-disk footprint (sample TSV + heap copy + columnar table) would not fit a safe fraction of free disk, and **warns (never blocks)** when the in-DB working set exceeds RAM headroom — larger-than-RAM is an intentional TheoDB regime (the M162 100M run was deliberately larger-than-RAM; the 15 GB box OOMed 2× with no guard). All are pure, env-injected helpers unit-tested with no DB (`benchmarks/test_m164_harness_guards.py`, 20 tests green — including the projection-vs-agg routing proof and the DIVERGENCE-outranks-TRIVIAL precedence).

## [0.154.0] - 2026-07-27

### Added
- Roadmap amended: added M163 Harness A/B por-tipo (retro item A) + M164 Endurecer o harness de benchmark (retro items B+C) (`/roadmap-feature`).
- M163: **type-coverage A/B harness** (`benchmarks/columnar_type_ab.py` + tests) — closes the project's most recurring defect (the ClickBench A/B doesn't exercise the type space, so type-class routing bugs survive it and are only caught by council review after a 14-min rebuild; M151/M154/M157/M161). For each routed admit-path × each per-type edge value (int2 `32767`, int4/int8 max, float `-0.0`/`NaN`/`inf`, timestamp/date/timestamptz, non-C text, NULL) it asserts the M161 fail-closed contract — **byte-identical if it routes, OR correct-decline** — over a synthetic `theodb_columnar` table. A **positive control** (a seeded-divergent twin run *through* the same `ab_check` oracle path the real cases use) proves the oracle catches a wrong result (the M161 `out_typoid` BLOCKER shape). Validated live on a fresh TheoDB build: Tier-1 `pytest` 8 passed, Tier-2 **20/20 cases as-expected**, positive-control diverged=2. Bespoke pytest reusing the shipped symmetric-EXCEPT oracle (no new dep, Rule 9); routing evidence and the byte-identity comparison come from the **same execution** (`EXPLAIN ANALYZE CREATE TEMP TABLE _ab_on AS <sql>`), so a `route` case whose pushdown silently didn't fire in the comparison query can't pass as a false `diverged=0` (council-benchmark HIGH); wired as a pre-`/review` gate in `rules/testing.md § 5.1`. Evidence: `docs/benchmarks/m163-type-coverage-verdict.md`.


### Fixed
- **CI: colisão de porta 5432 derrubava 6 dos 10 jobs, de forma autoperpetuante.** Todos os containers do `ci.yml` faziam bind fixo `-p 5432:5432` no host. Um run cancelado por `concurrency.cancel-in-progress` pula o `Teardown`, vaza o container, e a partir daí **todo run seguinte** morre com `Bind for 0.0.0.0:5432 failed: port is already allocated` — até alguém limpar o host à mão. Atingia `image-and-bench`, `ai-sql`, `hybrid-search`, `columnar-measure`, `bm25-measure` e `nl-sql`. Agora cada container usa porta **efêmera** (`-p 5432`), com o `PGPORT` resolvido via `docker port` e exportado por `$GITHUB_ENV`, mais uma pré-limpeza defensiva por nome. Além de corrigir, remove o impedimento estrutural a rodar esses jobs em paralelo (porta fixa é incompatível com concorrência). Mesmo padrão já adotado em `theo-memory/integration.yml`.
- **CI: o guard de SSRF do M134 bloqueava o stub offline das suítes de IA — 20 de 33 testes falhando.** `f34ec2a` (#117) fechou a classe de SSRF em `ai._chat`, recusando endpoints que resolvem para loopback/privado/link-local. As suítes `test_ai_sql` e `test_nl_sql` usam de propósito um **stub no host** (`host.docker.internal`, `127.0.0.1`) para serem determinísticas e não chamar API externa — então o guard passou a bloqueá-las. O conflito nunca apareceu porque esses jobs já morriam antes, na colisão de porta. Os três jobs afetados agora sobem o cluster com `-c theodb.egress_allowlist=host.docker.internal,127.0.0.1` — o escape hatch que a própria mensagem de erro instrui, um GUC `Suset` (operator-only, nunca settable pelo caller). **Não afrouxa produção:** o guard segue ativo para todo o resto e a distribuição continua com allowlist vazio; apenas o cluster efêmero de teste permite o host do próprio stub. Mecanismo validado localmente (o PostgreSQL aceita o GUC prefixado como placeholder no startup e `SHOW` devolve o valor).
- **CI: 4 jobs morriam com `pg_isready: command not found` — o runner não tem cliente PostgreSQL.** `image-and-bench`, `hybrid-search`, `ai-sql` e `nl-sql` chamam `scripts/smoke.sh`, que conecta **de fora** do container pela porta publicada — o caminho do usuário real, e justamente o que um `docker exec` nunca exercitaria. A lacuna ficou invisível enquanto esses jobs morriam antes, na colisão de porta: o script nunca era alcançado. Corrigir a porta a expôs como exit 127. Sem root no runner para instalar o pacote, e trocar por `docker exec` mudaria silenciosamente **o que** é asseverado — então o script agora cai para um cliente **containerizado compartilhando o namespace de rede do host**, onde `localhost:$PORT` resolve para a mesmíssima porta publicada e toda asserção mantém o sentido. Só ativa quando o host não tem `psql`/`pg_isready`; quem tem paga zero. Validado localmente, incluindo a passagem de heredoc.
- **CI: o `publish` falhava em ~2s e a imagem nunca era publicada — faltava o arquivo `.trivyignore` (#187).** O workflow reutilizável passa `trivyignores: .trivyignore` ao `trivy-action`; com o arquivo **ausente**, o Trivy não assume "sem exceções" — ele aborta com `cannot find ignorefile '.trivyignore'` antes de escanear qualquer camada. O job morria parecendo um gate de CVE fazendo seu trabalho, quando era o oposto exato: **nada nunca foi escaneado**, e o push ao GHCR era pulado — motivo pelo qual `docker pull ghcr.io/usetheodev/theo-db:latest` (o primeiro comando do README) responde `manifest unknown`. Adicionado o arquivo, deliberadamente **sem nenhuma CVE allowlistada**, seguindo a convenção já usada em theo-memory. Consequência esperada e honesta: o gate passa a rodar de verdade pela primeira vez, então pode reprovar por CVEs reais — isso seria achado de segurança legítimo numa imagem nunca escaneada, não regressão.
- **CI: `PGPORT` chegava vazio aos steps que conectam do host, e o vazio era silencioso.** Ao migrar para porta efêmera, os steps mantiveram um `env: PGPORT: ${{ env.PGPORT }}` explícito que **sombreava** com string vazia o valor que o `$GITHUB_ENV` já exportava corretamente. Como todo consumidor lê `${PGPORT:-5432}` — e `:-` trata vazio como ausente —, a conexão caía de volta em 5432, onde nada mais escuta: `hybrid-search` e `ai-sql` falhavam com "connection refused" três steps depois da causa. Os 12 overrides foram removidos (o export já propaga para todo step seguinte) e a resolução da porta agora é **validada na origem**: um valor vazio ou não-numérico aborta o job com mensagem explícita, em vez de degradar num fallback enganoso (Regra 8).
- **CI: o job `harness-unit` levava 999s para FALHAR — ~80% do tempo do run — num job cuja descrição é "Fast, container-free".** Cinco módulos de teste (`test_am_cosine_ip`, `test_am_crash`, `test_am_maintenance`, `test_extension_install`, `test_unified`) abrem conexão com um Postgres real mas não declaravam o marker `integration`, então eram arrastados para o tier `-m "not integration"`, onde não existe banco: cada um queimava seu `connect_timeout` e então erravam. Medido depois do fix: **219 testes passando em 1,54s**. Como esses módulos também eram *excluídos* de `-m integration`, o fix **aumenta** a cobertura — eles passam a rodar contra um container de verdade pela primeira vez. Novo guard `benchmarks/tests/test_marker_hygiene.py` impede a recorrência, com controle positivo para não exigir o marker de testes de falha-deliberada (`port=1`).
- **CI: resíduo da migração PG17→PG18 em `packaging/` quebrava `pg-regression` e `bm25-measure`.** O engine é PG18 (`Dockerfile`: `postgres:18-bookworm`), mas cinco arquivos seguiam pinados em 17 — enquanto a prosa deles já dizia 18, então revisor e leitor viam concordância onde o build via contradição. `run-regress.sh` usava `BIN=/usr/lib/postgresql/17/bin`: o caminho não existe em PG18, a substituição de comando devolvia string **vazia** e o guard reportava o desconcertante `engine is not 18.4 (got: )` — binário ausente disfarçado de divergência de versão. `Dockerfile.regress` clonava `REL_17_10` (baselines de `pg_regress` diferem entre majors); `Dockerfile.bm25` e `Dockerfile.m53-bm25` instalavam `postgresql-server-dev-17` contra `pg_config` do 18, matando o `make` com exit 2. Agora `BIN` é derivado de `pg_config --bindir` e falha alto e específico se o binário não existir (Regra 8), `PG_TAG=REL_18_4`, headers no 18, e `Dockerfile.m51-test` (local-dev) usa o próprio `$PG_MAJOR` que já declarava. Novo guard `benchmarks/tests/test_packaging_pg_major.py` casa qualquer pin de major em `packaging/` contra o `ARG PG_MAJOR` do Dockerfile — ignorando comentários, para não punir a prosa que explica o fix.

- M163: **float `GROUP BY` divergence** in the columnar CustomScan — the very bug the new type-coverage harness was built to catch, found on its first live run. A bare-`Var` group key of type `float4`/`float8` was admitted to the vectorized DataFusion path, which groups by IEEE-754 byte value (`−0.0` and `+0.0` land in *distinct* groups; distinct `NaN` bit-patterns each get their own group), whereas PostgreSQL's `float8eq`/`float4eq` group `−0.0` **with** `+0.0` and collapse all `NaN`s into one group. Result: `SELECT f, count(*) FROM t GROUP BY f` returned a different group set columnar-vs-heap (symmetric-EXCEPT `diverged=2` on the `-0.0`/`+0.0` edge). Now the group-key classifier declines float types (`admit_trace("group_key_float_ieee_semantics")`), routing them to the row executor — same fail-closed remedy as the M154 float `COUNT(DISTINCT)` decline. Integer/text/temporal group keys are unaffected. Regression proof: `benchmarks/columnar_type_ab.py` case `group_f8`/`group_f4` now DECLINE (was `diverged`), full matrix green (M163).

- **Ressalva honesta sobre as correções de CI acima: a esteira NÃO fica 100% verde, e isso é deliberado.** O job `migration-smoke` permanece vermelho porque um `pg_dump` de pgvector vanilla contendo `USING ivfflat` não restaura — o TheoDB expõe `theodb_ivfflat` mas não o alias de compatibilidade `ivfflat` (o #182 criou apenas o de `hnsw`). O gate está **certo** ao falhar: detectou uma lacuna real de drop-in, não um defeito de pipeline. Torná-lo verde exigiria remover o índice do seed do smoke — enfraquecer o gate para esconder o achado, anti-padrão explícito de `rules/cycle-implement.md`. Rastreado em #206. Da mesma forma, o `publish` segue bloqueado por 15 CVEs reais em `gosu` (#207), agora que o gate Trivy de fato escaneia. Quem for cortar release a partir deste estado precisa saber disso.

- **CI: instrumentado o step que concentra 80% do wall-clock da esteira.** Medido em 2026-07-27, com a esteira já corrigida: o run inteiro leva 27,9 min e o step `Integration tests` do `image-and-bench` leva **22,2 min** sozinho (os 34 steps de build de imagem + buildx somam 2,9 min = 10%; a suíte de regressão PG18.4, 1,0 min). O step agora roda com `--durations=25` porque ~305 testes a 4,3s de média não revelam se o custo está concentrado ou espalhado — e a resposta decide a estratégia: atacar testes específicos, ou paralelizar. Paralelizar hoje está **bloqueado**: não há `conftest.py` e os testes criam tabelas de nome fixo repetido (`CREATE TABLE m` em 12 pontos) no mesmo banco, então `pytest -n auto` trocaria tempo por flakiness. Parte dos 22 min é trabalho legítimo novo: os 5 módulos remarcados como `integration` (incl. `test_am_crash`, que reinicia o Postgres) nunca haviam rodado contra um container.

## [0.153.0] - 2026-07-27

### Added
- M162: the honest **100M** larger-than-RAM ClickBench gap measurement (the M159 `[NEEDS-100M]`). Full 99,997,497-row `hits` loaded into `theodb_columnar` on a 15 GB box (working set > RAM) and benchmarked vs ClickHouse MergeTree same-box. Measured verdict — **the scale-limit failure, not the ratio, is the finding**: TheoDB completes only **19/43** queries on this box (5 hard-fail — 3 timeouts >300 s, a `byte array offset overflow` i32 scale bug on a pushdown query, one backend OOM — and the run OOM-killed at 24/43) while ClickHouse serves all 43 in 0.008–10 s. The typical per-class gap stays in the 1M ballpark (aggregate-pushdown 8–26×, native 16×); the 24.3× geomean over survivors is outlier-carried (q0 `COUNT(*)` 1495× = missing count fast-path, q19 `SELECT *` 837× = M148 materialization) and cross-population, so not a matched widening. The measured ratio **overstates** the gap (both timing asymmetries favor ClickHouse → true gap ≤ measured). The deciding I/O-vs-CPU counter (`shared_blks_read`) was **not** isolated — the top levers point at materialization + the i32 bug, which no decode-byte encoding fixes. So encoding is deferred to a follow-up (M163, format subsystem, ADR-2) on grounds independent of the un-isolated I/O question; no encoding shipped on a guess (Rule 5). Council-benchmark reviewed; framing overreaches corrected. Verdict + artifacts: `docs/benchmarks/m162-100m-gap-verdict.md`.

## [0.152.0] - 2026-07-27

### Added
- M161: bounded expression-routing coverage for the columnar CustomScan — integer `IN`-list WHERE predicates (`ScalarArrayOpExpr`, previously never inspected) and the SAFE expression GROUP BY classes (constant key, integer `col ± k`, and `extract(minute|hour FROM ts)`) now route to the vectorized DataFusion path instead of the row executor. Each class clears its correctness gauntlet — IN(NULL) declines (3-valued logic), `col ± k` computes widened + range-checks the base type (reproduces PG's `22003` on overflow), and `extract` is restricted to the epoch-invariant, integer-valued units (minute/hour; day/month/quarter/year/second decline). Every routed query is A/B byte-identical to the row executor (`result diverged = 0`). Measured on ClickBench 1M: q40 (IN-list), q35 (`ClientIP ± k`) and q18 (`extract(minute)` under HASHED) flip non-pushdown → pushdown (M161).

## [0.151.0] - 2026-07-27

### Added
- M160 hardening (council-rust-pgrx LOW-1): `decode_columns_v2` + `decode_columns` guardam `comp.len() >= comp_len` antes do slice do zstd (diretório corrompido → `Err` tipado, não panic atravessando C) — espelha o guard já presente no scan geral.
- M160: decode zero-copy fixed-width→Arrow no path de pushdown colunar. `decode_columns_v2` decodifica colunas fixed-width NÃO-nulas (int2/4/8, float4/8, timestamp/tz, date) como um stream contíguo little-endian (`DecodedColumn::FixedRaw`), e `build_arrow_from_decoded` as constrói via UM `Vec<T>` tipado por coluna (`PrimitiveArray::from`, handoff zero-copy) — eliminando a tempestade de alocação `.to_vec()` por-célula + a re-cópia do `build_arrow` (o gargalo gêmeo do M148 que o deep-dive/flamegraph mediu na classe coberta). Colunas nullable/varlena/texto/bool e queries com linhas pending same-xact mantêm o caminho de células (fail-safe, byte-idêntico). Byte-idêntico por construção (mesmo `from_le_bytes` plano, sem transform de epoch) — validado por A/B symmetric-EXCEPT colunar vs heap **diverged=0** (GROUP BY sum, count-distinct, multi-agg, min/max) + teste unitário. Endian-safe (`from_le_bytes` explícito). Só o hot path pushdown (vindex/arrow_cache seguem `decode_columns`). **Ganho medido same-binário (GUC `theodb.enable_columnar_fast_decode` off vs on, agg pushdown ON em ambos, EXPLAIN ANALYZE median-of-3, 1M ClickBench): 5.7× (RegionID GROUP BY sum), 7.4× (SearchEngineID GROUP BY sum), 8.3× (sum+count), 1.8× (count-distinct)** — ~2-8× na classe coberta de agregações fixed-width (`docs/benchmarks/m160-decode-zerocopy-verdict.md`), consistente com o flamegraph do deep-dive (a ponte de decode era ~80% do custo).

### Deprecated

### Removed

### Fixed

### Security

## [0.150.0] - 2026-07-26

### Added
- Roadmap amended: added M160 (decode zero-copy fixed-width→Arrow — perf da classe coberta), M161 (roteamento de expressão ao DataFusion — cobertura das não-cobertas), M162 (gap a 100M + encodings type-specific — escala), as 3 layers do deep-dive columnar-improvement (`/roadmap-feature columnar-improvement-layers`; blueprint `knowledge-base/discoveries/blueprints/columnar-improvement-deepdive-blueprint.md`), com referências do acervo + metas de eficiência medidas.
- M159: baseline honesto do gap vs ClickHouse no ClickBench (`docs/benchmarks/m159-clickhouse-gap-verdict.md` + `docs/benchmarks/m159-artifacts/`). Comparação **mesma-box, mesmo subsample 1M sistemático, mesmo TSV** para ambos engines (TheoDB `theodb_columnar` PG18+v0.149.0 vs `clickhouse local` MergeTree, 8 vCPU) — elimina a incomparabilidade de escala vs os números publicados full-100M. Resultado medido (razão TheoDB/ClickHouse, hot min-of-2, 3 runs): **geomean 19.4× no benchmark inteiro**, **7.54× nas 32 queries com pushdown vetorizado**, **303× nas 11 sem pushdown** (executor row-based do PG sobre storage colunar, 12-21s vs 0.01-0.15s do CH); **6/43 já ≤3×** (q6 MIN/MAX **mais rápido** que o CH, 0.17×, via directory fast-path M105; q4/q8/q12/q13/q42 agregações simples em 2.2-3.0×). 43/43 queries OK, A/B byte-idêntico 43/43. **Veredito honesto:** o "2-3×" é alcançável **por classe** (agregações vetorizadas cobertas por M151-M157), NÃO no benchmark inteiro hoje; o maior lever é expandir cobertura de pushdown para as 11 queries restantes (GROUP BY multi-chave, agregados de texto/expressão), que sozinhas puxam o geomean de 7.54× para 19.4×. Caveats documentados: subsample 1M≠100M (razão é lower-bound do gap; `[NEEDS-100M]` p/ o canônico), números publicados c6a.4xlarge marcados `[NO-BASELINE-COMPARABLE]` (escala/box). Cobertura de pushdown cresceu 6→32/43 (M148-M158). Regra 5 honrada: número medido, nenhuma razão inventada.

## [0.149.0] - 2026-07-25

### Added
- M158: late materialization no regime `SELECT <cols> … ORDER BY <chave> LIMIT k` sobre tabela colunar. Um novo CustomScan (`theodb_columnar_project` M149 abaixo de `Sort`+`Limit`) é substituído por um top-k vetorizado que decodifica {projeção ∪ chave ∪ filtro} uma vez, roda `filter → sort([chave]) → limit(k)` no DataFusion (TopK O(N log k)) e materializa a projeção completa (via `arrow_value_to_datum`) SÓ para as k linhas sobreviventes — pagando o custo de `form_row`/`palloc` por linha (M148: ~80% do scan) para k linhas, não N. Reusa o framework do CustomScan de agregação (`mode == 2` no `custom_private`; o `Limit` acima é preservado e re-aplica k). Guards fail-closed (declinam ao plano nativo): GUC `theodb.enable_columnar_late_mat` default OFF; só `Limit(k)` constante int8 (≤ i32::MAX) sem OFFSET; `Sort` de exatamente 1 chave; projeção só de colunas base materializáveis; **chave-texto só sob colação byte-order C/POSIX** (colação linguística determinística como `en_US.UTF-8` declina — determinismo garante igualdade, não ordem de sort; a colação efetiva do `ORDER BY` é lida de `Sort.collations`, respeitando `COLLATE` override); bpchar declina como chave (apara brancos à direita); WHERE 100% pushable (zona/texto) senão declina. Validado por A/B symmetric-EXCEPT LIMIT-preserving byte-idêntico E ordem-idêntica (diverged=0 em SELECT*/subconjunto/filtro numérico/filtro texto/ASC/DESC/bpchar-output/chave-texto-C; chave-texto-linguística declina corretamente) + wall-clock medido **1.60× median** (2M×30 SELECT* ORDER BY LIMIT 10, `docs/benchmarks/m158-late-mat-verdict.md`). Caveat honesto: O(N) memória (segura o batch Arrow) vs O(k) do top-N heapsort nativo — por isso default OFF.

## [0.148.0] - 2026-07-25

### Added
- M157: `GROUP BY date_trunc('unit', ts)` sobre uma coluna `timestamp` agora roteia ao CustomScan colunar vetorizado (DataFusion), com a chave-expressão serializada num 3º canal de `custom_private` e reconstruída como `date_trunc` no `.aggregate` (o Agg-swap reconhece a chave-expressão além das chaves `Var`, tanto na admissão quanto na construção do `custom_scan_tlist`); byte-idêntico ao PostgreSQL, validado por A/B + EC harness; serialização fail-closed (declina em unit com NUL, sem panic em layout corrompido). Guards fail-closed (declinam ao plano nativo): granularidade só `{second,minute,hour,day}` (epoch-invariantes); `month`/`quarter`/`year`/`week` declinam (o storage colunar usa epoch µs-desde-2000 lido pelo Arrow como µs-desde-1970 → truncagem de calendário divergiria); `timestamptz` (diverge sob `TimeZone≠UTC`), `EXTRACT`, `CASE` e aritmética também declinam. Honest-negative medido: HAVING não é roteado (as queries HAVING do ClickBench têm bloqueios independentes — cobertura 0 isolada).
- Roadmap amended: added M157 cobertura expr-group/HAVING (`/roadmap-feature coverage-expr-group-having`), M158 late materialization (`/roadmap-feature late-materialization`), M159 medir gap vs ClickHouse (`/roadmap-feature measure-gap-vs-clickhouse`).

## [0.147.0] - 2026-07-25

### Added
- M156: predicados de TEXTO no WHERE (`col = 'x'`, `col <> ''`, `col LIKE '%p%'`, `col NOT LIKE 'a%'`) agora roteiam ao CustomScan colunar vetorizado (DataFusion), subindo a cobertura de agregação do ClickBench. O predicado de texto é serializado num 2º canal de `custom_private` (nós `Integer`/`String`) e reconstruído como filtro DataFusion sobre a coluna Utf8; byte-idêntico ao PostgreSQL nativo. Guards de correção (fail-closed → plano nativo): só sob collation determinística e operador `=`/`<>`/`~~`(LIKE)/`!~~`(NOT LIKE); declinam ILIKE (`~~*`), regex (`~`/`!~`/`~*`/`!~*`), `bpchar`, collation não-determinística, literal não-UTF-8 (server encoding LATIN1/etc.) e padrão LIKE terminado em escape pendente (`\` final, que o PostgreSQL rejeita com erro 22025).

## [0.146.1] - 2026-07-25

## [0.146.0] - 2026-07-25

### Added
- M153: `GROUP BY <coluna texto>` no path AGG_SORTED (GroupAgg) agora roteia ao CustomScan colunar vetorizado quando
  a saída é re-ordenada por um `Sort` acima (a ordem de grupo byte-wise do executor fica irrelevante) e a collation
  da chave é determinística. Guard de correção de contagem: chave texto com collation NÃO-determinística declina ao
  plano nativo (o hash byte-wise do DataFusion só agrupa como o PostgreSQL sob collation determinística) — aplicado
  no admit, cobrindo também o path AGG_HASHED. `GROUP BY texto ORDER BY texto` (ordem de grupo consumida direta, sem
  re-sort) declina. Resultado byte-idêntico ao heap (A/B).


### Fixed
- Colunas `char(n)`/`bpchar` deixam de ser roteadas ao agregado colunar vetorizado em `GROUP BY` e `COUNT(DISTINCT)`:
  a igualdade `bpchareq` do PostgreSQL ignora espaços à direita (`'ab'` = `'ab '`), o que o agrupamento byte-wise do
  DataFusion não reproduziria — poderia contar grupos/distintos a mais. Agora declinam ao plano nativo (correto).

## [0.145.0] - 2026-07-25

### Added
- M154: `COUNT(DISTINCT coluna)` agora roteia ao CustomScan colunar vetorizado (DataFusion `count_distinct` EXATO —
  nunca approx/HLL), subindo a cobertura de agregação do ClickBench. Guard de correção: colunas de texto sob
  collation não-determinística declinam ao plano nativo (a igualdade byte-wise do DataFusion só coincide com a do
  PostgreSQL sob collation determinística); `count(DISTINCT expr)`, `count(DISTINCT a,b)`, `sum/avg(DISTINCT ...)` e
  `count(DISTINCT float4/float8)` (a igualdade IEEE do DataFusion trata `-0.0 ≠ +0.0`/NaN-bits distintos, o
  PostgreSQL trata `0.0 = -0.0`/NaN igual) continuam no plano nativo. Resultado byte-idêntico ao heap (A/B).

## [0.144.0] - 2026-07-25

### Added

- Instrumentação de medição M152 (spike): `THEODB_ADMIT_TRACE=1` emite a razão pela qual o CustomScan de agregação declina cada query (`agg_distinct_filter_order`, `unpushable_where_qual`, `swap_agg_sorted_*`, etc.) — behavior-neutral quando off (roteamento idêntico). Mapa de roteamento medido (`docs/benchmarks/m152-routing-map.md`): as 29 queries não-vetorizadas classificadas por razão real (zero gaps) — reordena M153-M155 (COUNT DISTINCT > text-`<>`-WHERE > GROUP BY texto), pois os bloqueios são compostos e a cobertura marginal de cada fatia é 2-4. (#M152)
- Roadmap ampliado: M152 (spike measurement-first — mapear o roteamento das 29 queries não-vetorizadas), M153 (rotear GROUP BY texto), M154 (rotear COUNT(DISTINCT) exato), M155 (rotear Top-N/TopK) — o programa para fechar o gap colunar vs ClickBench (29/43 queries ainda row-based, ~80% materialização heap-tuple), derivado do deep research `columnar-gap-closing-strategy-blueprint` (padrão: ampliar o CustomScan DataFusion vetorizado que já temos, não trocar de paradigma nem re-embed engine). (`/roadmap-feature columnar-gap-closing`)

### Deprecated

### Removed

### Fixed

### Security

## [0.143.0] - 2026-07-25

### Added

- Cobertura do CustomScan vetorizado DataFusion (M151): agregados (`COUNT`/`SUM`/`AVG`, com ou sem `GROUP BY`) que filtram por `WHERE col <> const` em coluna numérica/temporal sobre `theodb_columnar` passam a rotear pelo caminho vetorizado (batch Arrow), não mais tuple-a-tuple. `<>` é detectado como o negador do `=` btree (não é estratégia btree) e aplicado só pelo `Filter` DataFusion (nunca poda chunk — a poda por min/max continua só nos ops ordenados). Const **cross-type na classe inteira** (`int2`/`int4`/`int8`) é coercido ao domínio da coluna com range-check (o padrão real do ClickBench: coluna `int2`, literal `int4` — ex. `AdvEngineID <> 0`), ampliando também `=`/`<`/`>` cross-type inteiro. Cross-type temporal/float é declinado ao plano nativo (a coerção por bits crus não é isomórfica de ordem lá — pegue pelo review). Escopo honesto: predicados numéricos/temporais/bool; `<>` em texto (`SearchPhrase <> ''`) é follow-up (const-texto não cabe na serialização `custom_private` atual — ADR-4). (#M151)

### Deprecated

### Removed

### Fixed

### Security

## [0.142.0] - 2026-07-25

### Added

- Chunk-group filtering no scan colunar (M150): um `WHERE col op const` sobre `theodb_columnar` pula chunk-groups inteiros cujo zone-map min/max prova não conter match, sem descomprimi-los (estilo Citus chunk-group filtering). Reusa o `chunk_can_match`/`directory` do M105 e o CustomScan do M149. Métricas SQL `theodb_columnar_chunks_skipped()`/`_scanned()`; GUC `theodb.enable_chunk_skip` (default on — requer `theodb.enable_projection` on, pois o skip viaja no CustomScan de projeção). (#M150)

### Deprecated

### Removed

### Fixed

### Security

## [0.141.0] - 2026-07-25

### Added

- Projection pushdown no scan colunar (M149): um `CustomScan` (`theodb_columnar_project`) vence o
  SeqScan sobre tabelas `theodb_columnar` em queries não-agregadas e materializa apenas as colunas
  referenciadas (`targetlist ∪ qual`, estilo Citus `ColumnarAttrNeeded`) — atacando o custo dominante
  do M148 (heap-tuple de 105 colunas por linha). Colunas fora do conjunto pulam o decode zstd e saem
  NULL; o resultado é byte-idêntico ao heap. Gated por `theodb.enable_projection` (default ON);
  fallback seguro para decode-tudo quando o nó não é escolhido, a query é agregada, ou aparece um
  Var whole-row/coluna-de-sistema. **Medido** (`docs/benchmarks/m149-projection-pushdown.md`): geomean
  **3,73×** em queries de projeção estreita sobre o ClickBench `hits` real (105 colunas) — `SELECT url`
  4,18×, projeção+filtro 3,12-3,24× — e A/B byte-idêntico ao heap nas 43 queries do ClickBench (0
  divergências). O `/review` (10 pilares + 2 councils) endureceu o nó antes do merge: corrigido um
  use-after-stale-projection (ABA) em que um `abort` de subtransação capturado deixava a máscara de
  colunas de um nó órfã no registry e um nó reusando o mesmo endereço a herdava — o registry passa a ser
  sincronizado na inicialização de todo scan (ambos os ramos), com testes de regressão para o fallback de
  coluna-de-sistema, self-join aninhado e o próprio cenário de subxact-abort
- Acervo de referências primárias local: 25 papers/livros de acesso livre em
  `.claude/knowledge-base/references/papers/` (HNSW, ScaNN/AQ, RaBitQ, DiskANN, Faiss, IIR, RRF,
  BEIR, C-Store, MonetDB/X100, Morsel, ARIES, SSI/MVCC, rigor estatístico de medição, GraphRAG,
  prompt injection) e 8 repositórios de referência (`pgrx`, `tantivy`, `FlameGraph`,
  `parquet-format`, `graphrag`, `beir`, `faiss`, `hnswlib`) — fecham as lacunas de segurança
  Rust/FFI, metodologia de profiling e spec colunar

### Deprecated

### Removed

### Fixed

### Security

## [0.140.0] - 2026-07-24

### Added

- M148 — flamegraph do scan colunar mede o gargalo real e prioriza os M149/M150/M151 por evidência (`docs/benchmarks/m148-flamegraph-scan.md` + harness `benchmarks/profile_columnar_scan.sh`): sobre o ClickBench `hits` real (105 colunas), o scan colunar é 100% CPU-bound e ~80% do tempo é a materialização de cada linha como heap-tuple (`palloc`+`heap_form_tuple` por linha) — a descompressão das colunas é só ~7%. Isso **corrige** a hipótese inicial (que supunha o decode dominante) e define a sequência M149 (projection pushdown, reduz a materialização) → M151 (execução vetorizada, elimina o heap-tuple por-linha) → M150 (chunk-filter, condicional a workload seletivo). Medição com piso de 500 amostras (o gate que impede um flamegraph vácuo) e confound de `cassert` descontado honestamente (M148)
- Roadmap ampliado: M148 (spike/flamegraph do scan colunar), M149 (projection pushdown), M150 (chunk-group filtering), M151 (ampliar CustomScan vetorizado) — o programa para tornar o pilar colunar competitivo em ClickBench, derivado da análise SOTA (Citus + DuckDB/DataFusion) do gargalo revelado pelo gate 1M pós-#190 (`/roadmap-feature columnar-scan-optimization`)
- Primeiro benchmark do pilar colunar em dados representativos após o fix do #190 (`docs/benchmarks/clickbench-1m-postfix-2026-07-24.md`): as 43 queries do ClickBench sobre 1M linhas reais carregam e rodam (42 completam byte-idênticas, 1 timeout, 0 erros), mas o resultado honesto mostra que só 6 das 43 engajam a aceleração vetorizada — as demais são lentas, revelando que os números anteriores vinham de uma amostra enviesada. Relatório consolidado da sessão em `docs/benchmarks/RELATORIO-clickbench-sessao-2026-07-24.md` (#190)
- Plano de correção do defeito que impede carregar dados reais no armazenamento colunar (`columnar-toast-materialize`): a solução escolhida materializa os valores grandes no momento da inserção, em vez de contornar o problema no momento da gravação — remove a causa em vez de mascará-la, e protege contra uma segunda falha possível (dado removido por limpeza automática antes da gravação) (#190)
- Previsão de budget do programa de benchmark oficial (`docs/benchmarks/clickbench-official-budget.md`): custo atual da infraestrutura medido via API de faturamento, custo por etapa com preços reais consultados na AWS, e os guardrails adotados para que nenhuma máquina temporária fique ligada por esquecimento (#187)
- Gate de compatibilidade pgvector no CI: a cada mudança de código, a esteira sobe a **imagem** publicável e executa o fluxo que uma aplicação real faz — `CREATE EXTENSION vector` sem `CASCADE`, tabela com coluna `vector`, e os três índices `USING hnsw (... vector_cosine_ops/l2/ip)` — verificando resultado correto, não apenas ausência de erro. Era a lacuna que deixou passar os dois bugs que impediam qualquer aplicação de subir contra o TheoDB (#181, #182)

### Deprecated


### Removed


### Fixed

- Corrigido o defeito que impedia carregar dados reais no armazenamento colunar: inserções sucessivas falhavam quando os dados continham textos grandes (acima de ~2 KB). Os valores passam a ser materializados no momento da inserção — onde o banco garante o contexto necessário para lê-los — em vez de mais tarde, durante a gravação, quando esse contexto já não existe. Isso também elimina o risco de o dado referenciado ser removido pela limpeza automática antes da gravação (#190)
- Corrigido um use-after-free introduzido pela primeira versão do fix acima, detectado pelo `/review` (2 análises independentes convergiram, com prova contra o source do PostgreSQL 18): a materialização gravava o valor de volta na estrutura de linha do executor e depois liberava a memória, deixando `INSERT ... RETURNING <coluna grande>` (e gatilhos AFTER-ROW) lendo memória já liberada. Agora a materialização usa uma cópia local e a estrutura do executor permanece intacta; regressão coberta por um novo caso `INSERT ... RETURNING` de TOAST no harness (#190)
- Corrigido o harness de benchmark ClickBench para falhar alto quando o oráculo de correção A/B (colunar vs heap) detecta divergência — antes o processo sempre saía com código 0, mascarando um bug de correção de um CI encadeado; e uma passagem de caminho de arquivo (`--cache`) para o shell passa a ser escapada (defesa contra injeção) (#187)
- Descoberto e registrado um defeito que impede carregar dados reais no armazenamento colunar: inserções sucessivas falham quando os dados contêm valores de texto grandes (acima de ~2 KB), com a tabela podendo ficar ilegível após o erro. O problema passou despercebido até agora porque a amostragem de benchmark usava uma fatia estreita do dataset, sem valores desse tamanho. Documentado em `docs/benchmarks/clickbench-scale-gate-2026-07-24.md`, com o impacto sobre a leitura dos números anteriores declarado honestamente (#190)
- Corrigido um viés na amostragem do benchmark ClickBench que favorecia os nossos próprios números: a amostra usava as primeiras N linhas do dataset, que é ordenado por tempo — uma fatia temporal estreita, com menos valores distintos do que a realidade, justamente o cenário em que a aceleração colunar mais se destaca. A amostragem passa a percorrer o arquivo inteiro pegando 1 linha a cada K, cobrindo todo o período. A estratégia usada fica registrada no artefato de resultado, e a antiga continua disponível apenas para testes rápidos (#187)
- O filtro que impede mudanças só-de-documentação de rodar a esteira completa passa a valer também para pull requests, não apenas para pushes diretos. Sem isso, um PR cujo diff era apenas o CHANGELOG reabria os ~14 jobs do runner único (observado no PR #189) (#187)
- Mudanças apenas de documentação deixam de disparar a esteira completa de CI. O runner é único e serial — cada push acionava ~14 jobs de ~45min (cerca de 10h de fila) mesmo quando nada de código mudava, o que travava as publicações e validações reais atrás de trabalho inútil (#187)

### Security


## [0.139.0] - 2026-07-24

### Added

- A imagem Docker do TheoDB passa a ser publicada no GitHub Container Registry a cada release: `docker pull ghcr.io/usetheodev/theo-db:latest` — o comando que o README e o quickstart já documentavam — agora funciona. Antes nenhum workflow publicava (o CI construía a imagem só para rodar os testes e a descartava), então o primeiro comando da documentação falhava com `manifest unknown`. A publicação valida a própria imagem antes de concluir: baixa, sobe o banco e executa o fluxo do README (extensão, tipo vector, índice ANN) (#187)



### Fixed

- Esteira de CI: cada push empilhava um run novo por workflow e nenhum era cancelado, entupindo a fila do runner (32 runs ativos, processados um a um). Os workflows passam a declarar `concurrency` por workflow+ref, cancelando runs superados da mesma branch — em `main` nunca se cancela, para não abortar um release em andamento

## [0.138.0] - 2026-07-24

### Added

- Aliases de compatibilidade pgvector para índices: `CREATE INDEX ... USING hnsw (col vector_cosine_ops)` — a sintaxe que toda aplicação pgvector escreve — passa a funcionar, junto com `vector_l2_ops` e `vector_ip_ops`. Com isso a migration versionada real do `theo-memory` aplica **inteira** contra o TheoDB sem alterar uma linha da aplicação (antes quebrava na criação do primeiro índice). Os aliases apontam para o mesmo índice ANN own-code do TheoDB — nenhuma implementação nova (#182)

## [0.137.0] - 2026-07-24

### Added

- Extensão `vector` de compatibilidade: o `CREATE EXTENSION IF NOT EXISTS vector` que aplicações pgvector executam no bootstrap (drizzle, alembic, prisma) passa a funcionar contra o TheoDB, e a imagem instala a dependência em `template1` para que bancos criados depois também o aceitem sem `CASCADE`. Tabelas com colunas `vector(N)` são criadas normalmente. O tipo e os operadores continuam sendo implementação própria do TheoDB (não é o pgvector — o `\dx` declara isso explicitamente). **Compatibilidade ainda parcial:** índices declarados como `USING hnsw (... vector_cosine_ops)` continuam falhando, porque esses nomes de método/opclasse ainda não existem — migrations com índices ANN exigem os nomes do TheoDB até o #182 ser resolvido. Descoberto pelo dogfood, que revelou que nenhuma aplicação conseguia sequer inicializar (#181)

## [0.136.0] - 2026-07-24

### Added

- Benchmark do M147 (`docs/benchmarks/m147-ab-byte-identical.md`) + harness de QPS (`theodb_rs/isolation/qps_bench.sh`): prova que o refactor de dispatch de `scan.rs` é byte-idêntico nos 6 caminhos IVF (v3..v8) e neutro em QPS (~−1%, o dispatch lê o bloco 0 uma vez em vez de 5×) (#170)


- Harness A/B do M147 (`theodb_rs/isolation/ab_scan_versions.sh`) + baseline capturado (`docs/benchmarks/m147-ab-baseline.txt`): constrói um índice IVF em cada versão v3..v8 com dataset determinístico e diffa o top-k (id:dist) do binário contra o baseline pré-refactor — a prova de comportamento byte-idêntico do refactor. Determinismo provado (compare contra si mesmo → OK) (#170)

- Plano de implementação do M147 (`m147-scan-version-dispatch`, SHIPPABLE_WITH_CAVEATS 89): 4 tasks TDD para o refactor de `am/scan.rs` em 3 eixos (enum de versão lido uma vez, gathers→Result+?, kernel Stage-1 compartilhado recebendo codes_off) + A/B byte-idêntico nos 6 caminhos v3..v8; 3 MUST-FIX do edge-case-plan absorvidos (v3 no A/B, ivf_version estrito+gate de len, baseline capturado) (#170)

- Blueprint de discovery do M147 (`m147-scan-version-dispatch`, SHIPPABLE_WITH_CAVEATS 89): padrão comprovado de version-dispatch de formato on-disk extraído de pgvectorscale (dispatch OCP por enum, tipo lido uma vez, decode isolado por-impl), lance (isolamento de corpos em módulo `previous/` + recusa fail-closed = a ADR-2 na prática) e pgvector (o contraste single-version) — decide a forma do refactor de `scan.rs` sem violar a ADR-2 do M145 (#170)

- Discovery plan do M147 (`m147-scan-version-dispatch`): investigação focada de como pgvectorscale (par pgrx), lance (formato versionado Rust) e pgvector (C) fazem dispatch de versão de formato on-disk OCP e isolam corpos de decode por-versão — insumo para o refactor de `scan.rs` sem violar a ADR-2 do M145 (#170)


## [0.135.0] - 2026-07-24

### Added

- Roadmap amended: added M146 Remediação do review-cycle theodb_rs (hardening + tests + cleanup) (`/roadmap-feature theodb-review-remediation`) (#168, #169)
- Roadmap amended: added M147 Refactor scan.rs version-dispatch IVF/AQ (`/roadmap-feature theodb-review-remediation`) (#170)
- Teste de regressão dos **gates de injeção de SQL** no `cassert-sql-safety`: 5 sondas (um eixo do `graph_build` + os três do `recommend_ef` + a superfície `_scan_stats`) usando o mesmo oráculo `1/0` do repro original. O `/review` apontou que nenhum dos dois fixes tinha teste — existiam só como medição numa mensagem de commit, então apagar o gate deixaria tudo verde. Não-vacuidade provada removendo o gate e reconstruindo: exit 1, com os dois eixos `tbl` acusados (M146, #172, #168)
- `theodb_rs/isolation/crash_parquet.sh`: prova de crash da durabilidade do export Parquet — crash imediato logo após o `write_parquet` retornar, com verificação de que o arquivo publicado continua íntegro (magic `PAR1` no início e no fim), sem temp órfão, e relegível pelo `read_parquet`. O plano exigia essa prova em quatro pontos e ela nunca havia sido executada; o `/review` apontou a lacuna. O harness declara no próprio cabeçalho o que **não** consegue provar: um `pg_ctl -m immediate` mata o processo, não a máquina, então não distingue o efeito do `fsync` do page cache do kernel (M146)
- `theodb_rs/isolation/Makefile` ganhou o alvo `check-corrupt`: o harness de injeção de corrupção não tinha chamador nenhum — a prova existia e ninguém a executava (M146)
- Teste de regressão do #177 no `cassert-sql-safety` (o job de CI que roda SQL real): um scan sem `ORDER BY <->` tem de falhar alto, nunca devolver linha vazia. Não-vacuidade provada nos dois sentidos — com o comportamento antigo simulado o script sai com exit 1; com o guard ativo, exit 0 (M146, #177)
- Harness de injeção de corrupção `theodb_rs/isolation/corrupt_index.sh`: corrompe bytes de um arquivo de índice real e verifica que nenhuma corrupção derruba o backend — a única prova possível dessa propriedade, já que teste unitário não toca página real. Parametrizado por AM (`AM=theodb_hnsw` default, `AM=theodb_ivfflat`), então a mesma propriedade é medida nos dois decodificadores (M146)
- `theodb_rs/examples/ivf_codec_check.rs`: cobre a aritmética pura do formato IVF (codificação de labels de largura fixa e cálculo de span com straddle de chunk), que não tinha teste algum. Executa de verdade — `cargo test`/`cargo pgrx test` não linkam neste crate, então o binário de example é o teste, seguindo a convenção que `examples/resumable_check.rs` já estabelecia (M146)


### Fixed

- `theodb.write_parquet` agora grava de forma **durável**, não apenas atômica: o arquivo temporário recebe `fsync` antes do rename e o diretório-pai recebe `fsync` depois, seguindo o protocolo `durable_rename` do PostgreSQL. Antes, um crash logo após o rename podia deixar um arquivo publicado porém truncado (M146)
- Desserialização do índice HNSW persistido passa a validar que todo índice de vizinho aponta para um nó existente, fechando a invariante que o próprio código declarava mas não cumpria (defense-in-depth — a análise de alcançabilidade medida está no comentário da função) (M146)
- Desserialização do índice HNSW também valida `max_level`: um valor corrompido fazia a busca varrer bilhões de níveis vazios sem `CHECK_FOR_INTERRUPTS`, ou seja, uma query impossível de cancelar com Ctrl-C (M146)
- Uma consulta com dimensão errada contra um índice IVF-AQ **íntegro** era reportada como `XX002 index_corrupted` — o SQLSTATE que o operador usa para decidir "REINDEX agora", e o REINDEX não ajudaria. `build_lut16` falha por duas causas distintas (codebook vazio = armazenamento; dimensão divergente = entrada do chamador) e a primeira versão da taxonomia colapsou as duas. Agora a dimensão errada sai como 22023, como já saía no HNSW (M146)
- Uma página truncada podia fazer `read_record_at` panicar (`XX000 internal_error`) em vez de emitir o erro tipado de corrupção — exatamente o que a passagem de taxonomia deste milestone existia para eliminar (M146)
- Falha de `fsync`/`rename` no export Parquet agora retorna **SQLSTATE 58030 (`io_error`)** em vez de 22023 (`invalid_parameter_value`). Um `fsync` que falha é o sinal mais forte de perda de dados que o kernel emite; rotulá-lo como erro de parâmetro convidava ao retry errado (M146)


### Security

- **`SELECT count(*)` retornava 0 num índice vetorial — resposta errada, sem erro.** Um scan sem `ORDER BY <->` saía silenciosamente do `amrescan`, o heap de candidatos ficava vazio e o executor concluía que a tabela não tem linhas. Medido em PG 18.4 com `enable_seqscan=off` numa tabela de 500 linhas: B-tree devolvia 500, o índice `theodb_hnsw` devolvia **0**, com plano `Index Only Scan` e nada no log. Agora falha alto com erro tipado e acionável — no `amgettuple`, que é onde o executor de fato pede linhas, e é onde o pgvector coloca o guard equivalente há anos (`src/hnswscan.c:214`); pôr no `amrescan` seria inovar onde o upstream já respondeu, já que o `amrescan` pode ser chamado em caminhos que nunca puxam tupla. Verificado que VACUUM, VACUUM FULL, ANALYZE, REINDEX, `ambulkdelete`, `aminsert` pós-vacuum e UPDATE seguem passando, e o top-k vetorial continua respondendo. Defeito **pré-existente**, não introduzido por este milestone (M146, #177)
- **`theodb.recommend_ef`/`theodb_rs._scan_stats`: o terceiro eixo de injeção continuava aberto.** A correção anterior fechou `qvec` e `col` e eu declarei a função segura — estava errado: `tbl` seguia interpolado cru nos três construtores de query, e o mesmo oráculo `1/0` provava execução. Agora resolvido via `($1)::regclass::text`, o mesmo mecanismo do `graph_build`. Corrigir dois de três eixos e declarar vitória é pior que não corrigir nenhum, porque aposenta a suspeita (M146, #172)


- **`theodb.scan_stats` e `theodb.recommend_ef` executavam SQL arbitrário fornecido pelo usuário**, por dois eixos independentes, ambos comprovados por `ERROR: division by zero`: o vetor de consulta era interpolado entre aspas simples cruas (`'{qvec}'`) e o nome da coluna entre aspas duplas cruas (`"{col}"`). O vetor passa a ser escapado por `quote_literal` e o nome da coluna passa por validação fail-closed de identificador na fronteira, retornando 22023 (M146, #172)
- **`theodb.graph_build` executava SQL arbitrário fornecido pelo usuário.** O nome da relação era interpolado cru (`%s`) na query de varredura, então um valor como `(SELECT ... WHERE 1/0 = 1) x` tinha seu SQL executado — comprovado por `ERROR: division by zero`. A relação passa a ser resolvida via `::regclass`, que valida e falha com 42P01 **antes** de qualquer SQL ser montado. Impacto limitado: a função é SECURITY INVOKER e tem `REVOKE ALL ... FROM PUBLIC`, executando com os privilégios de quem chama (M146, #168)

## [0.134.1] - 2026-07-23

## [0.134.0] - 2026-07-23

### Added
- Cadeia de upgrade `1.1.0→1.2.0` (`theodb_rs--1.1.0--1.2.0.sql`, full-schema self-healing conforme a convenção M137/oráculo CONV) que expõe a superfície lakehouse M143 (`public.read_parquet`/`write_parquet`/`olap` + REVOKEs least-privilege) a quem faz `ALTER EXTENSION theodb_rs UPDATE` — antes só o fresh-install a tinha; `default_version` → `1.2.0` (M144 T1.1, review HIGH)


### Fixed
- `theodb.vectorizer`: hardening do caminho de delete de embedding — os dois braços de `_vectorizer_process_delete` deixam de descartar o `Result` do SPI (`let _ =`) e passam a propagar a falha (o worker já a converte em dead-letter via M132), garantindo que um delete falho nunca seja marcado `done`. Defense-in-depth: no pgrx 0.19 um erro de DML já fazia longjmp (a falha já abortava o job), mas o Result descartado deixava aberto o caminho raro de `SpiError`-code; agora fechado e consistente com o braço de upsert (M144 T1.3, review HIGH)
- Tabela `theodb_columnar`: `INSERT` seguido de `DROP TABLE` na mesma transação agora faz COMMIT limpo — o flush de PRE_COMMIT usa `try_relation_open` e pula OIDs já dropados, em vez de abortar o COMMIT inteiro do usuário (M144 T2.1, review MEDIUM)
- Grafo nativo: node-ids acima de `u32::MAX` na construção do CSR agora falham com erro tipado, em vez de truncar silenciosamente e corromper a adjacência (M144 T2.4, review MEDIUM)
- Test-infra: `cargo pgrx test` volta a rodar (estava quebrado — a compilação do criterion bench `scan_hot_path` sob a feature `pg_test` falhava no link contra símbolos do PostgreSQL, bloqueando TODOS os `#[pg_test]` do crate). Fix: `test = false` no `[[bench]]` (o bench é para `cargo bench` standalone, não para o harness de teste) + extração do código `crate::ann`-dependente de `ann/scan_core.rs` para `ann/scan_core_mem.rs`, deixando `scan_core.rs` puro como o próprio módulo documenta (M144 fix de infra pré-existente)


### Security
- `symqg_spike_bench` (função de spike/benchmark que lia path arbitrário do filesystem via `std::fs::read`) agora é superuser-only (`REVOKE ALL FROM PUBLIC`, aplicado no fresh-install e via o script de upgrade para installs existentes) — antes era executável por qualquer role. Least-privilege canônico do PostgreSQL core (`pg_read_file`/`lo_import`); mantém a função no `.so` para não quebrar a cadeia de upgrade (M144 T1.2, review HIGH)
- `theodb.vectorizer`: a redação de credenciais em `last_error` (`sanitize_error_text`) agora é robusta a caracteres Unicode cujo lowercase muda de comprimento (ex.: `İ`) — antes o desalinhamento entre o vetor original e o lowercased desalinhava a redação e deixava caracteres residuais da credencial (ex.: o prefixo `sk`) no texto persistido. Correção de robustez da redação (nenhum vazamento do segredo inteiro foi encontrado em 48+ inputs brute-forced; o fix usa um único espaço de índice via `to_ascii_lowercase`) (M144 T2.2, review MEDIUM)

## [0.133.0] - 2026-07-23

### Added
- Roadmap amended: added M144 Remediação P0+P1 do code-review (upgrade chain 1.1.0→1.2.0, REVOKE do `symqg_spike_bench`, delete engolido do vectorizer + MEDIUMs P1 sob TDD) (`/roadmap-feature review-findings-remediation`)
- Roadmap amended: added M145 Refactor dos 4 hotspots de CC refactor-worthy com prova A/B e válvula honest-negative (`/roadmap-feature cc-hotspots-refactor`)

## [0.132.0] - 2026-07-22

### Added
- Docs (`docs/features/`): 7 novos docs de feature cobrindo os pilares que faltavam, todos verificados contra o código real (assinaturas/reloptions com file:line, banner em API-alvo/não-shipped, perf só com link a benchmark): grafo nativo/GraphRAG (13), analítico colunar `theodb_columnar` (14), lakehouse Parquet own-code (15), vectorizer declarativo (16), índice SymphonyQG (17), motor lexical BM25 (18), quantização vetorial (19). Documentação de features passa de 11 → 18


### Fixed
- Docs (`docs/features/14-analitico-colunar.md`): seção do seqscan corrigida — um `SELECT` plano decodifica todas as colunas (sem projeção no TAM) e é medido paridade-ou-mais-lento que heap ([`m99-columnar-tam.md`](docs/benchmarks/m99-columnar-tam.md)); o ganho de projeção/vetorização é exclusivo do caminho `CustomScan` M100. A afirmação anterior ("decodifica apenas as colunas projetadas") era falsa
- Docs (`docs/features/14-analitico-colunar.md`): bloco "Caveats honestos" agora divulga o contrato DML append-only — `UPDATE`/`DELETE`/tuple-lock/parallel/bitmap/sample/TID-range/`CREATE INDEX` falham com erro tipado em tabelas `theodb_columnar`; bitmap scan é desviado pelo planner (callbacks `NULL`) em vez de errar (caveat 4). Regressão coberta por `scripts/docs-features-lint.sh`
- Docs (`docs/features/13-grafo-nativo.md`): exemplo de `graph_expand` quebrava copiado verbatim — a função `RETURNS SETOF bigint` e a coluna default chama-se `graph_expand`, não `node`; exemplo agora aliasa a SRF (`AS t(node)`). Regressão coberta por `scripts/docs-features-lint.sh`

## [0.131.2] - 2026-07-22

## [0.131.1] - 2026-07-22

### Fixed
- Build: `Makefile` `PARTS` estava sem `sql/85-theodb-htap.sql`, divergindo do `Dockerfile` — o install base gerado por `make theodb-build`/`make install` ficava sem as funções `theodb.htap_refresh`/`theodb.olap` (a imagem Docker shipada nunca foi afetada; drift só no caminho de build local/regress)

## [0.131.0] - 2026-07-22

### Added

- Lakehouse Parquet own-code no default (M143, Fase 1): `theodb.read_parquet(path)`→SETOF jsonb (arrow-json, todos os tipos) + `theodb.olap(path)` tipado (paridade M62), via DataFusion/Arrow **sem DuckDB**; feature `spike-parquet` promovida a permanente
- `theodb.write_parquet(rel, path)` own-code (M143, Fase 2): materializa uma tabela PG em Parquet via `parquet::arrow::ArrowWriter` (escrita atômica temp+rename); round-trip write→read→olap validado sem DuckDB; tipo não-suportado na escrita → erro tipado fail-closed
- Superfície M62 reescrita own-code (M143, Fase 3): `theodb.htap_refresh(rel)` (escreve snapshot own-code + registra) e `theodb.olap(rel)` (lê+agrega own-code) — colapsam o codegen do pg_duckdb (o motivo do codegen, "DuckDB não roda em função", some com own-code). Sem `duckdb.query`/`COPY parquet`/guard. Extensão `theodb` bumpada 1.6 (`theodb--1.5--1.6.sql`)

- Spike (Fase 4) do leitor Parquet own-code (`theodb.read_parquet_agg_spike`, atrás da feature `spike-parquet`, off no build default): lê Parquet externo via DataFusion/Arrow (Apache-2.0, sem DuckDB) — veredito **VIÁVEL**, paridade byte-a-byte vs `pg_duckdb.read_parquet` a +9 MB no `.so` vs 118 MB do bundle DuckDB (`docs/benchmarks/parquet-reader-owncode-spike.md`)
- Roadmap amended: added M143 Remoção total do `pg_duckdb` (lakehouse Parquet own-code no default) (`/roadmap-feature pgduckdb-total-removal`)


### Removed

- **BREAKING:** `pg_duckdb` removido por completo (M143) — o último componente C++/httpfs do projeto. O lakehouse (ler/escrever/agregar Parquet externo) é agora own-code (DataFusion/Arrow); +9 MB no binário vs os 118 MB do bundle DuckDB. As funções codegen `theodb.htap_refresh_sql`/`olap_sql` (retornavam texto pg_duckdb) foram substituídas por `theodb.htap_refresh(rel)`/`theodb.olap(rel)` own-code (ADR-0057)


### Security

- Least-privilege nas primitivas de lakehouse (M143 review HIGH): `public.read_parquet`/`write_parquet`/`olap` têm `REVOKE ALL FROM PUBLIC` (escrita/leitura de arquivo server-side é superuser-only, como `COPY … TO file`) — um role sem privilégio não escreve/lê arquivo arbitrário. `block_on` sob `HeldInterrupts` (longjmp-safe) e leitura limitada por `work_mem` (GreedyMemoryPool, sem OOM); `write_parquet` usa nome canônico via `$1::regclass` (injection-safe) + temp único por-backend

## [0.130.0] - 2026-07-22

### Added

- Roadmap amended: added M142 Tier-out do `pg_duckdb` (imagem default enxuta + imagem opcional `theodb-htap`) (`/roadmap-feature pgduckdb-htap-tiering`)
- Imagem opcional `theodb-htap` (`packaging/Dockerfile.htap`) = imagem default + `pg_duckdb`, para o lakehouse de arquivos externos (Parquet/Iceberg/CSV) (M142)


### Fixed

- Imagem default voltou a buildar: `PGRX_VERSION` `0.16.1`→`0.19.0` e `RUST_VERSION` `1.91.0`→`1.97.1` no Dockerfile (o crate `theodb_rs` foi para pgrx `=0.19.0` no M98, que exige cargo-pgrx 0.19 + rustc ≥ 1.96; o Dockerfile nunca acompanhou) (M142)

## [0.129.0] - 2026-07-22

### Added
- **M140.4** (MVCC/VACUUM/crash + consumidor theo-lens — em progresso): probe de thread-safety **#153** — `theodb_lexical::probe::ThreadRecordingStore` (test-only, núcleo pgrx-free) registra as threads que chamam o `SegmentStore` num build real do Tantivy e prova a **separação estrutural**: como o store no caminho das threads vive no crate pgrx-free (não linka pgrx), é impossível por construção tocar o PG de qualquer worker thread — uma regressão que ponha SPI numa thread teria de sair do núcleo (pega no gate zero-pgrx + review). 2 testes, `cargo test -p theodb_lexical` verde. **Robustez provada contra o binário shipado** (`scripts/m140-4-lexical-robustness.sh`, `docs/benchmarks/m140-4-robustness-consumer.md`): **CRASH_OK** (bm25_build commitado sobrevive a SIGABRT+WAL replay), **VACUUM_OK** (n_dead_tup 24→0 após rebuilds, busca intacta), **MVCC RR+RC** (leitor snapshot antigo não vê build de outra sessão em RR; vê no próximo statement em RC — cache invalida correto). Fecha o M140.3 review LOW (straddle SPI): read_generation+load são read-only → mesmo snapshot da statement (ADR-0055 D3). **Consumidor theo-lens** (`scripts/m140-4-consumer-theolens.sh`, **CONSUMER_OK**): o shape real do theo-lens (input||output do span, hoje ts_rank) sobre `bm25_search` retorna o trace correto (termo distintivo, tool Claude Code, query natural) + wiring `theo-lens/.../trace-bm25-search.ts` testada (4/4 unit, mock db, roda no CI do theo-lens; ts_rank default intocado). O cutover de produção + 30 dias = M141 (dogfood). ADR-0055. Review council-rust-pgrx (NEEDS_FIXES→corrigido): **HIGH** — o claim de que `Spi::get_one` seria read-only era FALSO (em pgrx 0.19 é `connect_mut`/`update` → marca a txn mutável → snapshot fresco por statement, reabrindo o straddle, E quebra em read replica). Corrigido: `read_generation` usa `Spi::connect+c.select` (read-only genuíno) → fecha o straddle de verdade + `bm25_search` roda em replica sem burn de XID. + crash gate agora gateia crash-pegou+recovery (não só data-presente); VACUUM gate exige recuperação real; probe multi-thread assere >1 thread. **FECHA O M140** (M140.1→M140.4)

## [0.128.0] - 2026-07-22

### Added
- **M140.3** (engine BM25 de produção own-code — em progresso): a superfície de produção sobre heap (ADR-0052): (1) `theodb_lexical::IndexCache` (lógica pura pgrx-free, `cargo test` stock) que invalida por **geração** — mata o reload-por-query do spike M139 — MVCC-correto (leitor com snapshot antigo lê geração antiga e reconstrói do estado que enxerga); 5 testes. (2) `bm25_build(index_id, table, id_col, text_col)` (indexa tabela real id+body no heap, geração bumpada) + `bm25_search(index_id, query, k)→(id, score)` (lê geração sob snapshot, usa o cache). **Validado em PG18 real** (`scripts/m140-3-bm25-smoke.sh`, via `cargo pgrx install`): 9/9 asserções incl. o **crux MVCC** (sessão A com snapshot antigo NÃO vê o build de B). **nDCG@10 in-PG scifact = 0,6611** (reproduz o M140.1 byte-a-byte; paridade com pg_textsearch, bate ts_rank). **Latência cache-vs-reload MEDIDA** (`docs/benchmarks/m140-3-bm25-engine.md`, sweep): o cache elimina o reload-por-query, ganho escala com N — ratio cache/reload 0,62 (N=2k) → **0,36 (N=10k)** → **0,22 (N=50k, 4,5× mais rápido)**; o gate `<50%` é atingido em N≥~5k (o regime realista do theo-lens). ADR-0054 supersede a exceção `pg_textsearch` do ADR-0013 (own-code é a superfície BM25; paridade medida + índice menor + moat, não superioridade de ranking). Review council-rust-pgrx (NEEDS_FIXES→corrigido): recuperação de poison do `Mutex` do cache (`unwrap_or_else(e.into_inner())` — evitava que um panic transitório envenenasse `bm25_search` para toda a sessão do backend) + typed errors nos caminhos de heap; MVCC-core/thread-safety-#153/SQL-injection aprovados

## [0.127.0] - 2026-07-22

### Added
- **M140.2** (crate núcleo lexical pgrx-free): o núcleo do motor lexical (o `Directory`/`MemStore`/`SegmentStore` do spike M139) extraído para o crate próprio **`theodb_lexical`** (rlib, dep só `tantivy`, **sem pgrx**) — testável com `cargo test` stock, sem o link de símbolos PG que prendia os 6 testes (M139). `theodb_rs` vira workspace root e consome o núcleo atrás da feature `spike-lexical`; imports trocados (`crate::lexical::pg_directory::` → `theodb_lexical::`). Gate objetivo do pgrx-free: `cargo tree -p theodb_lexical | grep -c pgrx == 0`. CI (`lint-rust.yml`) agora roda o teste do núcleo + o gate zero-pgrx + `cargo check --features spike-lexical` (o cdylib consumindo o núcleo). ADR `docs/adr/0053` reconcilia com o ADR-0009 (superfície SQL única — outra camada, zero externs). 6 testes do núcleo verdes local

## [0.126.0] - 2026-07-22

### Added
- **M140.1** (medição lexical — gate de rigor do M140, **MEDIDO E CONCLUÍDO**): artefato reproduzível `docs/benchmarks/m140-1-lexical-measurement.md` + dados `docs/benchmarks/m140-1-data/` + ADR `docs/adr/0052`. Veredito: a **BM25 own-engine (Tantivy) bate o baseline `ts_rank_cd`** em retrieval lexical puro (o caso do theo-lens) em dois eixos — BEIR nDCG@10 (scifact 0,661 vs 0,072; nfcorpus 0,308 vs 0,206, reproduzindo o M138) e logs HDFS reais known-item (direção robusta em todo m; **magnitude honesta modesta** no regime justo m=1-2 ~9-13%, com o gap enorme de m≥3 declarado como **artefato de semântica de query**, não headline). Storage: índice Tantivy **~3,5× menor** no footprint enxuto (até 5× vs o baseline fiel theo-lens; apples-to-apples review H2) → **ADR 0052 decide heap buffer-then-flush** (AM custom rejeitado por over-engineering). Harness: `theodb_bench/{knownitem,logcorpus,lexical_engines}.py` + `run_m140_1_lexical.py` + gate offline `test_m140_1_decision.py`. 30 testes verdes, ruff limpo. Caveat declarado: corpus log-proxy público (LogHub), validação em traces reais é o boundary M140.4/M141


## [0.125.0] - 2026-07-22

### Added
- Spike M139 (gate 4 MEDIDO + VEREDITO GO): head-to-head Tantivy-in-PG vs `pg_textsearch` no mesmo corpus (2000 docs) — índice **2,8× menor** (68K vs 192K), latência competitiva (33 vs 40 ms). **Todos os 4 gates do spike medidos em PG18 real (index+search, MVCC, crash-real, custo) → veredito GO, SEM fork** (Tantivy MIT stock + Directory custom via buffer-then-flush+heap resolveu). Engine lexical própria é viável → M140 (M139)
- Spike M139 (gate 3 PROVADO — crash-real): `scripts/m139-lexical-crash-smoke.sh` — um índice lexical commitado sobrevive a `SIGABRT` no postmaster + replay do WAL (`search=1` antes e depois; "WAL replay ocorreu"). A crash-safety é herdada do heap WAL-logged do PG (buffer-then-flush), sem WAL/rmgr custom. **Gates 1–3 (viabilidade: index+search, MVCC, crash-real) todos provados em PG18 real** — a pergunta central do spike (Tantivy no PG com MVCC+WAL+crash?) é SIM (M139)
- Spike M139 (gate 2 PROVADO — MVCC): `theodb_rs/src/lexical/pg_backing.rs` (feature `spike-lexical`) — arquitetura buffer-then-flush (o Tantivy indexa multi-thread num buffer em memória; a main thread faz flush ao heap `theodb.lexical_files` bytea). **Medido em PG18 real:** round-trip buffer→flush→PG→load→search (`roundtrip('lazy')=1`) e MVCC cross-session (sessão B mede 0 durante txn A não-commitada, 1 após COMMIT — o DoD literal do gate 2). A integração transacional (a parte cara) funciona via heap MVCC+WAL do PG, sem página/WAL custom (M139)
- Spike M139 (gate 2, passo A): seam `SegmentStore` no `PgDirectory` (feature `spike-lexical`) — separa o contrato do trait `Directory` (pgrx-free) da fonte dos bytes; `MemStore` (gate 1) e o backend de páginas PG (gate 2/3, ADR 0051) plugam pela MESMA porta. 6/6 testes verdes standalone (M139)
- Spike M139 (gate 1 PROVADO): `theodb_rs/src/lexical/pg_directory.rs` — impl NOSSA do trait `Directory` do Tantivy (atrás de `spike-lexical`); teste `test_pg_directory_indexes_and_searches` verde (crate standalone pgrx-free) prova que o Tantivy indexa e busca sobre o `PgDirectory` **sem tocar o filesystem**; endurecido com 4 casos negativos/borda (erro tipado em arquivo/delete ausente, roundtrip+substituição de atomic_write, FileAlreadyExists em open_write duplicado — testing.md § 4.1). Achado: o núcleo lexical é pgrx-free → crate separado (direção M140). Gates 2–4 (MVCC/crash-real/custo) seguem (M139)
- Spike M139 (gate 1, atrás da feature `spike-lexical` — não entra no build shipado): `tantivy = "0.26"` (MIT) integra limpo na árvore de deps do `theodb_rs` (arrow 58/datafusion), 0 erros de build, e `cargo deny check licenses` verde (zero AGPL transitiva) — o primeiro gate do spike do `Directory`-sobre-PG retira o risco de integração da dependência (M139)

## [0.124.0] - 2026-07-21

### Added
- Gates mecânicos de qualidade Rust (M136): `lint-rust.yml` (clippy `-D warnings` com baseline de 21 categorias em `theodb_rs/.clippy_args` estilo neon, rustfmt `--check`, `cargo machete`, `metadata --locked`, doc) + `cassert-sql-safety.yml` (smoke dos 4 AMs + columnar sob Postgres `--enable-cassert` — a classe do crash #143 — via `scripts/cassert-smoke.sh`, e pgspot na SQL de instalação). Cada gate verificado verde por medição direta (`docs/benchmarks/m136-quality-gates.md`); backlog pré-existente em baseline-com-sunset (#151)


### Fixed
- Runner de CI self-hosted provisionado com o toolchain Rust+pgrx (rustc 1.97.1, cargo-pgrx 0.19.0, `cargo pgrx init --pg18` com `--enable-cassert`) para o usuário `ghrunner` — antes só Docker/shell estavam provisionados, então NENHUM job Rust rodava (o `license-gate` falhava silenciosamente). Agora os 3 gates Rust rodam verdes no CI (M136)

## [0.123.0] - 2026-07-21

### Added
- Workflow `ci-failure-notify` (M133/#140): dispara em `workflow_run` e, quando um CI conclui `failure` em develop/main, abre ou atualiza um issue de rastreamento (dedup por título) com link do run + SHA — para um CI vermelho, ou um CI que volte a morrer pré-step, aparecer **na hora** em vez de ficar 30 runs invisível como no #140 (M133)

## [0.122.0] - 2026-07-21

### Added
- Gate de CI `license-gate` + `theodb_rs/deny.toml`: a regra **D1** (nenhuma dependência AGPL na distribuição) passa de vigilância humana a gate de máquina, com allowlist fail-closed — licença desconhecida falha em vez de passar. Primeira execução: `licenses ok`, zero AGPL na árvore (M136)
- Medição decision-grade da fusão híbrida-com-BM25 vs híbrida-com-`ts_rank_cd` em BEIR (`benchmarks/run_m138_bm25_fusion.py` + `docs/benchmarks/m138-bm25-fusion.md`) — a medição que o M53 registrou como follow-up e nunca rodou. **Resultado (M138, honest-negative):** a fusão com BM25 **não** justifica trocar o default lexical — em scifact empata (nDCG@10 0,7418 vs 0,7337, p=0,51) e em NFCorpus (lexical-heavy) mede **significativamente pior** (0,3797 vs 0,3946, p=0,0168). A perna BM25 é 9,8× mais forte isolada, mas o RRF premia complementaridade com o vetor, não força bruta da perna. **O default lexical permanece `ts_rank_cd`**; `pg_textsearch` não é embarcado (M138)


### Fixed
- `benchmarks/theodb_bench/db.py::bm25_query`: usa a forma de 2 args `to_bm25query(texto, índice)` do `pg_textsearch` 1.3.1 — o bare `col <@> $bind` e o `to_bm25query($bind)` de 1 arg levantam "operator requires index" com bind parameter no PG18. Descobriu o bug irmão na fusão in-DB (issue #146) (M138)

## [0.121.0] - 2026-07-21

### Added
- `scripts/test-upgrade.sh`: harness reproduzível da cadeia de upgrade (cenário A, convergência, idempotência, cenário B1). Substitui provas que eram digitadas à mão — e que já haviam produzido duas leituras falsas; o harness aborta se o envelhecimento do catálogo não remover nada, fechando justamente esse modo de pass vacuoso (M137/F4)
- Gate de CI `schema-drift-gate`: falha um PR que altere a superfície da extensão (`theodb_rs/src/`) sem bumpar `default_version` nem adicionar script de upgrade — a mecânica que fez `1.0.0` rotular cinco catálogos diferentes ao longo de 120 releases continua ativa, e este gate a interrompe. Inclui a guarda que impede editar um script de upgrade cujo alvo já foi lançado (M137/F5)
- Oráculo de schema cobre também **privilégios** (`proacl`), não só membresia — um upgrade que perdesse um `REVOKE ... FROM PUBLIC` passaria batido no `pg_depend` (M137)
- Cadeia de upgrade do `theodb_rs`: `ALTER EXTENSION theodb_rs UPDATE TO '1.1.0'` passa a existir — pela primeira vez em 120 releases a extensão Rust é atualizável sem dropar e recriar. Inclui o script convergente `theodb_rs--1.0.0--1.1.0.sql` (gerado por `scripts/gen-upgrade-script.py`, idempotente) e o oráculo `schema_snapshot.sql` que compara o schema pós-upgrade com o de uma instalação limpa (M137)
- Roadmap ampliado com a fundação decidida em 2026-07-21 (`/roadmap-feature`, 6 milestones): **M136** gates mecânicos de qualidade + Postgres `cassert` no CI (torna o D1 gate de máquina via `deny.toml`); **M137** cadeia de upgrade do `theodb_rs` (94 funções `pg_extern` e zero scripts hoje — instalações não conseguem `ALTER EXTENSION UPDATE`); **M138** BM25 como perna lexical default, executando o gate de adoção já medido no M53 (`ts_rank_cd` 0,0703 vs BM25 0,6881 nDCG@10); **M139** spike-gate do `Directory` do Tantivy sobre block storage do Postgres; **M140** engine lexical própria sobre Tantivy (MIT) + crate núcleo sem pgrx, *gated* pelo M139; **M141** dogfood `running`, o que autoriza qualquer reivindicação de production-ready


### Fixed
- Corrupção silenciosa no script de upgrade (defeito e correção provados empiricamente sobre um shell type real: o guard antigo avalia `f` e nunca aplicaria a definição; o novo avalia `t`) do `theodb_rs`: o shell type e a definição completa do tipo `vector` compartilhavam o mesmo predicado de guarda, então num catálogo sem o tipo o shell era criado e a definição real **nunca aplicava** — o tipo ficava shell para sempre, sem erro. Guards agora diferem (`typisdefined`) e qualificam o namespace, evitando também ligar em cima de um `vector` de outra extensão (M137)

## [0.120.0] - 2026-07-21

### Added
- Roadmap ampliado: adicionado M135 — suporte a PostgreSQL 18 (migração 17 → 18), com custo medido por sondagem de compilação (27 erros mapeados por arquivo/linha), não estimado (`/roadmap-feature pg18-support`)


### Fixed
- **Crash de servidor:** operações não suportadas do `theodb_columnar` (ex.: `CREATE INDEX` sobre tabela colunar) abortavam o postmaster inteiro em vez de retornar erro tipado — 30 callbacks afetados. O panic do pgrx não encontrava frame de guarda e o unwinder saía da pilha (#143)
- Lint do harness de benchmark limpo (24 erros do `ruff` + falso-positivo do `vulture`), removendo um dos dois bloqueios do sinal de CI; o `entry_sql` acusado pelo vulture é uma fixture do pytest e foi ignorada por nome, não deletada — apagá-la desativaria o guard de skip-offline (#140)

## [0.119.0] - 2026-07-21

### Added
- Denylist de egresso SSRF no único ponto de saída HTTP das chamadas de IA: endpoints que resolvem para endereços loopback/privados/link-local (incl. `169.254.169.254`) são recusados com erro tipado `22023` nomeando o endereço, cobrindo `theodb.embed`, `ai._chat` e `theodb.rerank` de uma vez (#117)
- GUC `theodb.egress_allowlist` (superusuário) para permitir explicitamente um host on-prem que resolva para endereço privado, sem desabilitar a proteção inteira (#117)


## [0.118.0] - 2026-07-21


### Added
- Roadmap amended: added M132 fix #132 — vectorizer bgworker embeda no self-host (destrava o anchor de dogfood) (`/roadmap-feature vectorizer-worker-embed-fix`)
- Roadmap amended: added M133 fix #140 — restaurar o sinal de CI (todo job do Actions falha antes de qualquer step) (`/roadmap-feature ci-restore-signal`)
- Roadmap amended: added M134 fix #117 — SSRF cego via `theodb.llm_endpoint` setável pelo chamador (`/roadmap-feature llm-endpoint-ssrf-hardening`)

### Deprecated

### Removed

### Fixed
- **Vectorizer worker failures are now diagnosable; a zero-row batch no longer counts as success (M132, closes #132).**
  A failing job recorded the blanket literal `embed/upsert failed` for every cause — a 401, a missing embedding GUC
  and a malformed response were indistinguishable — because the subtransaction helper caught with
  `catch_others(|_| None)` and discarded the error. The caught **SQLSTATE + message** are now returned and stored in
  `last_error` (passed as a **bound parameter**, never interpolated into SQL). The worker also logs its own view of
  the embedding config once at startup (`embedding_endpoint=set|MISSING … api_key_len=N` — the key **length** only,
  never the value), so a worker that booted without the `ALTER SYSTEM` GUCs is identifiable from one line instead of
  a debugger. Separately, `Some(0)` from the batch path used to take the success arm: a batch that ran cleanly but
  embedded **nothing** was counted as processed and its jobs consumed with no result and no failure signal — a
  zero-row batch now falls back to the per-job path, whose outcome is always observable.
  **Honest scope:** the symptom reported in #132 (all embed jobs dead-lettering on self-host) **does NOT reproduce**
  on the current build — a clean end-to-end embeds 5/5 fresh rows with the queue draining to 0 failures
  (`knowledge-base/discoveries/blueprints/vectorizer-worker-embed-blueprint.md`). This milestone ships the
  diagnosability that made the original report cost a day, not a fix for an absent defect.
  Hardened after review (council-security + council-rust-pgrx, both 0 BLOCKER/0 HIGH): the persisted cause is
  **sanitized at the sink** — credential-shaped runs (`Bearer …`, `sk-…`) are redacted and the text bounded inside
  `_vectorizer_mark_failed`, so a misconfigured echo endpoint reflecting `Authorization` headers into its 200 body
  can no longer write a token into a durable table row (logs rotate, dead-letter rows do not); a job is now counted
  as processed only when its **owner-guarded** `mark_done` succeeds, so a lease-lost job is never double-counted by
  two workers (the H1 fencing contract); the new startup diagnostic is subtransaction-isolated so it can never crash
  the worker it exists to diagnose; and both mark arms use bound parameters.

### Security

## [0.117.0] - 2026-07-21


### Added
- Roadmap amended: added M131 fix #135 — destravar o columnar-agg pushdown (planner hang em tabelas largas mixed-type) (`/roadmap-feature columnar-agg-planner-hang-fix`)

### Deprecated

### Removed

### Fixed
- **Columnar-aggregate CustomScan: EXPLAIN no longer hangs on `ORDER BY <aggregate>` (M131, closes #135).** The
  swapped `theodb_columnar_agg` CustomScan published a self-referential `custom_scan_tlist` (`Var(INDEX_VAR, i)`
  entries), so PostgreSQL's `ruleutils.c::resolve_special_varno` recursed forever whenever a `Sort` above the node
  had a key on the aggregate output — `EXPLAIN` never returned and was uninterruptible (`statement_timeout` does not
  fire during plan printing). `custom_scan_tlist` now carries deparse-safe expressions (group keys → base-rel `Var`s;
  aggregates → their `Aggref` with arguments rebuilt as base-rel `Var`s); the executed `plan.targetlist` is
  unchanged, so query results are untouched. Root cause established by a live gdb backtrace — the issue's
  "planner hang / O(cols²) / wide-table" diagnosis was **falsified**: the hang was in EXPLAIN deparse, not planning
  or execution (the affected query always executed correctly in 0.5 s), and the trigger is `ORDER BY <aggregate>`,
  not table width (`knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md`).
  MEASURED after the fix: the 43-query ClickBench EXPLAIN sweep with the pushdown ON goes from **2 hangs to 0**
  (max 60 ms; Q16 31 ms and Q33 30 ms, both now engaging the CustomScan — gate script `scripts/m131_sweep.sh`), and
  the accelerated ClickBench run is **byte-identical 43/43** vs heap while measuring **1.90× on the full-suite hot
  geomean** (0.896 s vs 1.700 s) and **24.8× across the six queries the pushdown touches** (20.7× excluding q6,
  which is served by the pre-existing zone-map directory fast-path, not the pushdown) — an internal same-box A/B
  (pushdown ON vs OFF), single suite run per configuration, on a self-hosted NOT-canonical box with a 100 k-row
  subsample, with the ±21 % noise floor of the 37 untouched queries disclosed; not a competitive claim
  (`docs/benchmarks/m131-columnar-agg-accelerated.md`). `custom_scan_tlist` is also the node's runtime scan
  TupleDesc, so the replacement list is descriptor-equal to the one it replaces and the construction is
  fail-closed (an inconsistent list declines the swap to the native plan).

### Security

## [0.116.0] - 2026-07-21

### Added
- **Official HTAP benchmark: CH-benCHmark via BenchBase over self-hosted TheoDB (M130, ADR-0050).** HTAP driver
  (`benchmarks/run_m130_htap.py`) + a container-entry script + BenchBase config (`benchmarks/htap/`, our `.sh` +
  `.xml` driving the Apache-2.0 BenchBase tool from a pinned SHA inside a Java-23 Docker container — no BenchBase
  source vendored/linked) + DB-free unit tests (`benchmarks/theodb_bench/test_htap.py`, 13 tests). MEASURED against
  self-hosted TheoDB PG17 across **3 sessions**: CH-benCHmark (TPC-C 45/43/4/4/4 mix + all 22 TPC-H-style analytical
  queries in one mixed phase) runs with **0% error** — mixed-workload throughput mean **116.46 req/s** (between-session
  CV 3.08%), derived **dual metric proxy tpmC-proxy 2994.5 / QphH-proxy 5116.8** (CV 3.4%; labeled proxy, NOT audited
  TPC). Proves the mixed **OLTP+OLAP wire-compatible gate** end-to-end. The retained OLAP result-consistency oracle
  RAN LIVE — **22/22 CH analytical queries PASS** against TheoDB. Honest finding recorded (with its own artifact):
  SERIALIZABLE isolation exhausted SSI predicate-lock shared memory (a documented PostgreSQL SSI limitation, not a
  TheoDB defect) → READ COMMITTED (the realistic HTAP isolation) runs clean. Self-hosted shared box (NOT canonical
  hardware) →
  functional baseline, not a competitive claim; BenchBase is Apache-2.0 run as an external Java-23 Docker driver;
  seed-determinism unconfirmed (`docs/benchmarks/m130-htap.md`).

## [0.115.0] - 2026-07-21

### Added
- **Official OLTP benchmark: pgbench + HammerDB TPROC-C over self-hosted TheoDB (M129, ADR-0050).** OLTP driver
  (`benchmarks/run_m129_oltp.py`) + HammerDB TPROC-C Tcl (`benchmarks/oltp/hammerdb_tproc_c.tcl`, our script driving
  the GPLv3 tool via its CLI — no HammerDB source vendored/linked) + DB-free unit tests
  (`benchmarks/theodb_bench/test_oltp.py`, 8 tests). MEASURED against self-hosted TheoDB PG17 across **3 sessions ×
  10 runs**, each persisted to its own artifact: **pgbench TPS means 1247.3–1328.3** with run-to-run **coefficient
  of variation 7.6–9.5%** (within-session) / **3.2%** (between-session) — the honest single-system dispersion metric
  the OLTP tools lack — + **HammerDB TPROC-C NOPM = 18 269** (real TPC-C 45/43/4/4/4 mix; a functional smoke, not
  claim-grade). Durability posture is **server-reported** (`SHOW fsync=on`, `synchronous_commit=on`), and every
  throughput number is paired with the retained crash-safety gate (ADR M129-2 — throughput without durability is
  meaningless; the OLTP tools do not enforce ACID). Proves the 100%-wire-compatible OLTP gate end-to-end. Honest:
  self-hosted shared box (NOT canonical hardware) → functional baseline, not a competitive claim; NOPM is NOT
  audited tpmC; pgbench is D1-clean (PostgreSQL License), HammerDB is an external out-of-tree Docker driver
  (`docs/benchmarks/m129-oltp.md`).

## [0.114.0] - 2026-07-20

### Added
- **Official ClickBench entry over theodb_columnar (M128, ADR-0050).** The ClickBench per-db contract (`benchmarks/clickbench/theodb/` — create/queries/glue + results) + a driver running the 43 ClickBench queries over a `theodb_columnar` table on real (subsampled) `hits`, reusing the wrap layer. MEASURED: 43/43 queries run, **byte-identical result A/B vs heap PASSES 43/43** (the correctness oracle ClickBench lacks — its `check` is a `SELECT 1`). Honest: self-hosted box + n=1000 subsample; `hits` is CC-BY-NC-SA (CI-only, never vendored). The vectorized-agg CustomScan pushdown hit a real planner-hang bug on the wide real hits table (filed #135) — the measured columnar-STORAGE path (agg off) is sound + complete; pushdown is tracked follow-up (`docs/benchmarks/m128-clickbench-columnar.md`).

## [0.113.0] - 2026-07-20

### Added
- **Official benchmark adapter for the vector pillar (M127, ADR-0050 pilot).** A TheoDB `BaseANN`-shaped ann-benchmarks adapter (`benchmarks/theodb_bench/ann_adapter.py`) + the reusable adopt-and-wrap layer (`regression.py` byte-identical A/B + the M123 `significance.py`) the official tools lack. MEASURED on real GloVe (D1-safe PDDL): recall@10×QPS Pareto (0.72→1.0 as ef 10→200) via `theodb_hnsw`, byte-identical A/B PASS, significance deterministic (`docs/benchmarks/m127-ann-benchmarks-vector.md`). Honest scope: self-hosted box + n=5000 subsample — canonical-box + full-corpus leaderboard PR is the operational follow-up (ADR M127-2).
- ADR-0050: official benchmark harness = ADOPT-AND-WRAP (not pure replace) — adopt the official per-pillar driver + datasets + leaderboard entry, retain a thin TheoDB analysis layer (significance + byte-identical regression + correctness gating) the official tools do not provide (`docs/adr/0050-official-benchmark-adopt-and-wrap.md`).
- Roadmap amended: added benchmark-official program M127 (vector) / M128 (columnar) / M129 (OLTP) / M130 (HTAP), vector-pilot-first, per the discovery blueprint (`knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md`, SHIPPABLE 97.5).

## [0.112.0] - 2026-07-20

### Added
- **Dogfood enabler: the theo-data retrieval anchor exercised end-to-end on a self-hosted TheoDB (M124).** A reproducible self-host quickstart (`docs/ops/self-host-quickstart.md`) + a re-runnable anchor smoke (`benchmarks/dogfood_anchor_smoke.sh`) that drives `theodb.create_vectorizer` → the vectorizer worker → `ai.hybrid_search_rrf`, with the QUERY path proven using **real** OpenAI embeddings — a genuine two-leg RRF fusion (a doc ranked by BOTH the FTS and vector legs scores 1/61+1/61=0.032787, above the single-leg ceiling; the smoke asserts both legs non-empty AND max_score>1/61). Dogfood manifest advanced `planned → wired`; first evidence recorded (`.claude/knowledge-base/dogfood/evidence/`) including two real failure stories (async vectorizer worker dead-letters embeds on self-host → #132; `create_vectorizer` does not backfill pre-existing rows). Honest: this is `wired`, NOT `running` — the production-ready claim stays unmade until sustained real use (≥30d, cross-repo).

## [0.111.0] - 2026-07-20

## [0.110.0] - 2026-07-20

### Added
- **Hybrid retrieval significantly beats vector on a lexical-favoring set (M125).** Extended the significance harness to report three paired comparisons (hybrid-vs-vector, hybrid-vs-fts, fts-vs-vector) + the fts leg's mean nDCG@10, so a parity is attributable (fusion value vs ts_rank-leg quality). MEASURED on NFCorpus (323 queries): hybrid nDCG@10 0.3950 vs vector 0.3845 → Δ̄=+0.0105, 95%CI=[+0.0027,+0.0188], p=0.0099 → **SIGNIFICANT** — resolving the H6 risk (M123's SciFact parity was a dense-strong set + a dead lexical leg, not 'hybrid never helps'). Small, regime-dependent, honest (`docs/benchmarks/m125-hybrid-lexical.md`).
- Roadmap amended: added M124 Dogfood real — capability theo-data sobre TheoDB self-hosted (gap-analysis Rec.1/H9)
- Roadmap amended: added M125 Significância da híbrida em dataset lexical-heavy — resolve H6 (gap-analysis Rec.2)
- Roadmap amended: added M126 Split do god-file hnsw_page.rs (3.456 LoC) — risco de manutenção (gap-analysis Rec.3)
- Dogfood production-readiness gate configured: anchor `theo-data-capability-on-theodb` (`rules/dogfood-golden-rule.md § 1` + `knowledge-base/dogfood/manifest.md`), honest status `planned` — no sustained real-use evidence yet, so no production-ready claim.

## [0.109.0] - 2026-07-20

### Added
- **Paired significance test for hybrid vs vector retrieval on BEIR (M123).** `theodb_bench.significance.paired_significance`
  — paired permutation p-value (Smucker CIKM2007) + paired-bootstrap 95% CI on the mean per-query nDCG@10 difference
  + paired t-test cross-check + wins/losses/ties; wired into `run_m53_hybrid_beir.py` (pre-declared endpoint: nDCG@10,
  hybrid vs vector). MEASURED on SciFact (300 queries): Δ̄=+0.0041, 95%CI=[−0.0010,+0.0108], p=0.25, 296/300 ties →
  **PARITY (not significant)** — the +0.004 mean is within noise (`docs/benchmarks/m123-hybrid-significance.md`). Also
  hardens the eval harness (OpenAI 429 backoff-retry; `theodb_rs` own-vector extension instead of stale pgvector).

### Deprecated

### Removed

### Fixed

### Security

## [0.108.0] - 2026-07-20

### Added
- Roadmap amended: added M122 Embed totalmente assíncrono no vectorizer (`/roadmap-feature async-embed-vectorizer`)
- Roadmap amended: added M123 Significância estatística pareada do hybrid vs vector — BEIR (`/roadmap-feature hybrid-beir-significance`)

### Deprecated

### Removed

### Fixed
- **Vectorizer embed no longer pins the xmin horizon (M122).** The background worker now embeds each in-place
  (1→1) batch in a 3-phase split — read+lease (txn) → **HTTP embed with no open transaction** → write+mark (txn)
  — so a slow/hung embedding endpoint no longer holds a transaction snapshot (and thus `backend_xmin`) for the
  whole round-trip, which previously delayed local autovacuum by up to the embed timeout (~90s). Source-proven
  (pgrx `BackgroundWorker::transaction` holds an active snapshot for its whole closure) + measured (worker
  `backend_xmin` 0/28 held during a real 8s embed): `docs/benchmarks/m122-async-embed-xmin.md`, ADR-0049.
  Crash-safety unchanged (at-least-once re-embed via lease expiry; idempotent owner-guarded write). Chunk-mode
  (M66) keeps the single-txn path (documented drawback). Adds `theodb.vectorizer_single_txn` GUC (default off) as
  an operator kill-switch + A/B apparatus.

### Security

## [0.107.0] - 2026-07-20

### Added
- **Fail-closed structured `filter` for `ai.hybrid_search` (M120)** — a `[{col, op, value}]` predicate composed with
  `quote_identifier` + `quote_literal` + an operator allowlist (`= < > <= >= <> IN &&`); an un-allowlisted operator
  or a `filter`+`filter_sql` combination raises SQLSTATE 22023 (fail-closed). Closes council-security F1: the raw
  `filter_sql` (retained as an opt-in documented caller-privilege escape hatch) was a syntactic blacklist, not a
  parser — the structured path is the only fail-closed option for untrusted/multi-tenant callers. Validated in-PG:
  parity with `filter_sql`, bad-op → 22023, injection value (`DROP TABLE`) quoted-as-literal → table survives
  (`docs/security/m120-fail-closed-filter.md`).
- Roadmap amended: added M120 Filtro estruturado fail-closed para `ai.hybrid_search_rrf` (`/roadmap-feature hybrid-fail-closed-filter`)
- Roadmap amended: added M121 IVF cosine/ip spherical k-means — recall quality (`/roadmap-feature ivf-spherical-kmeans`)

### Deprecated

### Removed

### Fixed

### Security

## [0.106.0] - 2026-07-20

### Added
- **`NOTICE` file** na raiz agregando a atribuição das extensões permissivas (PostgreSQL License)
  redistribuídas na imagem — `pgvector` e `pgvectorscale` (refs pinadas) — para satisfazer a
  obrigação da PostgreSQL License de que o aviso de copyright apareça "in all copies". Complementa
  (não duplica) a due-diligence AGPL em `docs/packaging/license-audit.md`. Resultado de uma auditoria
  de proveniência/similaridade `loop-check-licence` (veredito **CLEAN**: 100% cobertura, zero cópia
  incompatível, zero lacuna de atribuição — `.claude/knowledge-base/audits/licence-compliance-2026-07-19.md`).
- Roadmap amended: added M116 Operabilidade em escala — eliminar o muro do VACUUM (`/roadmap-feature vacuum-wall-operability`)
- Roadmap amended: added M117 SIMD cosine/IP no hot path de embeddings (`/roadmap-feature simd-cosine-ip-kernels`)
- Roadmap amended: added M118 Filtered ANN eficiente — resume-from-discarded (`/roadmap-feature filtered-ann-resume-discarded`)
- Roadmap amended: added M119 AI-native depth — cross-encoder re-rank + chunking recursivo (`/roadmap-feature ai-native-depth-rerank-chunking`)
- **Filtered ANN resume-from-discarded (M118, T1.1+T2.1)** — the iterative HNSW scan now RESUMES from the retained
  beam frontier (pgvector 0.8.5 `ResumeScanItems` technique — the frontier IS the discarded set, never dropped)
  instead of re-searching the whole graph with a doubled `ef`, on the V1 (exact-f32) path. Validated in-PG:
  **recall@10 = 1.0** vs brute-force exact kNN under a selective filter (`Index Scan using theodb_hnsw`,
  `max_scan_tuples` armed). SBQ/AQ indexes keep the M52 re-search (per-batch rerank is a tracked follow-up).
  `ann/scan_core.rs::ResumableGround`, `am/hnsw_page.rs::{resumable_init,resumable_next}`, `am/scan.rs` wiring.
- **`theodb_hnsw.resume` GUC (M118)** — on|off kill-switch (default ON) for the resume-from-discarded filtered
  iterative scan; OFF reverts to the M52 re-search (operator escape hatch + own-path A/B baseline). V1 only.
- **`theodb_hnsw.resume_max_mb` GUC (M118 T2.2)** — memory ceiling (default 64 MB; `0` = disabled) for the
  resume scan's retained frontier; on overflow the scan stops resuming and returns what it holds (fail-safe,
  no panic — validated in-PG: `resume_max_mb=1` returns cleanly). Milestone M118 DoD **re-scoped** (owner-approved)
  after the ≤1.2×-vs-pgvector target was FALSIFIED by measurement (structural page-native gap, ADR-0033) — shipped
  on the measured own-path win (~1.95× vs the M52 re-search at matched recall). Review **READY_TO_MERGE**
  (`council-rust-pgrx` + `council-index-storage` clean; 2 LOW review-fixes applied: resume-loop `if let` +
  `resume_max_mb` doc caveat).

### Deprecated

### Removed

### Fixed

### Security

## [0.105.0] - 2026-07-19

### Added
- **Columnar `min(col)`/`max(col)` aggregate + zone-map directory fast-path — MEASURED
  (`docs/benchmarks/columnar-minmax-zonemap-verdict.md`).** Admits `min`/`max` on ordered native types (int2/4/8,
  float4/8, timestamp/date) in the columnar CustomScan byte-identical to PostgreSQL (output type = input column type,
  emitted via the `build_arrow` reverse). Adds a **directory-only fast-path** (Phase B) that answers a scalar
  `min`/`max` with no WHERE by folding the zone-map `min_bits`/`max_bits` already written per (chunk_group, col) — never
  decoding a column chunk. **VERDICT (1M rows, c-8): GOAL MET** — integer/temporal scalar min/max answer in **< 1 ms
  (~1300–1400× faster than the native scan)**, every shape byte-identical (as TEXT). MVCC-correct (verified by
  `council-index-storage`): folds only visible-stripe directory entries + scans same-xact pending rows (proven: an
  uncommitted `INSERT` is seen by `max()`). `max(float)` correctly falls back to the decoded scan (directory skips NaN
  while PG `max` returns NaN); GROUP BY / WHERE / empty / all-NULL all byte-identical. Scope: ordered native types
  (bool has no PG min/max aggregate; text/numeric deferred).

### Deprecated

### Removed

### Fixed

### Security

## [0.104.0] - 2026-07-19

### Added
- **Numeric-output integer aggregates — `sum(int8)` + `avg(int2/4/8)` byte-identical — MEASURED
  (`docs/benchmarks/numeric-output-aggregates-verdict.md`).** Extends the columnar aggregate CustomScan to admit the two
  integer aggregates M114 declined because their output is PG `numeric`. The columnar path computes
  `sum(cast(col AS Decimal128(38,0)))` (i128-exact) + `count(col)` in DataFusion and builds the PG numeric in Rust via
  pgrx `AnyNumeric` — for `avg`, `AnyNumeric(sum) / AnyNumeric(count)` delegates to PG's own `numeric_div`, so the
  DATA-DEPENDENT result scale (`select_div_scale`) is PG's, not a fixed-scale reimplementation. **VERDICT (1M rows,
  c-8): GOAL MET** — every shape a CustomScan AND byte-identical to the heap as TEXT: `sum(int8)` including a sum of
  **1e19 that exceeds i64 max** (proves the Decimal128/i128 path is load-bearing — a wrapping Int64 sum would go
  negative), and `avg(int2/4/8)` reproducing PG's shrinking avg scale byte-for-byte (**16 → 12 → 8** sig-digits as the
  sum grows). Empty group → SQL `NULL` (zero-count guard). Scalar + GROUP BY; 7.14–10.10× vs native (context, not a new
  perf claim). Scope: integer input only — `sum/avg(numeric)` column input deferred (needs Arrow `Decimal128` column
  decode).

### Deprecated

### Removed

### Fixed

### Security

## [0.103.0] - 2026-07-19

### Added
- **M115 — columnar-aggregate CustomScan composability — MEASURED
  (`docs/benchmarks/m115-columnar-composability-verdict.md`).** Fixes the pre-existing M100 limitation where consuming
  a columnar-aggregate output VALUE inside an enclosing expression (subquery / join / aggregate-`ORDER BY`) failed with
  `cache lookup failed for attribute N of relation 0`. Fix = **Agg-swap rearchitecture** (TimescaleDB `vector_agg`
  pattern): `admit` now STASHES its result at `upper_paths_hook` (no CustomPath added), `standard_planner` builds a
  normal `Agg` (whose output the parent references as plain Vars — no `Aggref` leaks), and a `planner_hook` swaps that
  `Agg` → our `CustomScan` post-`set_plan_refs` with a plain-typed-`Var` tlist. The swapped grouped result is
  ascending-sorted by the group keys (Rust-side) to reproduce a `GroupAgg`'s output order (`SORTED` text-key GroupAgg
  left native — collation safety). Reached after **three empirically-disproven CustomPath-level attempts** (the honest,
  non-workaround path). **VERDICT (1M rows, c-8): GOAL MET** — the four previously-failing shapes byte-identical +
  CustomScan; **no regression** (M114 aggregate breadth 5.89–12.69×, GROUP BY 4.57–9.87×, top-level all still columnar
  + byte-identical). Milestone M115.

## [0.102.0] - 2026-07-19

### Added
- **M114 — Columnar analytical aggregate completeness — MEASURED (`docs/benchmarks/m114-columnar-aggregate-verdict.md`).**
  Broadens the M100 columnar `CustomScan` to admit **GROUP BY combined with a WHERE** (zone-map skip + DataFusion
  Filter + hash aggregate in one plan), **`avg(float8)`**, and **`sum(int2/int4)`** — each byte-identical to the native
  plan. `df_executor::AggSpec` gains `SumInt`/`AvgFloat8` variants and `agg_datum` emits the exact PG output type per
  variant (`sum(int2/4)`→int8 = Arrow Int64, no overflow; `avg(float8)`→float8 = Arrow Float64); the grouped executor
  now accepts predicates + a filter (filter-before-aggregate). `admit` widens the sum/avg arg-type guards and runs
  `extract_all_predicates` in the grouped branch. The numeric-output shapes (`avg(int*)`, `sum(int8)`, `sum(float4)`,
  `avg(float4)`) **decline to the native plan** (which returns the exact numeric) — a byte-fidelity call backed by the
  blueprint's primary-source analysis (PG docs + DataFusion 54 source + Citus pattern), not a defect. **VERDICT (1M
  rows, c-8, warm): GOAL MET** — byte-identical + CustomScan for every shipped shape, **6.58×–12.99×** faster;
  declined shapes native + correct (proven by EXPLAIN + spot-check). Milestone M114.
- Roadmap amended: added M114 Columnar analytical aggregate completeness (GROUP BY+WHERE + avg/sum(int)) (`/roadmap-feature columnar-aggregate-completeness`)
- Roadmap amended: added M115 Composabilidade do M100 (saída columnar-agg usável em subquery/join) (`/roadmap-feature columnar-aggregate-completeness`)
- **Columnar/HTAP: GROUP BY pushdown — MEASURED (`docs/benchmarks/columnar-groupby-verdict.md`).** The M100
  `CustomScan` admitted only a scalar aggregate (one output row); this slice (plan `columnar-groupby-pushdown`) adds
  vectorized `GROUP BY key, count(*)/sum(float8)`. `columnar_agg::admit` now accepts a `groupClause` — classifying
  each output-target expr as a bare group `Var` (a `build_arrow`-supported type, incl. temporal) or a supported
  `Aggref`, and building an explicit **output layout** so PG's target order (even agg-before-key) is honored (ADR-2).
  `df_executor::run_columnar_grouped_aggs` runs DataFusion `.aggregate(group_exprs, agg_exprs)`;
  `arrow_value_to_datum` converts group keys back to PG datums (reverse of `build_arrow`), materialized in
  `es_query_cxt` so `text`-key varlena datums survive the multi-row emit (ADR-3); `exec_custom_scan` emits N rows via
  a cursor (the scalar path is the N=1 case). **VERDICT (1M rows, c-8, warm): GOAL MET** — top-level grouped result
  byte-identical to the heap, CustomScan engaged, **4.53×–9.75×** faster across int / multi-key / temporal (date) /
  agg-before-key shapes. Scope: `count(*)`/`sum(float8)`, bare-column keys, no simultaneous WHERE (declines to native).
  Honest caveat: consuming a columnar-aggregate output VALUE inside an enclosing expression hits a pre-existing M100
  limitation (the scalar path fails identically) — orthogonal to GROUP BY, tracked separately; canonical top-level
  `SELECT key, agg … GROUP BY key` works.
- **Columnar/HTAP: zone-map skip-pruning (predicate pushdown consumer) — full slice, MEASURED
  (`docs/benchmarks/columnar-zonemap-verdict.md`).** The `theodb_columnar` TAM already WROTE a per-`(chunk_group,
  column)` min/max zone-map (`compute_minmax`) but never read it — this slice (plan `columnar-zonemap-skip-pruning`)
  builds the missing consumer, closing the M99/M100/M103 "where pruning unproven" gap. The M100 planner
  `CustomScan` now **admits a `WHERE`**: `columnar_agg::extract_zone_predicate` extracts `col <op> const` clauses
  (operator resolved by **btree strategy number**, const in the column's SAME native type — ADR D5
  same-domain-or-fallback; any un-pushable qual → native plan), carried plan-time→exec in `custom_private`. A
  **DataFusion Filter** (`df_executor::build_filter_expr`) applies the predicate (the final authority — ADR D3), and
  `decode_columns` **skips** chunk groups whose min/max PROVE no row can match (`am/zonemap.rs::chunk_can_match`,
  pure + off-PG-proven; fail-safe on `has_minmax=false`). New GUC `theodb.columnar_zonemap_skip` (default on) +
  `THEODB_SCAN_PROFILE` skip-ratio metric. **VERDICT (1M clustered, 10%-selective, c-8, warm): GOAL MET.**
  Byte-identical to full decode (incl. the partial-overlap chunk group where the Filter drops non-matching
  survivors); skips **89/100 chunk groups** (decodes 11% ≤ the 25% target) for a **measured 7.29× lower latency**
  (19.3 ms vs 140.8 ms). A real measured win on the columnar/lakehouse axis (not the vector-QPS ceiling). Honest
  caveat: the skip ratio tracks selectivity × clustering — unsorted columns prune little; the 7.29× is on a
  clustered column, not unconditional (`public-copy.md`).
- **Columnar/HTAP: zone-map skip-pruning extended to TEMPORAL columns — MEASURED
  (`docs/benchmarks/columnar-zonemap-temporal-verdict.md`).** The zone-map consumer covered int/float/bool but
  `minmax_kind_of` returned `None` for temporal types, so a time-range filter (`WHERE ts BETWEEN …`) — the most
  common analytical filter on time-series — did not prune. This slice maps `timestamp`/`timestamptz` (int64 µs) to
  the proven **I8** skip path and `date` (int32 days) to **I4** — the stored bytes ARE the internal int, so
  `chunk_can_match` / `compute_minmax` / `extract_zone_predicate` / `encode_const_bits` reuse the i64/i32 path
  unchanged. `df_executor::build_arrow` builds naive-tz Arrow `Timestamp(µs)` / `Date32` arrays and
  `build_filter_expr` emits a matching Arrow-typed literal so the DataFusion Filter stays type-correct (D3).
  **VERDICT (1M clustered monotonic time-series, 10%-selective, c-8, warm): GOAL MET** for both columns —
  byte-identical, `CustomScan` engaged, skips **89/100** (timestamptz, **8.69×**) and **88/100** (date, **8.19×**)
  chunk groups. Honest caveat: same selectivity × clustering dependence — the win is on a monotonic `ts` (the natural
  time-series case). `arrow_cache` (M101 heap path) untouched — no regression.
- **Vector research (E2 FastScan): `theodb_symqg` v3 FastScan 1-bit SIMD sign kernel — full slice, MEASURED
  (`docs/benchmarks/e2-symqg-fastscan-verdict.md`).** The E2 verdict showed `theodb_symqg` slower than
  `theodb_hnsw`; the per-hop bottleneck is the 32 scalar sign-dot estimates. This slice (plan `symqg-fastscan-1bit`)
  batches them: `vec/ah.rs::build_sign_lut16` reformulates the 1-bit dot `⟨q_r,u⟩` as a LUT16-pshufb scan
  (`⌈dim/4⌉` groups of ≤4 sign-dims → 16 patterns → signed-sum LUT, int8 requant mirroring `build_lut16`), and
  `sign_estimate_block` reuses the tested `ah_score_block` (Rule 9) to score 32 neighbours in one SIMD pass + a
  cheap per-neighbour finalize reproducing `estimate_sign`. `page/symqg.rs` v3 packs neighbour sign-codes in
  block32-nibble transposed layout (`row_bytes` unchanged; `⌈degree/32⌉` blocks). `scan.rs` dispatches per index
  (**D5**): FastScan when `⌈dim/4⌉ ≤ 258` (int16-safe), else the scalar `estimate_sign` fallback — protecting
  1536-dim OpenAI embeddings from an int16-accumulator overflow. New GUC `theodb.symqg_fastscan` (default on) is
  the same-index A/B kill-switch. **VERDICT (SIFT1M, dedicated c-8, warm): gate NOT met.** The FastScan kernel is
  **correct and recall-neutral** (same-index ablation: recall identical to scalar within 0.1 pp; int8 requant
  preserves ranking), but its **measured speedup is modest — 1.07–1.22×** (grows with ef), NOT the ~2.8× a naive
  cross-box v2-vs-v3 comparison suggested (that was the steal→dedicated box change: `theodb_hnsw` itself went
  287→712 QPS from the box alone). `theodb_hnsw` remains 2.1–3.5× faster at matched recall (0.95–0.994), parity
  only at 0.999. The estimate is not the sole per-hop bottleneck (decode + heap + page-read dominate — Amdahl; the
  E1/E2 lesson repeats). **No symqg QPS-win claim is made** (`public-copy.md`, rule 5).
- **Vector research (E2): `theodb_symqg` index AM — SymphonyQG co-located quantized graph in-PG (own-code,
  clean-room).** New custom index AM `CREATE INDEX … USING theodb_symqg (col vector_l2_ops) WITH (degree_bound=N)`.
  Persists a co-located quantized graph (own-code, clean-room from arXiv:2411.12229 — the NTUITIVE-licensed
  reference C++ is **study-only, never copied**, D1): each vertex row stores its rotated vector `P·x` plus, per
  ≤`degree_bound` neighbour, a **1-bit RaBitQ sign code**, so a beam-search hop estimates all neighbours from the
  co-located codes and the popped centre's exact distance is a local read (no separate rerank). Build reuses the
  own-code HNSW for the base adjacency (`ann/symqg_spike.rs::encode_sign`), streaming encode+pack per vertex (no
  N·R materialization — the 1M OOM fix). Page layout `page/symqg.rs`: meta · rotation codebook · tids ·
  **contiguous fixed-size rows** (arithmetic `ord·row_bytes` addressing, no directory — folds the index 5.66×);
  GenericXLog crash-safety, `INSERT`→pending, `VACUUM`, MVCC-delete all validated on real SIFT1M. L2-only.
- **Vector research (E2 VERDICT): in-PG A/B `theodb_symqg` vs `theodb_hnsw` on SIFT1M
  (`docs/benchmarks/e2-symqg-inpg-verdict`).** Settles the per-hop random-page-read tax the off-PG spike could not.
  **Honest measured negative: the gate (symqg QPS ≥ 1.5× hnsw at recall@10 ≥ 0.95) is NOT met** — `theodb_hnsw` is
  **2.6–3.9× faster** at matched recall (0.95–0.994), warm. The page tax was real and dominant (v1 one-page-per-row
  layout, 7828 MB out-of-RAM → 8.5× loss) and **mitigable** (v2 contiguous packing → **1383 MB, 5.66× smaller,
  +2.3× QPS**), but a residual gap remains: the off-PG spike's 1.8–2.66× advantage did not transfer against the
  mature HNSW AM (first-cut symqg scan + per-hop 1408-byte row decode vs M35–M46-optimized hnsw). AM correctness
  (recall parity 0.857–0.9995, pending/VACUUM/MVCC) is proven; only the QPS gate is unmet. **No symqg QPS-win claim
  is made** (`public-copy.md`). Next lever (separate scope): FastScan 1-bit SIMD sign kernel (reuses
  `vec/ah.rs::ah_score_block`) + copy-free row reads.
- **Vector research (E1 core): extended multi-bit RaBitQ quantizer own-code (`vec/rabitq.rs`).** From-scratch
  reimplementation of the extended multi-bit RaBitQ algorithm (arXiv:2409.09913, Apache-2.0; the vendored tree
  was deleted in ADR-0046) — the **f32-free rerank codec**. Estimator stores per vector only the B-bit code `u`,
  the residual norm, and `W = ⟨u, o'⟩`; at search `⟨q_r, o⟩ ≈ ⟨q_r, u⟩/W` (Δ and ‖ō‖ cancel → a pure
  integer-weighted dot, **no raw vector touched**). Random orthogonal rotation via seeded Gram–Schmidt, std-only
  (no new deps, D1/D4). **VALIDATED own-code (hermetic Monte-Carlo, droplet-free, `docs/benchmarks/rabitq-estimator-validation`):**
  mean relative error 7.16% (1-bit) → **0.09% (7-bit)** with ~zero bias — a 7-bit code is accurate enough to be
  the FINAL ranking f32-free, deleting the exact Stage-2 f32-rerank bind measured in M82/v5. NOT yet wired to the
  AM scan (next: dedicated code page + f32-free `scan_ivf_aq_split` Stage-2 + SIFT1M A/B). Research blueprint:
  `knowledge-base/discoveries/blueprints/vec-f32free-rerank-blueprint.md`.
- **Vector research (E1 wiring): IVF-AQ v8 index `WITH (separate_storage=1, refine=2, rabitq_bits=N)` — f32-FREE
  residual-RaBitQ Stage-2 rerank (L2-only).** Wires the validated `vec/rabitq.rs` codec into the
  `theodb_ivfflat` AM. Build encodes the per-list residual `x − centroid[ci]` into a dedicated RaBitQ code page
  (`[i8×dim][nr][w]` = dim+8 B/vec), on pages distinct from both the AH codes and the f32 vectors (v5 storage
  separation preserved). Scan `scan_ivf_aq_split_rabitq`: Stage-1 AH prune over codes-only pages (identical to
  v5/v6), Stage-2 reranks the `rerank_pool` survivors via `estimate_l2_sq` on the RaBitQ codes — **removing the
  exact-f32 random-read bind measured in M82/v5** (zero raw vector touched at rerank). Pending (post-build INSERT)
  rows stay f32-exact. New reloption `rabitq_bits` (default 7, range 1–8); `refine=2` selects RaBitQ. 6-file AM
  surgery (`options.rs`, `page/ivf.rs`, `build.rs`, `scan.rs`, meta v8).
- **Vector research (E1 VERDICT): f32-free RaBitQ rerank MEASURED on SIFT1M in-PG (`docs/benchmarks/e1-rabitq-inpg-verdict`).**
  Same-data v5 (f32 rerank) vs v8 (7-bit RaBitQ rerank) A/B, 1M vectors, official GT, real `theodb_ivfflat` scan.
  **Recall parity** (v8 within ~1.5 pp of v5 across the full sweep, e.g. 0.979 vs 0.9925 @ of=16/probes=64);
  **index 3.28× smaller** (161 MB vs 528 MB — the f32-free rerank drops the raw-vector refine region); **cold /
  out-of-RAM latency 2.5–2.8× lower** at recall parity (75 ms vs 189 ms @ of=16/probes=64 with the OS cache
  dropped per query) — the E1 gate (≥2× QPS at recall parity) is MET in the out-of-RAM regime. **Warm/in-RAM is
  parity** (QPS 0.86–1.08×, buffers/query 1.01–1.02×): Stage-2 refinement is not the in-RAM bottleneck (M85
  holds). The win is memory + billion-scale (the North-Star-credited axis, ADR-0035); it is NOT a warm
  vector-QPS-superiority claim over ScaNN/AlloyDB (that ceiling stands — M73/M82/ADR-0036).
- **Vector research (E2 productization): `theodb_symqg` in-PG AM plan** (`knowledge-base/plans/symqg-inpg-am-plan.md`, plan-confidence SHIPPABLE_WITH_CAVEATS 70). Plans the in-PG SymphonyQG quantized-graph index AM (persisted co-located graph + build reusing HnswIndex/encode_sign + page-reading beam scan + WAL/VACUUM/reloptions), with the acceptance gate = in-PG A/B ≥1.5× vs theodb_hnsw at matched recall on SIFT1M (settles the per-hop page-tax the off-PG spike could not). Clean-room from the paper (D5).
- **Vector research (E2 discovery): SymphonyQG clean-room blueprint** (`knowledge-base/discoveries/blueprints/symphonyqg-graph-quant-blueprint.md`).
  Maps the SymphonyQG design (arXiv:2411.12229, SIGMOD'25) from the paper + a STUDY-ONLY clone of the
  NTUITIVE-non-commercial reference (D1: never copied/transcribed — clean-room from the paper only, like the RaBitQ
  own-code per ADR-0046). Design: per-vertex row co-locates the R neighbors' 1-bit RaBitQ codes (FastScan block-32)
  + factors + IDs; beam search FastScan-estimates all neighbors per hop with NO separate rerank — the lever that
  attacks the Stage-1/traversal bottleneck E1 measured. Honest gap: standalone-C++ 3.5–17× vs HNSWlib does NOT
  transfer to a warm-QPS win over ScaNN (paradigm ceiling stands, M73/M82); realistic prize = beat our OWN
  HNSW/IVF-AQ in-PG; index GROWS (replicated codes). Gate: hermetic own-code spike must beat our best engine ≥1.5×
  @ recall 0.95 off-PG BEFORE any in-PG AM build (anti-sunk-cost).
- **Vector research (E2 spike): SymphonyQG mechanism MEASURED — recall parity + 12–26× fewer exact distances, but
  wall-clock gated on a FastScan 1-bit kernel** (`ann/symqg_spike.rs`, `docs/benchmarks/e2-symqg-spike.md`).
  Clean-room own-code (D1): HNSW base graph + per-parent co-located RaBitQ codes (`encode(x_i − x_parent)`, reusing
  E1's `estimate_l2_sq` with `c`=parent) + faithful Algorithm-1 beam search (estimate-keyed beam, separate exact-NN;
  a first-cut termination bug that mixed estimate/exact scales was found+fixed). Measured on SIFT via an in-PG
  `symqg_spike_bench` entrypoint: **7-bit symqg recall == exact recall at every beam, at 12–26× fewer EXACT distance
  computations** (mechanism GREEN); but **wall-clock 0.45–0.82× (slower)** because a SCALAR estimate costs ≈ one L2,
  so the all-neighbors estimates outweigh the pruned exacts. The ≥1.5× gate is now localized to ONE unbuilt
  component: a batched FastScan 1-bit RaBitQ kernel (~8–16× cheaper than exact) — RaBitQ-Library (Apache-2.0) as
  permissive reference. Spike-first gate did its job: de-risked the mechanism, priced the remaining work honestly.
- **Vector research (E2 gate MET): 1-bit SIGN codec — recall parity + ~2.2× faster at SIFT1M, off-PG (scalar).**
  The multi-bit estimator lost because its dot ≈ one L2; the SymphonyQG **1-bit sign** makes the neighbor dot
  `Σ ±q_r[d]` multiply-free (~2-3× cheaper/elem). Our multi-bit codec is degenerate at bits=1, so a dedicated sign
  codec was added. **Measured on the FULL SIFT1M (correct GT, real recall@10):** symqg reaches recall parity
  (0.998) and is **1.8–2.66× faster** than exact-distance traversal on the same HNSW graph at recall 0.95–0.99,
  15–27× fewer exact distances — SCALAR (FastScan SIMD kernel is an ADDITIONAL multiplier). The off-PG gate
  (≥1.5×) is MET. Caveat: off-PG (pure in-RAM search; no heap/WAL/MVCC) — the next gate is the in-PG AM (per-hop
  random page read). `docs/benchmarks/e2-symqg-spike.md`.

- **Vector research (E2 impl T1.1): `theodb_symqg` co-located page layout** (`am/page/symqg.rs`). Persisted per-vertex row `[nbr_ids][1-bit sign bytes][nr/w factors]` (degree padded to 32; sentinel-skipped slots) + `SymqgMeta` + directory; rows are chunked so a high-dim×degree row spans pages (EC-2). Pure codec (SymqgMeta encode/decode, pack_row/decode_row, sign-bit pack) proven 6/6 standalone (round-trip, bad-magic reject, padding, row-spans-pages, truncated→typed-Err EC-7); crate compiles clean. Foundation for `ambuild_symqg`/`scan_symqg_structured`.

- **Vector research (E2 impl T2.1+T3.1): `theodb_symqg` build + scan WORKING in-PG** (`am/build.rs` ambuild_symqg, `am/scan.rs` scan_symqg_structured, `am/mod.rs` handler+opclass). `CREATE INDEX … USING theodb_symqg` persists the co-located graph (HNSW base adjacency + 1-bit sign codes + rotated vector P·x per row); `SELECT … ORDER BY e <-> q LIMIT k` beam-searches reading one row/hop, reusing the off-PG-validated `estimate_sign` + the rotation trick (exact dist=‖q_r‖² and q_r=rot_q−rot in one O(D) subtraction, no per-hop rotate). L2-only (fail-fast), EC-1 build cancellation, EC-3 query-dim guard, sqrt-L2 scale (E1 lesson). **Measured: recall@10=1.0000 vs exact brute-force** (2000×16d, 20 queries) — correctness proven end-to-end. Next: T4.1 reloptions/VACUUM/crash + T5.1 SIFT1M A/B vs theodb_hnsw.

- **Vector research (E2 impl T4.1): `theodb_symqg` production hardening — reloption + VACUUM + INSERT/pending + crash-safety.** `WITH (degree_bound=R)` reloption (`am/options.rs`, R multiple of 32, HNSW m=R/2); VACUUM is a safe no-op on the co-located graph (scan pending-fold + MVCC re-check drop dead TIDs, same as IVF v4-v8); INSERT→pending fixed (`am/page/mod.rs` `main_index_pages` symqg branch — the E1-class pending-region-base gap); crash-safety inherited from GenericXLog (`extend_page_with_item`). **Validated on DISTINCT random data (a degenerate-test-data false-pass was caught + fixed): recall@10=0.97 vs exact (ef=100, 3000×16d), INSERT dup found at dist 0 (top-2), DELETE+VACUUM leaks 0 rows.** T2.1+T3.1+T4.1 GREEN end-to-end. Next: T5.1 SIFT1M A/B vs theodb_hnsw.

## [0.101.0] - 2026-07-17

### Added
- **M113 — SQL/PGQ-subset surface (native graph pillar Phase 6):** `theodb.pgq_match(edge_rel, source_ids,
  pattern, default_max)` — a DuckPGQ-style UDF-minimal bounded-path `MATCH` that parses the SQL/PGQ quantifier
  (`*min..max`, `*N`, `*`, bare edge) and dispatches to the M108/M109 traversal (min-hop shell via subtracting
  the `<min` reachable set). Composes with `<=>` (vector) and `ai.rerank` in one SQL statement (the
  composability gate, proven). Honest scope: the ergonomic SUBSET GraphRAG needs (bounded reachability), NOT
  full SQL/PGQ conformance (path variables / ELEMENT_ID / pattern-WHERE) — a real grammar-level parser hook is
  the deferrable part the milestone scopes out. No new crate. 359 pg_tests GREEN (+3, 0 regression). (M113)

## [0.100.0] - 2026-07-17

### Added
- **M111/M112 — GraphRAG retrieval flow (vector-entry→traversal→rerank) + Personalized PageRank (native graph
  pillar Phases 4–5):** `theodb.graph_rag_search` (cosine-entry over `graph_nodes.embedding` → `graph_expand`
  → edge-weight rank), `theodb.graph_embed_nodes` (reuse `ai.embed`), and `theodb.graph_ppr` (Personalized
  PageRank power-iteration over the CSR, HippoRAG ranking; hermetic oracle: symmetric + monotone-decaying from
  seeds). Mechanisms BUILT + proven. **HONEST MEASURED VERDICT on real HotpotQA distractor** (HuggingFace
  `hotpotqa/hotpot_qa`, `text-embedding-3-small`, `docs/benchmarks/m111-m112-graphrag-retrieval`): pure vector
  wins in EVERY configuration — graph-only heuristic 0.32, hybrid 0.72, **LLM(gpt-4o-mini)-extraction + PPR
  0.53, hybrid 0.83** all < **pure vector 0.85–0.87** (recall@4). Even the full HippoRAG recipe does not beat a
  strong modern dense embedder on HotpotQA (HippoRAG's gains were vs weaker 2024 retrievers; HippoRAG-2 warns
  graph-RAG can drop below standard RAG on factual tasks). Anti-sunk-cost (D3): the pillar's real value is its
  fast engine + extraction surface (M108 16×, M109 5–8×, M110 theo-rag→3-SQL-calls), NOT a retrieval-quality
  win over vectors. 356 pg_tests GREEN (+8, 0 regression). (M111, M112)

## [0.99.0] - 2026-07-16

### Added
- **M110 — in-DB graph extraction surface (native graph pillar Phase 3):** `ai.extract_entities` /
  `ai.extract_graph` (heuristic-default: capitalized-run entities + windowed co-occurrence edges — a byte-identical
  Rust port of theo-rag's `graph-extractor.ts`; `use_llm` opt-in reuses `chat::chat` with a GraphRAG delimited
  prompt, parser-tested, fail-soft to heuristic) + idempotent `theodb.graph_upsert` into CSR-shaped
  `theodb.graph_nodes`/`graph_edges` (`ON CONFLICT … mention_count/weight +=`, mirrors theo-rag `graph-store.ts`).
  **Gate = cross-language parity (100% coverage, golden from the real theo-rag extractor) + E2E set-hash**
  (extract→`graph_build`→`graph_expand`) → downstream recall non-regressed BY CONSTRUCTION (blueprint ADR-2).
  Deep research (GraphRAG arXiv:2404.16130, HippoRAG 2405.14831, KGGen/MINE 2502.09956) established the extrinsic
  gate over entity-F1 and the honest heuristic-vs-LLM delta. **MEASURED (`docs/benchmarks/m110-extraction`):**
  extraction 1537 chunks/sec, parity 100%. Payoff: theo-rag's graph strategy sheds `extraction/` + `graph-store/`
  + the recursive CTE for 3 SQL calls. Security: parameterized-data-only, REVOKE-from-PUBLIC, newline-collapse
  prompt-injection guard (ADR-3). No new crate (Rule 9 — port + reuse). Review (council-security + council-ai-in-db)
  fixed 2 MEDIUM (edge `description` COALESCE-upgrade on re-ingest; corrected the parity-coverage claim to
  ASCII/English scope) + filed 2 pre-existing platform gaps (#117 SSRF `llm_endpoint`, #118 tenant-blind
  `graph_build`). 348 pg_tests GREEN (+11, 0 regression). (M110)

## [0.98.0] - 2026-07-16

### Added
- **M109 — vectorized Multi-Source BFS operator (native graph pillar Phase 2):** `theodb.graph_expand_multi` /
  `graph_expand_multi_card` advance up to 64 independent BFS lanes per CSR sweep via per-vertex `u64`
  source-masks (frontier-driven, bit `l` = lane `l`; auto-vectorized bitwise-OR — the source-parallel
  mechanism, NOT `vec/ah.rs`'s candidate-parallel `pshufb`). Each lane's reachable set is proven byte-identical
  (per-lane set-hash oracle) to single-source `expand`. **MEASURED traversal-only (`docs/benchmarks/m109-msbfs`,
  confound-free, mean±std over 3 runs):** batched MS-BFS beats N sequential single-source BFS **~1.7× @N=1 →
  ~5–8× @N≥16** (pure_speedup), and the win is **robust across topologies** (~10× on a uniform-random graph at
  N=64, refuting hub-gaming); oracle PASS at every N=1..512 — the growth-with-N is Then et al.'s (VLDB'14)
  edge-sharing mechanism. Also `graph_expand_card` (single-source reach-count). Deep research
  (Then VLDB'14, DuckPGQ CIDR'23/VLDB'23, HippoRAG, GAP) corrected the ROADMAP "reuse ah.rs kernels" misframing
  and caught a row-materialization benchmark confound that had masked the win. 337 pg_tests GREEN (+7, 0
  regression). (M109)

## [0.97.0] - 2026-07-16

### Added
- **M108 — persisted-CSR graph structure (native graph pillar Phase 1):** `theodb.graph_build/expand/refold` persist the graph CSR once as a WAL-safe `bytea` (`theodb.graph_csr`) + a per-backend deserialized-CSR cache (M101 pattern, `built_at`-keyed invalidation) — so per-query traversal is load+traverse, NOT the per-query rebuild that capped M107. **MEASURED (`docs/benchmarks/m108-persisted-csr`, release):** warm `graph_expand` 16.5ms vs recursive-CTE 263ms = **16×** (cold 10×), build paid once (274ms), correctness-oracle PASS (reached=27752). Rule 9: reuses PostgreSQL's native bytea durability (no hand-rolled index-AM WAL). Review (council-index-storage) fixed 2 HIGH (OID-reuse stale row → `sql_drop` event trigger; clock_timestamp cache-invalidation regression proven) + 1 MEDIUM (set-hash oracle). 330 pg_tests GREEN (+5, 0 regression). (M108)
- Roadmap amended: added the native graph pillar follow-on milestones **M108–M113** (persisted-CSR index-AM → vectorized MS-BFS operator → `theodb.graph_expand`/`ai.extract_graph` surface → vector-on-nodes flow → PPR/community (gated on measured need) → SQL/PGQ surface (optional)), each with its own measurement gate per ADR-0048 (`/roadmap-feature` ×6)

## [0.96.0] - 2026-07-16

### Added
- **M107 Phase 0 — native graph engine D3 gate = GO:** SOTA blueprint (DuckPGQ/Kùzu/GRFusion/SQL-PGQ/GraphRAG) + a reproducible own-code spike proving native **CSR adjacency + frontier BFS** beats the theo-rag recursive-CTE baseline by **262–732× on traversal** (8–108× end-to-end), correctness-oracle PASS on all 8 trials (`docs/benchmarks/m107-graph-spike.{md,json}`). Honest caveat measured: on-the-fly CSR build dominates at 1M → Phase 1 persists the CSR (ADR-0048). Architecture decided: native traversal operators over the existing columnar+vector substrate — NOT recursive-CTE, NOT Apache AGE (Cypher-on-joins, same per-hop tax), NOT a bundled graph engine (Rule 9). Phase 0 = gate only; the engine phases are follow-on milestones authorized by the GO. (M107)
- Roadmap amended: added M107 native graph pillar Phase 0 — SOTA blueprint + measurement-first spike gate (CSR + vectorized MS-BFS + SQL/PGQ fused with the columnar+vector+AI engine) (`/roadmap-feature native-graph-engine`)

## [0.95.0] - 2026-07-16

### Added
- **M106 — weighted RRF na busca híbrida (`vector_weight`/`text_weight`):** a fusão `ai.hybrid_search(jsonb)` agora honra pesos por perna — `score = vector_weight/(k+rank_vec) + text_weight/(k+rank_fts)` (default 1.0/1.0 = RRF pura, byte-idêntico ao anterior; peso finito ≥ 0, `0` desliga a perna, negativo → erro tipado 22023). Move a chave `weight` documentada-mas-não-entregue (audit gap 06) para shipped. Injeção segura: pesos validados e formatados como literais numéricos. **MEDIDO** (`docs/benchmarks/m106-weighted-rrf.md`): com o mesmo corpus, `vector_weight=3` sobe o doc da perna vetorial ao topo e `text_weight=3` FLIPA para o doc da perna FTS. Provado: 3 pg_tests Rust + 5 testes do twin offline + 2 de integração SQL. 324 pg_tests GREEN (+3), 0 regressão. (M106)

## [0.94.0] - 2026-07-16

### Added
- **M105 — docs/features reconciliadas com a superfície entregue:** os 12 specs em `docs/features/` foram alinhados ao código real — `theodb_ml.embedding`→`theodb.embed`; opclasses próprias (`theodb_hnsw_l2_ops`/`theodb_ivfflat_l2_ops`) nos exemplos de AM próprio; `CREATE EXTENSION theodb_ml`→schema+registry; o `ai.rank` fantasma de 4-arg→`ai.rerank(query, documents[], model, top_n)` real (idx 0-based, off-by-one do RAG corrigido); chaves JSON não-implementadas do hybrid + `g_to_tsquery` removidas dos exemplos runnable. Superfícies aspiracionais (04 `USING ivf`, 05 `USING scann` — ScaNN-QPS measured-negative ADR-0035/0036, 08 Proxy Model, 12 `theodb_ai_nl.*`) movidas para seções **🎯 API-alvo / roadmap (não-shipped)**. GATE: varredura determinística confirma que todo bloco SQL runnable da seção shipped referencia só símbolos reais (12/12). Docs-only, zero mudança de código. (M105)
- Roadmap amended: added M105 docs/features reality reconciliation + M106 API-consistency hygiene (`/roadmap-feature docs-features-reality-reconciliation`)

## [0.93.0] - 2026-07-16

### Added
- **M104 review fixes (H1 + MEDIUM):** the five M104 bounded-memory/resilience knobs (`theodb.vacuum_fold_max_mb`, `arrow_cache_max_entries`, `vectorizer_dead_letter_max`, `http_breaker_open_ms`, `ai_max_batch`) are now REGISTERED via `GucRegistry` (were read via `current_setting` only, so `SET` was silently ignored — the review's H1: an advertised 'configurable bound' that wasn't operable). And the `_vectorizer_*` internal functions (claim/mark/process/reap + the new dead-letter purge) are now `REVOKE ALL ... FROM PUBLIC` (dynamic `::regprocedure` block) — matching the codebase's per-function least-privilege convention. Proven: `m104_dead_letter_max_guc_is_registered_and_settable`, `m104_vectorizer_internals_revoked_from_public`. 321 pg_tests GREEN (+2). (M104)
- **M104 Phase H — Boundaries + Scaling trade-off documentation:** split the 1986-LoC `am/page.rs` god-module into `am/page/{mod,ivf}.rs` (generic page/buffer/WAL primitives vs the IVF/AQ on-disk format cluster; facade re-export keeps all 47 call-sites unchanged; 319/319 pg_tests GREEN) — closes the audit's tangled-namespace Boundaries finding physically. ADR-0047 records the three residual Scaling items as **deliberate bounded designs with migration paths** (in-VACUUM fold guard + REINDEX; external-memory HNSW fold is research-scope/YAGNI-deferred; HTTP conn-pool mitigated by `ai_max_batch` batching; v4 default retained for on-disk-format stability with WARN). (M104)
- **M104 Phase G — vectorizer producer backpressure via coalescing (MEDIUM):** the enqueue trigger now COALESCES — a partial `UNIQUE (vectorizer_id, source_pk) WHERE state='pending'` index plus `ON CONFLICT DO NOTHING` means repeated writes to the SAME source row produce at most ONE pending job. A hot row / bulk backfill can no longer flood the single worker past the DISTINCT changed-row set: pending queue depth is bounded by distinct pending work, not by write volume (the audit's producer-faster-than-consumer data-flow gap). Proven: `m104_enqueue_coalesces_repeated_writes_to_one_pending` (4 writes → 1 pending; distinct rows stay independent). 319 pg_tests GREEN (+1). (M104)
- **M104 — VACUUM fold memory bound + legacy deprecation markers + columnar-read boundary:** the O(N)-in-RAM VACUUM compaction fold (blob/v3/HNSW paths; the modern IVF v4–v7 already no-op) now SKIPS with a WARN when the index exceeds `theodb.vacuum_fold_max_mb` (default 1024) — a possible OOM becomes a documented safe deferral (correctness preserved via the scan pending-fold + MVCC re-check; REINDEX compacts; the fully-bounded streaming fold is M55). Added `DEPRECATED (M104)` markers to the M26 blob legacy path (`page::read_blob`, the `vacuum_rebuild` blob branch). Documented `columnar::decode_columns` as the intentional columnar-read API boundary (not an internal leak). 318 pg_tests GREEN. (M104)
- **M104 Phase E / deletion hygiene — vectorizer dead-letter bound + v4 legacy WARN:** the vectorizer worker now purges the on-disk `failed` dead-letter beyond a retained cap (`theodb.vectorizer_dead_letter_max`, default 1000; `_vectorizer_purge_dead_letters` wired next to the reaper) — a poison row / mis-set endpoint no longer accumulates tombstones forever (data-flow MEDIUM). The legacy v4 (interleaved, OOM-prone) IVF-AQ build path now emits a WARN pointing at `WITH (separate_storage=1)` (the bounded-memory streaming layout) — the audit's inverted-default finding, without a risky on-disk-format default flip. 318 pg_tests GREEN (+1). (M104)
- **M104 Phase B2 / Q3 — bounded caches & batches:** the M101 per-backend Arrow cache is now entry-bounded (`theodb.arrow_cache_max_entries`, default 16 — evicts before inserting a new table, closing the unbounded-cache finding). The batched AI (`ai.generate_batch` / `ai.if_batch` / `embed_batch` via `run_batch_chat`) chunks prompts into `theodb.ai_max_batch`-sized groups (default 256) — a huge array becomes several bounded round-trips instead of ONE giant request/response. 317 pg_tests GREEN, 0 regression. (M104)
- **M104 Phase B1 — streaming columnar scan (HIGH):** the seq-scan no longer full-materializes the whole visible table before the first row — `columnar_scan_begin` resolves the visible-stripe SET once (MVCC-fixed under the scan snapshot) and `getnextslot` decodes ONE stripe at a time (draining the same-xact pending rows as the final batch). Peak scan memory is **O(one stripe ≈ maintenance_work_mem)**, not O(the whole table) — the Arrow RecordBatch / DuckDB row-group-at-a-time streaming pattern. Row order is byte-identical to the old eager path (`m104_streaming_scan_matches_full_result` + the M99 roundtrip suite). The orphaned `materialize_rows` was removed. 317 pg_tests GREEN (+1). (M104)
- **M104 Phase D — boundary & deletion hygiene:** deleted the inert `theodb_rs/src/rabitq/vendor/` tree (5.6k LoC, never compiled — no `mod rabitq`, no refs, not in Cargo.toml; git preserves it) — the audit's HIGH zombie (ADR-0046). Relocated `AqQuantizer` `am/aq.rs` → `vec/aq.rs` (it is pure domain, no `am` deps) — fixes the `vec/ah.rs → am::aq` layering inversion (SIMD/domain layer no longer imports the storage AM). 316 pg_tests GREEN, 0 regression. (M104)
- **M104 Phase F — North-Star governance reconciliation:** ADR-0033 (repositioning to "recall parity + memory + AI-native/HTAP/open") signed → **ACCEPTED** (owner-authorized via the M104 goal); a supersede note added to the LOCKED ADR-0002 pointing at the measured verdicts ADR-0035/0036 (the vector-QPS-superiority axis is measured-invalidated). Closes the audit's sole `rationale_valid=0` trade-off (ADR-0045). (M104)
- **M104 Phase C — AI HTTP circuit breaker (per-backend, HIGH):** `http.rs` gains a `thread_local` closed/open/half-open circuit breaker (Nygard / MS / resilience4j) keyed by endpoint — after K=5 consecutive failures the breaker OPENS and further calls fail FAST (SQLSTATE 38000, no TCP attempt) for `theodb.http_breaker_open_ms` (default 30s), then one half-open probe decides re-close. A per-row `ai.*` surface over a dead endpoint now costs ~K probes instead of N × retries × timeout. The SSRF/redirect=0/api-key-in-header/38000 posture is unchanged. 2 pg_tests GREEN (opens+fails-fast <100ms; success closes). 316 pg_tests total (+2). Cross-backend (shared-shm) coordination is a documented non-goal until measured. (M104)
- **M104 Phase A — bounded columnar write memory (#99 CRITICAL closed):** the columnar TAM now flushes a stripe INCREMENTALLY once pending bytes exceed `maintenance_work_mem` (the DuckDB row-group / ClickHouse one-part-per-INSERT pattern, reusing the existing atomic `flush_pending`), so a big `INSERT...SELECT` holds **O(maintenance_work_mem)** — not O(rows-in-xact) — in RAM. **MEASURED (`docs/benchmarks/m104-write-envelope.{md,json}`):** 64× more rows → 46× more stripes (linear) while the peak pending set stays ~constant (~2–3 MB ≈ mwm). Snapshot-safe (H1: self-referential INSERT honors its snapshot) + crash-safe (H3: `crash_columnar_incremental.sh` — aborted multi-stripe INSERT → 0 rows, committed → survives crash+WAL-replay byte-identical; no #46/#47 regression). 314 pg_tests GREEN (+2). (M104)
- Roadmap amended: added M104 system-design hardening — fechar as findings da auditoria `/loop-system-design` (health 4.2 → ≥4.9/5) (`/roadmap-feature system-design-hardening-49`)

## [0.92.0] - 2026-07-16

### Added
- Durability crash-recovery proofs for the AM (closes the ADR-0014 "Prova pendente"): `theodb_rs/isolation/crash_fold.sh` induces **3 real backend crashes (SIGABRT)** across all VACUUM-fold phases (before-pivot / post-pivot / mid-reclaim) + WAL replay and asserts the #47 guarantee — crash before the meta-pivot ⇒ old generation correct; crash after ⇒ fail-loud REINDEX; **never a silently-wrong result**. `theodb_rs/isolation/crash_unlogged.sh` proves the #46 fix via standby promotion (a RED/GREEN toggle shows `wal_log_init_fork` is load-bearing: without it the promoted UNLOGGED index is broken; with it, INSERT + scan work). Wired as `make -C theodb_rs/isolation check-crash`. Issues #46/#47 verified & closed.

### Deprecated

### Removed

### Fixed

### Security

## [0.91.0] - 2026-07-16

### Added
- **M103 — vector + columnar in one substrate (Lance-inspired co-residence):** the IVF vector index (`part_id` + raw `vec` bytea) is stored AS columns co-resident with the scalar `label` + the analytical columns in a `theodb_columnar` table, so a scalar-prefiltered vector top-k + an analytical aggregation compose in ONE column-pruned scan. New `theodb.vindex_assign` (IVF partition per row, materialized as a column), `theodb.vindex_knn_columnar` (filtered top-k reading ONLY the 4 index columns), `theodb.vindex_decode_bytes`, `theodb.f32vec_to_bytea`. **GATE (recall correctness):** the co-resident filtered top-k is BYTE-IDENTICAL to the exact filtered brute-force (shared `am/scan.rs::Scored` tie-break + `vec::l2_dist_from_bytes` kernel) — proven by `m103_full_probe_byte_identical_to_exact_filtered` (312 pg_tests GREEN, +5). **MEASURED (`docs/benchmarks/m103-vector-columnar.{md,json}`):** column pruning quantified by an isolated decode control — decoding only the 4 index columns (49.57 ms ± 0.29) vs ALL columns (219.81 ms ± 1.78) on the wide index = **77.4 % of decode time saved**; the end-to-end knn latency is invariant to analytical width (ratio 1.009); composed filter-knn + aggregation in one plan (225.41 ms ± 1.02). ADR-0044. Sign-off: council-vector-ann + council-index-storage + council-benchmark all READY_TO_MERGE. **Honest ceiling:** a cost/scale/composability win — recall EQUAL by construction (not a claim), **NO QPS-vs-ScaNN claim** (the M73/M74 paradigm ceiling is untouched by co-residence); the out-of-RAM value is a projection, not measured. Follow-up #108. (M103)

## [0.90.0] - 2026-07-16

### Added
- **M102 — AI predicates as SET-oriented, planner-optimizable operators (`AI.IF` pushable):** `ai.if_batch(condition, vals[])` answers N rows in ONE inference round-trip (a yes/no-shaped batched call — same boolean framing as per-row `ai.if`) instead of one HTTP call per row, and `ai.if_costly(condition, val)` is declared with a high `COST` so Postgres's `order_qual_clauses` evaluates cheap relational filters FIRST — LOTUS's dependency-safe filter push-down, delegated to the planner (Rule 9). New `ai.call_count()` / `ai.call_reset()` expose the inference round-trip count as the wiring-triad runtime metric. A hermetic `theodb.llm_test_model = 'parity'` proves the batched operator equals the per-row `ai.if` WITHOUT a live LLM (ADR D3). **MEASURED on droplet (pg17):** batched **1 round-trip vs per-row 1000** for N=1000; push-down `WHERE id<=100 AND ai.if_costly(...)` evaluates the AI on **100 survivors, not 1000**; real OpenAI `gpt-4o-mini` (K=16, 3 runs) **≈12× lower latency** batched vs per-row (`docs/benchmarks/m102-ai-operators.{md,json}`). 307 pg_tests GREEN (+4), zero regression. Sign-off: council-ai-in-db + council-security both READY_TO_MERGE (2 HIGH from council-ai-in-db — boolean shaping + ADR honesty — fixed and re-verified). ADR-0043 revisits ADR-0007 (batched inference). Honest ceiling: a composability / round-trip win with statistical accuracy, **orthogonal to vector recall** — never framed as "faster at vectors". Follow-up #106. (M102)

## [0.89.0] - 2026-07-16

### Added
- **M101 /review sign-off (council-index-storage READY_TO_MERGE + council-benchmark corrections applied):** council-index-storage signed off the MVCC correctness with zero blockers — the design is sound and proven: the cache is a derived copy, the invalidation `generation` is read via MVCC (a read-only SPI runs under the reader's ActiveSnapshot, so the generation read and the rebuild seqscan are CO-SNAPSHOT), so `built_generation == current_generation` is a correct "the committed set I see is the set the cache captured" test — RR-safe, with no per-row xmin/xmax (the M99 D2 trap avoided). Added the `// MVCC-LOAD-BEARING` invariant comment (the correctness is fragile to a mutating-SPI refactor). Applied council-benchmark's traceability corrections: the authoritative cache-vs-native-heap equivalence is the `m101_cache_agg_matches_heap` pg_test (floats within 1e-6, not "byte-for-byte"), the isolation spec paths are corrected, and the scorecard states OLTP-p95-under-load is NOT measured (structural argument only). Filed follow-up issue #104 (read-your-own-write permutation, count(*)-only admission test, OLTP-p95 load benchmark). (M101)
- **M101 Phase D (MVCC isolation permutations + HTAP benchmark — completes the M101 DoD):** (1) **Two `pg_isolation_regress` permutations** (`theodb_rs/isolation/arrow_cache_{invalidation,rr_snapshot}.spec`) prove the heap-authoritative cache respects snapshot isolation — **MEASURED, both GREEN on the droplet:** (a) a committed write by another session invalidates a reader's cache, so the reader's next read rebuilds and sees the new row (cross-backend invalidation via the shared generation); (b) a REPEATABLE READ reader holds its snapshot across a concurrent committed write (its cache-generation read under the RR snapshot is unchanged → the cache is reused → still sees the old set), and a fresh transaction after commit sees the new row (rebuild). This is the "MVCC-correct cache" gate (ROADMAP M101 DoD #3). Added `theodb_cache_refresh` (build a backend's cache without bumping the generation — the per-backend cache is not shared). (2) **HTAP benchmark** (`docs/benchmarks/m101-arrow-cache.{md,json}`): 2,000,000-row heap table, `count(*), sum(measure)`, 5 runs — **the vectorized Arrow-cache aggregate (52.4 ms) is 2.48× faster than the native heap aggregate (130.0 ms)** (no heap seqscan on a cache hit), EXPLAIN-confirmed as the CustomScan. Honest ceiling: a write costs a rebuild; the manual `columnarize` pragma is NOT AlloyDB's auto-maintained engine; OLTP-p95 non-interference is structural (read-only, no extra heap lock) with a load-measured p95 as an honest follow-up. (M101)
- **M101 Phase C (planner `CustomScan` for a heap table with a usable Arrow cache — the HTAP headline):** extended the M100 `create_upper_paths_hook` admission to a second mode: a simple `count(*)` / `sum(float8)` aggregate over a HEAP base table is now admitted when this backend holds an Arrow cache covering the summed columns (a cheap thread-local `has_cached_columns` check — no SPI in the planner hook), and at exec runs the aggregate over the cache (`run_cache_aggs` → `get_or_build`, which rebuilds snapshot-correctly if a write invalidated it). The columnar-table branch (M100) is unchanged; `custom_private` carries a mode flag. **MEASURED on droplet (pg17): a `count(*)`/`sum(measure)` over a 20000-row HEAP table with a cache is planned as a `Custom Scan` (EXPLAIN), result-identical to the native heap aggregate, and stays correct after a write (the cache rebuilds at exec — 20001)** (`m101_heap_cache_customscan_matches_heap` pg_test; full suite 303 GREEN, zero regression). This delivers the HTAP acceleration in a single plan. The cross-xact MVCC permutations + the OLAP-accelerated/OLTP-non-degraded HTAP benchmark are Phase D. (M101)
- **M101 Phase B (invalidate-on-write + snapshot-correct rebuild — the MVCC substrate):** a shared `columnar.cache_state (relid, generation, cols)` catalog + an AFTER INSERT/UPDATE/DELETE/TRUNCATE statement trigger (`columnar._invalidate()`, installed by `theodb_columnarize`) that bumps the generation on any write, within the writing xact. A read reuses its per-backend cache ONLY when its built generation matches the current generation; otherwise it REBUILDS under the reader's own snapshot — which makes the cache snapshot-correct by construction (it materializes exactly what the reader's snapshot sees), never carrying per-row xmin/xmax visibility (the M99 D2 "don't re-implement MVCC" trap). **MEASURED on droplet (pg17): after the cache is built (10000 rows), an INSERT bumps the generation via the trigger, and the next cache read rebuilds and returns 10001 — the cache never returns a stale answer** (`m101_write_invalidates_cache` pg_test; full suite 302 GREEN, zero regression). The full cross-xact snapshot-correctness proof is the Phase D `pg_isolation_regress` permutations. (M101)
- **M101 Phase A (heap-authoritative Arrow columnar cache — the HTAP substrate de-risked):** new `am/arrow_cache.rs` + a `theodb_columnarize(table, cols)` pragma build an in-memory Arrow `RecordBatch` from a HEAP table's projected columns (via SPI over the heap's committed rows — the heap stays the source of truth) that the M100 DataFusion executor aggregates. Split `df_executor::run_aggs_on_batch` (the batch→DataFusion-aggregate half, shared by the M100 columnar path and the cache) out of `run_columnar_aggs`. **MEASURED on droplet (pg17): a `count(*)` + `sum(measure)` over the Arrow cache of a 50000-row heap table is result-identical to the same aggregate over the heap** (`m101_cache_agg_matches_heap` pg_test; full suite 301 GREEN, zero regression). This de-risks the heap→Arrow build + aggregate before the MVCC machinery. Follow-up phases: invalidate-on-write trigger + snapshot-compatibility gate (B), planner `CustomScan` admitting a heap-with-valid-cache (C), the pg_isolation MVCC permutations + HTAP benchmark (D). Own-code glue (Rule 9). (M101)

## [0.88.0] - 2026-07-16

### Added
- **M100 /review sign-off (council-rust-pgrx + council-benchmark = READY_TO_MERGE):** both councils reviewed the final planner-hook + CustomScan + async-seam implementation and the benchmark honesty, signing off with zero blockers. Applied their corrections: the aggregate admission guard now rejects `aggsplit != AGGSPLIT_SIMPLE` (a partial/parallel-split Aggref carries the transtype, not the final int8/float8 — a type-safety hole → fail-safe to the native plan; council-rust-pgrx HIGH); and 3 benchmark-doc honesty qualifiers (the 9.89× reflects a 5-column table and scales with width; the EXPLAIN evidence is a `Custom Scan` grep; the heap `VACUUM ANALYZE` asymmetry is intentional and does not touch the measured pair). Filed follow-up issue #102 (`build_arrow` `try_into().unwrap()` should be a typed error on truncated stored bytes). (M100)
- **M100 Phase D (safety hardening + measured OLAP benchmark — completes the M100 DoD):** the vectorized executor's DataFusion `RuntimeEnv` now uses a `GreedyMemoryPool` bounded to `work_mem` (returns a typed `ResourcesExhausted` → clean SQL error instead of OOM-panicking) and `target_partitions = 1` (single-thread `Send`-pinning — no second thread ever touches the PG pointers behind the Arrow batch), on top of the `HeldInterrupts` guard around `block_on` (the async-in-C safety discipline, DoD item 3). **MEASURED benchmark** (`docs/benchmarks/m100-datafusion-executor.{md,json}`, `theodb_rs/isolation/bench_m100.sh`): 2,000,000 rows, `count(*), sum(measure)`, 5 runs, single-threaded — **the vectorized CustomScan (531 ms) is 9.89× FASTER than the M99 row-at-a-time seqscan (5251 ms) on the SAME columnar data** (projection pushdown + no heap-tuple form/deform + Arrow aggregate), result-identical to heap, EXPLAIN-confirmed as the CustomScan node. Honest ceiling: the gain is vs the M99 seqscan; heap (147 ms) is still faster for this single narrow aggregate (no decode overhead) — the columnar advantage grows with wider projections / GROUP BY / larger-than-RAM scans; **no superiority claim vs heap or AlloyDB in-core** (Rule 5 / M73/M97). (M100)
- **M100 Phase C (planner `CustomScan` integration — the single-plan vectorized aggregate, the M100 headline):** new `am/columnar_agg.rs` installs a `create_upper_paths_hook` (`UPPERREL_GROUP_AGG`) that intercepts a simple `count(*)` / `sum(float8)` aggregate (no GROUP BY/HAVING/WHERE/DISTINCT/window) over a `theodb_columnar` base table and replaces it with a `CustomScan` (`scanrelid=0`, `custom_scan_tlist` = the aggregate output) that runs the DataFusion vectorized executor and emits the result as one tuple. Admission is fail-safe (any unsupported shape → the native plan; the hook never errors) and gated behind the new `theodb.enable_columnar_agg` GUC (default OFF). **MEASURED on droplet (pg17): `EXPLAIN` over a columnar `count(*), sum(measure)` shows the CustomScan node, and `count(*)` / `sum(measure)` over a 40000-row `theodb_columnar` table (GUC on) are result-identical to the same aggregates over a heap table** (`m100_columnar_agg_customscan_matches_heap` pg_test; full suite 300 GREEN, zero regression). This closes the M100 DoD headline (a DataFusion CustomScan over the M99 TAM in a single plan, result-equivalent to a row-store — unlike pg_duckdb's two-engine ceiling). Slice-1 scope (type-matching cases without a cast); GROUP BY / WHERE pushdown / `avg` / `sum(int/numeric)` + the `work_mem` MemoryPool + per-batch interrupt safe-points + the measured OLAP benchmark are the follow-up slices (Phase D). (M100)
- **M100 Phase B (projection pushdown — the columnar performance lever):** `columnar::decode_columns` now takes a `projection: Option<&[usize]>` and decodes + returns ONLY the requested columns — skipping `read_chunked`/zstd on unprojected columns; `column_index(rel, name)` resolves a name to its attribute index. `df_executor` projects the aggregate to just its numeric column. **MEASURED on droplet: a `count(*)` + `sum(measure)` over a WIDE 6-column, 30000-row `theodb_columnar` table decodes only the `measure` column and returns the correct result** (`m100_projection_decodes_only_aggregated_column` pg_test; full suite 299 GREEN, zero regression). Min/max skip-pruning consumption (the other Phase B lever) + the planner `CustomScan` integration (C) + safety hardening/benchmark (D) follow. (M100)
- **M100 Phase A (DataFusion vectorized executor over `theodb_columnar` — the async-in-C seam de-risked over REAL columnar data):** new `am/df_executor.rs` decodes a columnar table's visible stripes into Arrow arrays and drives a vectorized DataFusion aggregate (`count(*)` + `sum`, DataFrame API — no SQL parser feature) to completion with a synchronous `block_on` inside the backend, under a `HeldInterrupts` guard so a mid-flight query-cancel cannot siglongjmp past the live tokio runtime. Exposed `columnar::decode_columns` (per-column value vectors incl. same-xact pending) as the Arrow-batch input. **MEASURED on droplet (pg17): a `count(*)` + `sum(measure)` over a 50000-row `theodb_columnar` table via the DataFusion path is result-identical to the same aggregate over a heap table** (`m100_df_columnar_agg_matches_heap` pg_test; full suite 298 GREEN, zero regression). This de-risks the pillar's #1 hazard (Drawback #2, HIGH — async runtime in a sync C callback) over real columnar Arrow batches BEFORE the planner wiring. Follow-up phases: projection pushdown + min/max skip-pruning consumption (B), planner `CustomScan` integration + EXPLAIN node (C), the `work_mem` MemoryPool + per-batch interrupt safe-points + measured OLAP benchmark (D). Own-code glue (Rule 9); Apache-2.0 `datafusion`/`arrow` the adopted engine. (M100)

## [0.87.0] - 2026-07-16

### Added
- **M99 /review sign-off (council-index-storage + council-rust-pgrx + council-benchmark = READY_TO_MERGE):** the three domain councils reviewed the final implementation (storage/WAL/MVCC, FFI safety, benchmark honesty) and signed off with zero blockers. Applied their non-blocking corrections: a compile-time `assert!(cfg!(target_endian = "little"))` guard on the column-major byval encoding (fail the build on a big-endian target, not silently at runtime); and 3 honesty qualifiers on the benchmark doc (9.2× compression is dataset-dependent not universal; the `columnar.stripe` catalog heap is not counted in the on-disk size; result-equivalence here is count/sum, GROUP BY correctness is the isolation suite). Filed 2 follow-up issues: #99 (WRITE_STATES flush unbounded → OOM on a giant single-xact INSERT...SELECT) and #100 (`relation_estimate_size` returns tuples=0 → planner blind). (M99)
- **M99 Phase D2 (crash-safety WAL-replay + honest columnar-vs-heap benchmark — completes the M99 DoD):** (1) **Crash-safety** (`theodb_rs/isolation/crash.sh`): a committed columnar INSERT of 10000 rows survives an *immediate* (crash) shutdown + recovery byte-for-byte — **MEASURED: PRE=POST count 10000, sum 50005000, sample `v5000`, 1 catalog stripe, all identical after WAL replay** (the column-chunk/header pages are GenericXLog-WAL'd, the visibility-granting `columnar.stripe` row is heap-WAL'd; crash-before-commit ≡ abort, already proven by the D1 `columnar_abort_vs_reader` permutation). (2) **Benchmark** (`docs/benchmarks/m99-columnar-tam.{md,json}`, `theodb_rs/isolation/bench.sh`): 1M rows × 4 columns, 5 runs, single-threaded, on the droplet — **MEASURED: 9.2× on-disk compression (columnar 6.5 MB vs heap 60.2 MB), aggregates result-identical to heap.** Scan wall-time is honestly **slower** (full-aggregate 2331 ms vs heap 88 ms; GROUP BY 2887 ms vs 179 ms) **by design** — M99 has no projection/skip/vectorization pushdown (a plain seqscan decodes every column of every chunk group and reconstructs full heap tuples), so the win is on-disk size; scan speed is the **M100** deliverable (which consumes the min/max directory + projection this milestone stores). **No superiority claim** (Rule 5 / M73/M97). (M99)
- **M99 Phase D1 (MVCC isolation permutation proofs — the correctness GATE):** wired a standalone `pg_isolation_regress` harness (`theodb_rs/isolation/`, Citus-style — CI does not run `cargo pgrx test`) with 3 permutation specs, run against a temp instance of the pgrx-managed pg17 with the extension installed. **MEASURED — all 3 GREEN on the droplet:** (a) `columnar_reader_vs_writer` — a REPEATABLE READ reader sees count=1, another session commits a new stripe, the RR reader STILL sees 1 (snapshot held), a fresh xact then sees 2 → the `columnar.stripe` catalog row's visibility is correctly bound to the scan snapshot; (b) `columnar_abort_vs_reader` — an uncommitted writer's rows are invisible to a concurrent reader (count=1) and stay invisible after ROLLBACK (no leaked stripe); (c) `columnar_write_concurrency` — two concurrent OPEN transactions insert 5 rows each; after both commit the table has exactly 10 distinct rows (non-overlapping row_number ranges reserved under the metapage buffer lock; concurrent pre-commit flush correct). This closes the "MVCC-correct columnar is over-claiming without isolation permutations" gate (ROADMAP M99 DoD #3). Also fixed a real bug the single-backend pg_tests could not catch: SPI at a flush point (`finish_bulk_insert` / pre-commit) ran without a pushed active snapshot (`ERROR: cannot execute SQL without an outer snapshot or portal`) — now wrapped in `PushActiveSnapshot(GetTransactionSnapshot())` when none is set (no-op during a scan, so the SPI read still honors the query's isolation-level snapshot). (M99)
- **M99 Phase C2a (MVCC via a heap catalog — `columnar.stripe`, ADR-0042 D2):** moved the stripe directory off the metapage (physical/WAL state that is durable regardless of the xact's commit/abort — an MVCC violation: an uncommitted or aborted INSERT's stripe would be visible) into an ordinary heap catalog `columnar.stripe (relid, stripe_id, header_block, row_count, first_row_number, ncols)`. A stripe is now visible to a scan IFF its catalog row is visible under the scan's snapshot — delegating snapshot isolation, WAL, crash recovery and abort-rollback to Postgres. The metapage keeps only the monotonic reservation counters; the on-disk TCS1 header already indexes chunks, so ONE catalog table suffices (no chunk_group/chunk tables — council-index-storage). Writes flush at xact **pre-commit** (a plain `INSERT ... VALUES` never fires `finish_bulk_insert`) via a `RegisterXactCallback`; same-xact reads append the backend's not-yet-flushed pending rows (thread-local, no cross-xact leak); the catalog insert (SPI, inheriting the xact's xmin) is the LAST write, after every data page is durable. **MEASURED on droplet: the catalog is the visibility root — 0 catalog rows before flush (rows visible via the same-xact buffer), exactly 1 after, count correct through the catalog; INSERT→SELECT (incl. NULLs, text across chunk-group boundaries, float) result-identical through the encode→disk→decode + MVCC-catalog-read path** (`m99_mvcc_catalog_is_visibility_root` + `m99_stripe_is_column_major` pg_tests; full suite 296 GREEN, zero regression). **Honest scope:** the cross-xact permutation proofs (uncommitted-invisible / REPEATABLE-READ-holds-snapshot / abort-leaves-nothing) are the Phase D `pg_isolation_regress` gate — single-session tests prove the catalog *is* the root, not race-freedom. A `sql_drop` event trigger reclaims a dropped columnar table's `columnar.stripe` rows so a later OID reuse can never inherit stale stripes (`m99_drop_table_reclaims_catalog_rows` GREEN). Known follow-ups: min/max skip-pruning + projection *consumption* land with the M100 CustomScan qual/projection pushdown (a plain TAM seqscan receives no quals as scan keys), so min/max is *stored* here and *consumed* there. (M99)
- **M99 Phase C1 (real COLUMN-MAJOR stripe encoding + per-chunk min/max — the actual columnar layout):** replaced the row-major zstd-blob stripe payload with a true column-major format (magic `TCS1`): each stripe is a grid of `[chunk_group (10k rows)][column]` chunks, each chunk = `zstd(null_bitmap + packed present values)` addressed by a fixed-stride directory, plus per-chunk min/max for skip-pruning. The bit-layout codec lives in a new FFI-free `am/columnar_codec.rs` (locally unit-tested — 11 pure `#[test]`s green offline), keeping the segfault-prone FFI (datum extraction, varlena detoast via `pg_detoast_datum_copy`+`pfree`, byval LE serialization, tuple reconstruction) isolated in `columnar.rs`. Column values are packed present-only with a separate null bitmap; min/max is stored for the native-ordered builtin types (int2/4/8, float4/8, bool) and falls back to "cannot skip" for the rest (never fail-wrong). **MEASURED: a 25000-row insert produces a `TCS1` stripe with 3 chunk groups × 3 columns, chunk-group-0/column-0 (`a int`, rows 1..10000) carrying min=1/max=10000, and INSERT→SELECT is result-identical through the new encode/decode incl. a text value round-tripping across a chunk-group boundary** (`m99_stripe_is_column_major` pg_test; the existing round-trip/compression/registration/reservation tests stay GREEN). Design reviewed by council-index-storage (on-disk format + crash-safety invariant: stripe visible only after the metapage descriptor is pivoted last) + council-rust-pgrx (FFI safety idioms). Skip-pruning *consumption* (applying min/max vs quals) + projection pushdown are Phase C2. (M99)
- `am/page.rs::extend_page_with_item` now returns the `BlockNumber` it received (P_NEW), so the columnar directory records real blocks instead of assuming contiguity from a pre-read count — robust to a concurrent backend's interleaved extend (council-index-storage). Existing call sites ignore the return value. (M99)

## [0.86.0] - 2026-07-14

### Added
- **M99 Phase A (columnar TAM registration spike — the de-risk slice):** registered an own-code `theodb_columnar` append-only Table Access Method (`CREATE ACCESS METHOD theodb_columnar TYPE TABLE HANDLER ...`) in Rust/pgrx 0.19 (pg17). All 45 `TableAmRoutine` callbacks are non-NULL: relation lifecycle (`relation_set_new_filelocator` creates storage + sets relfrozenxid like heapam) + slot/scan lifecycle + empty seqscan are real; UPDATE/DELETE/parallel/bitmap/sample/index-fetch are typed-`error!` stubs (append-only surface, ADR-0042 D4). **MEASURED: `CREATE TABLE ... USING theodb_columnar` loads end-to-end, registers in `pg_am`, empty seqscan returns 0 rows, DROP works** (`m99_columnar_am_creates_table` pg_test GREEN; 279 existing tests GREEN, no regression). Key correctness fix: the TAM routine is built ONCE in `TopMemoryContext` and returned for every columnar relation — PG stores the routine pointer directly in `rel->rd_tableam` without memcpy (unlike index AMs), so a transient-context allocation dangles and segfaults on the next statement. The write path (stripe/chunk/zstd + `columnar.stripe` catalog) is Phase B; read+MVCC+pruning is Phase C; isolation proofs + crash-safety + benchmark are Phase D. (M99)
- **M99 Phase A2 (columnar metapage + monotonic reservation):** the columnar fork's block 0 is a fixed metapage (magic `TCOL`, version, `reserved_row_number` + `reserved_stripe_id` counters), initialized at `CREATE TABLE`. Reservation is a read-modify-write of block 0 under a buffer EXCLUSIVE lock, WAL-logged full-image via `GenericXLog` (reuses `am/page.rs` — Rule 9), so concurrent inserters get non-overlapping id ranges (the synthetic-TID/stripe-id source for Phase B). **MEASURED: 1000 sequential reservations return 0..999 gap-free + a batch-of-5 advances the counter correctly** (`m99_reserve_row_number_monotonic` pg_test GREEN). Cross-backend non-overlap + crash-durability are proven in Phase D. (M99)
- **M99 Phase B/C1 (write path + reader — INSERT→SELECT round-trip):** wired `tuple_insert`/`multi_insert`/`finish_bulk_insert` (accumulate rows per backend) + flush-to-stripe (write row blobs across data pages, reserve the row_number range, append a stripe descriptor to the metapage, all WAL-logged via `GenericXLog` so an aborted xact rolls the stripe back) + the seqscan reader (materialize every stripe's rows at `scan_begin`, deform each into a virtual slot via `heap_deform_tuple`). **MEASURED: INSERT of 5001 rows into a `theodb_columnar` table reads back result-identical — `count`/`sum(int)`/`sum(float8)`/text values/NULL handling all match** (`m99_insert_select_roundtrip` pg_test GREEN; A1+A2 still GREEN; 279 existing GREEN, no regression). **Honest scope:** this slice stores rows as formed heap-tuple bytes (row-major on disk) — a correct, general round-trip proving the storage+retrieval + stripe/metapage machinery. The true column-major encoding (per-column chunks + zstd compression + min/max skip-pruning — the actual columnar *benefit*) is the follow-up refactor; TDD order is correct-first. Single-transaction MVP visibility; snapshot-scoped cross-backend MVCC is Phase C2/D. (M99)
- **M99 (zstd stripe compression — the measurable columnar space benefit):** each stripe's payload is zstd-compressed (level 3, the DuckDB/Parquet default) before being written to data pages, and decompressed on scan (`zstd` reused from the tree via datafusion/arrow — parsimony rung 4, MIT/BSD, D1-clean). **MEASURED: 20000 rows with a `repeat('x',200)` column occupy < HALF the on-disk size of the same rows in a heap table** (`m99_stripe_compression_shrinks_ondisk` pg_test compares `pg_relation_size` columnar vs heap; round-trip still identical through the compress/decompress path; full suite GREEN, no regression). Per-column chunking + min/max skip-pruning (the *skip* half of the columnar benefit) is the follow-up slice. (M99)

## [0.85.0] - 2026-07-14

### Added
- Roadmap amended: single-planner columnar+AI pillar (AlloyDB-class HTAP) — 6 milestones M98-M103 from the `single-planner-columnar-ai` discovery (blueprint SHIPPABLE 98.8, GO-CONDITIONAL): M98 pgrx-0.19-upgrade + DataFusion/Arrow coexistence spike (the GATE), M99 append-only columnar TAM (Hydra-model, Apache-2.0), M100 DataFusion CustomScan vectorized executor (the single-planner seam), M101 heap-authoritative Arrow columnar cache (MVCC-correct HTAP), M102 AI operators as pushable plan nodes (LOTUS/Palimpzest), M103 vector+columnar unified substrate (Lance-inspired). Honest ceiling locked in every DoD: DuckDB/Photon-class 15-30× on columnar-resident data — capability-match AlloyDB, never superiority over its in-core engine (M73/M97). Supersedes ADR-0041's DEFER + corrects its Hydra-license error (Apache-2.0, not AGPL) (M98, M99, M100, M101, M102, M103)

## [0.84.1] - 2026-07-13

### Fixed
- Integrity: commit the M95 review HIGH-1 fix to `customscan.rs` (`term_B` uses `indextotalcost` for a single-predicate `IndexPath` — mirroring `cost_bitmap_tree_node`, no heap double-count — instead of `.total_cost`) + the `m95_multi_predicate_filter_correct` regression test. These were reviewed + tested green on the droplet (the M96/M97 277-test runs used them via the working tree) but the `fix(m95 review)` commit staged only the blueprint `.md`, so v0.82.0–v0.84.0 shipped without them; the released source now matches the reviewed/tested state (the page.rs HIGH-2 bounds guard was already committed). Plan-cost only — no user-visible behavior change (the node isn't auto-selected, R4) (M95)

## [0.84.0] - 2026-07-13

### Added
- **M97 (Columnar/HTAP (D2) discovery, veredito `DEFER` — discovery-only, ZERO product code):** a rigorous, web-grounded (R0) answer to "is a NEW columnar pillar worth months?" — **DEFER**. The only D1-permissive columnar route (pg_duckdb + DuckDB, MIT) is ALREADY shipped (M61/M62/M64, ADRs 0020/0021/0023); every "go further" differentiator is **license-barred** (moonlink/pg_mooncake sync = BSL 1.1; Hydra columnar + Citus columnar = AGPLv3 — all barred by D1) or **paradigm-blocked** (TheoDB is structurally two engines / two planners — ADR 0023 — so it cannot match AlloyDB's in-core in-memory single-planner columnar engine; the M73 vector lesson applied). **Viability benchmark MEASURED (20M-row `hits`, same box): DuckDB columnar 15–23× faster than PG row-store on analytical aggregations** (`docs/benchmarks/m97-htap-viability.{md,json}`) — confirming columnar's value AND that the shipped pg_duckdb already delivers it (no new differentiator to chase). Deliverables: blueprint (SHIPPABLE 98.8, `knowledge-base/discoveries/blueprints/columnar-htap-blueprint.md`) + the viability benchmark + the DEFER decision ADR (`docs/adr/0041-m97-columnar-defer.md`, owner sign-off pending) with a moonlink-license watch-item. The honest terminal: deliver KNOWLEDGE, position honestly ("on-demand vectorized columnar via pg_duckdb, a lakehouse D2 bet — NOT AlloyDB's in-memory-auto engine"), don't over-invest chasing a closed/barred SOTA. (M97)

## [0.83.0] - 2026-07-13

### Added
- **M96 (tuplesort-streaming ambuild, veredito `READY_TO_MERGE`): the IVF-AQ v5 build no longer materializes the corpus — peak build RAM is now `O(maintenance_work_mem + sample)`, independent of N.** Mirrors pgvector's `ivfbuild.c` (PostgreSQL License — study, own code): two heap scans (sample-train the centroids + AQ codebook on a bounded 200k prefix, then stream-assign each vector to its nearest centroid inline and `puttupleslot` it into a `tuplesort` that spills past `maintenance_work_mem`), `performsort` by list#, and write the pages list-by-list from the sorted read-back (one list in flight, O(N/lists) buffer). **MEASURED (DO Xeon Platinum 8358, dim 128, mwm=256MB): peak RSS FLAT across a 10× data range — 1M 0.65GB / 3M 0.62GB / 10M 0.56GB, ratio-vs-base collapsing 1.26×→0.11×** (`docs/benchmarks/m96-streaming-build.{md,json}`) — the definitive O(mwm) signature, vs the M88 in-RAM 4.21×-base build that OOM'd at 30M. 30M/100M peaks honestly PROJECTED from the flat curve (~0.6GB vs 64.7GB OOM / impossible), NOT fabricated — the single-threaded assignment wall-clock makes a direct 100M build impractical here (parallel-assign is the deferred follow-up). A per-row bytea leak was found BY the measurement (1.84→0.65GB at 1M) and fixed. Byte-identical v5 on-disk format (no REINDEX); the ≤mwm in-RAM fast-path stays byte-identical; streaming is recall-EQUAL (bounded-sample training). Dispatch is exact on the layout flags — SQ8/v6, label/v7, SOAR keep the in-RAM build (never a silent wrong path); streaming v6/v7 + parallel assignment are documented follow-ups. 277 tests GREEN (4 new: tuplesort FFI roundtrip + 50k-row external spill, streaming recall-in-band, streamed-scan durable). Sign-off council-rust-pgrx (1 HIGH found + fixed — missing `#[pg_guard]` on the two build-scan callbacks → panic/longjmp-across-C). NOT a QPS claim (teto M73/M82). (M96)

### Fixed
- Roadmap amended: added M96 tuplesort-streaming ambuild (M96)

## [0.82.0] - 2026-07-13

### Added
- **M95 (honest vecfilter cost model, veredito `READY_TO_MERGE`): the spike's forced `total_cost = min_cost × 0.1` selection heuristic is replaced by an HONEST cost** = term_B (the bitmap sub-plan's produce-only cost — `indextotalcost` for a single-predicate `IndexPath`, no heap-fetch double-count) + term_V (`cost::vecfilter_scan_cost`, re-derived from the bitmap selectivity via `cost::effective_probes`, imaging the M91 adaptive loop; the child IndexPath cost is probe-blind so it cannot be reused). Fail-safe (EC-3): any unreadable meta / null bitmapqual / degenerate input degrades to NOT adding the node (native plan wins) — a `set_rel_pathlist_hook` must never error. The forced hack that made the node hijack EVERY filtered query is gone (`m95_loose_selectivity_not_chosen`). **MEASURED (SIFT1M, DO Xeon Platinum 8358):** the honest cost correctly PREVENTS over-selection; the planner does not auto-select the node at any selectivity because the native post-filter competitor is probe-blind/under-priced (M48 `amcostestimate` unchanged — the blueprint's predicted R4). The node stays correctness-critical: native POST recall 0.55-0.67 vs forced INLINE 0.88-0.95 across 1-25% selectivity + 13× QPS at 1% — the planner cannot see recall (`docs/benchmarks/m95-cost-model.{md,json}`). **Resolution: new `theodb.vecfilter_force` GUC (default off)** — an explicit user override (same rationale as the `enable_*` knobs) for a selective filter whose recall the planner is blind to; the honest cost is the safe default. 273 tests GREEN (6 cost unit tests + loose-not-chosen + multi-predicate regression); no page-format change; GUC-off byte-identical. Sign-off council-index-storage (2 HIGH found in review — heap-fetch double-count + a planner-hook longjmp on a torn meta page — both fixed). Follow-up (tracked): making M48 probe-aware for the filtered case would unlock auto-selection. NOT a QPS-superiority claim vs ScaNN/AlloyDB (teto M73/M82). (M95)

### Fixed
- `read_page_item_into` now bounds-checks `block < nblocks` (mirroring `read_page_item_at`) — a torn/concurrently-folded meta page no longer raises a C `ereport(ERROR)` longjmp that would abort ALL query planning from a planner hook; it degrades to a typed `Err` → fail-safe (M95 review HIGH-2; also hardens the M48 amcostestimate read path)
- Roadmap amended: added M95 honest cost model for the vecfilter node (M95)

### Deprecated

### Removed

### Fixed

### Security

## [0.81.0] - 2026-07-13

### Added
- **M94 (per-scan membership scoping, veredito `READY_TO_MERGE`): filtered `UNION`/self-join/partitioned-`Append` vector queries now WORK** — the capability the M93 fail-loud guard refused. Each vecfilter Custom Scan node stores its membership in a thread-local registry keyed by the node pointer and installs it only during its own synchronous child-pull windows (RAII swap-discipline, re-entrant for SubPlan nesting); xact/subxact-abort callbacks close the longjmp-leak paths (incl. PL/pgSQL `EXCEPTION` = subxact abort, and `PREPARE TRANSACTION`). Resolves the M92/M93 review's convergent BLOCKER (per-backend membership cross-contamination) — the owning council re-reviewed against the PG17 source and declared it "genuinely fixed, not papered over". New pg_tests: UNION of two filtered scans == union of exact seqscans (both nodes asserted in the plan), rescanned inner correct, subxact abort clears a stale membership. **265 tests GREEN**; benchmark spot-check recall **byte-identical** to v0.80.1 at every point (QPS delta = droplet host variance, both arms uniformly; documented). (M94)

### Fixed
- vecfilter: a fresh `TIDBitmap` was leaked on every node begin/rescan (`ExecEndBitmapIndexScan` does not free it — the prior comment claiming otherwise was wrong); now freed immediately after materialization (M94 review MEDIUM-2)
- vecfilter: the membership swap-restore is now unwind-safe via an RAII guard (a PG error inside the child pull no longer relies solely on the abort callbacks) (M94 review MEDIUM-1)
- vecfilter: the planner hook now requires unparameterized children — a parameterized LATERAL bitmap path would have violated the node's `param_info = NULL` contract; such queries fall back to native plans (M94 hardening)
- Roadmap amended: added M94 per-scan membership scoping (M94)

## [0.80.1] - 2026-07-13

### Fixed
- Integrity: ship the v5 selectivity-adaptive probing loop + the Pareto-frontier benchmark harness that the M92/M93 263-test suite and the SIFT `INLINE-dominates-POST` measurement actually ran against — they were left uncommitted when v0.80.0 was cut, so the released source now matches the benchmark artifact (`docs/benchmarks/m92-arbitrary-where.{md,json}`). No behavior change at the benchmarked 1%/5% selectivity; the v5 adaptive only materially affects ultra-selective (<0.1%) recall (M92)

## [0.80.0] - 2026-07-13

### Added
- **M92/M93 (arbitrary-WHERE filtered vector search via a Custom Scan Provider, veredito `GO` — experimental, OFF by default behind `theodb.enable_vecfilter`): push an arbitrary scalar `WHERE` INTO the IVF-AQ vector scan.** A hand-rolled 2-child Custom Scan node intercepts `WHERE <scalar> ORDER BY e <-> q LIMIT k`, runs the planner's native bitmap sub-plan over the scalar column (Rule 9 — reuses BitmapAnd/Or), materializes a lossy-safe TID membership, and the vector scan's Stage-1 skips non-members inline (+ M91 adaptive probing); the vector child's own qpqual Filter is the MVCC recheck of the lossy/pending over-admits. **MEASURED (DO 8-vCPU Xeon Gold 6548N, SIFT1M, real neighbors): INLINE dominates the native post-filter on BOTH recall AND QPS — 1% sel recall 0.953 @ 266 QPS vs POST 0.673 @ 21 QPS (+0.28 recall, ~12× QPS); 5% sel 0.915 @ 126 vs 0.593 @ 92 (+0.32, ~1.4×)** (`docs/benchmarks/m92-arbitrary-where.{md,json}`). Correctness proven byte-identical to exact seqscan on a non-label column (pending + lossy rechecked); the inline skip engages on both the v5 plain-vector and v7 label layouts. **263 tests GREEN, GUC-off path byte-identical.** Concurrent filtered vector scans in one plan (UNION/self-join) **fail loud** (per-backend membership; per-scan scoping is a follow-up) — never silently wrong (Rule 8). Sign-off council-rust-pgrx + council-index-storage + council-benchmark (1 BLOCKER + 3 HIGH found in review and fixed). NOT a QPS-superiority claim vs ScaNN/AlloyDB (teto M73/M82) — the AlloyDB "inline filtering" tier ③ mechanism in a permissive OSS Postgres extension. (M92, M93)
- Roadmap amended: added M92 arbitrary-WHERE Custom Scan Provider + M93 Custom Scan node integration (`/roadmap-feature`) (M92, M93)

## [0.79.0] - 2026-07-13

### Added
- Selectivity-adaptive probing on the v7 INLINE filtered scan (M91): a selective label filter automatically probes more IVF lists until the matching-candidate pool fills, recovering filtered recall@10 from 0.741 to ~1.0 at 0.01% selectivity on SIFT1M while leaving loose/unfiltered scans byte-identical. Self-tuning on the measured match count — no new GUC, no on-disk format change (no REINDEX). Opt-in `THEODB_SCAN_PROFILE=1` now reports `probes_effective` vs `probes_default` (M91)

## [0.78.0] - 2026-07-12

### Added
- **M90 (inline label filter, veredito `GO`): filtro de label empurrado PARA DENTRO da travessia do IVF-AQ** (Approach A — scan-key/label-in-index, o mecanismo do pgvectorscale, código próprio). Um índice `theodb_ivfflat (e, lbl)` com coluna `smallint[]` faz o planner empurrar `lbl && '{…}'` como Index Cond; o novo layout **v7** co-localiza o label nas code-pages e a Stage-1 PULA candidatos sem-overlap antes do rerank (`xs_recheck` garante correção). **MEDIDO (DO c-8, 500k, ~1% seletividade): recall@10 1.00 (inline v7) vs 0.52 (M87 post-filter v5) — delta +0.48 + ~19× QPS** (`docs/benchmarks/m90-inline-filter.{md,json}`, ADR `0040`). 253 pg_tests GREEN (250 + 3 v7: inline/vacuum/pending), zero regressão; vetor-only e v5/v6 sem-label byte-idênticos (v7 opt-in na 2ª coluna). Honesto: só a coluna de label + `&&`, format v7 + REINDEX p/ usar labels; NÃO é claim de QPS-superior vs ScaNN/AlloyDB (teto M73/M82); o arbitrary-WHERE inline (Custom Scan) é o M91. Sign-off council-index-storage + rust-pgrx + benchmark (2 blockers de correção achados no review e corrigidos: VACUUM no-op v7, xs_recheck no pending). (M90)
- Roadmap amended: added M91 adaptive filter strategy (pre/inline/post pela cardinalidade do bitmap — a peça adaptive AM-local; gated M90) (`/roadmap-feature adaptive-filter-strategy`) (M91)
- Roadmap amended: added M90 inline filter pushdown (bitmap-in-traversal via Custom Scan — fecha o inline filtering vs AlloyDB; gated M87/M89) (`/roadmap-feature inline-filter-pushdown`) (M90)

### Deprecated

### Removed

### Fixed

### Security

## [0.77.0] - 2026-07-12

### Added
- **M89 (build escalável — ambuild streaming, veredito `DOD_MET`): o build do índice vetorial agora tem memória limitada por-lista.** Fecha o teto de memória descoberto no M88 (ADR-0038): o `ambuild` do `theodb_ivfflat` picava ~4× o dataset base em RAM → OOM a 30M. Duas mudanças byte-idênticas ao formato on-disk (sem REINDEX): (1) `build_owned` **move** o corpus p/ o índice (sem clonar); (2) os writers v5/v6 leem os vetores por referência e **escrevem cada lista incrementalmente**, liberando o blob f32 por-lista (elimina o clone `list_entries()` + os buffers `enc_vec`/`items`). **MEDIDO (DO m-8vcpu-64gb, 30M×128 = 15.4 GB base):** o build de 30M agora **completa** num box de 64 GB com pico **1.28× (v5) / 1.50× (v6)** base — o build antigo OOMou a **4.21×/64.7 GB** (reproduz o M88). 250 pg_tests GREEN, zero regressão. Honesto: NÃO é `O(maintenance_work_mem)` (o pico ainda tem a cópia 1× `idx.vectors`) → 100M+ ainda não cabe em RAM commodity; o streaming via `tuplesort` dos vetores é o follow-up. `docs/benchmarks/m89-ambuild-streaming.{md,json}`, ADR `0039`. Sign-off council-index-storage + council-rust-pgrx + council-benchmark. (M89)
- Roadmap amended: added M89 ambuild streaming (flush incremental via `tuplesort` nativo — derruba o teto de memória de build ~4×→~1× base descoberto no M88; gated M88) (`/roadmap-feature ambuild-streaming`) (M89)

### Deprecated

### Removed

### Fixed

### Security

## [0.76.0] - 2026-07-12

### Added
- **M88 (Roadmap v7 — veredito terminal da track storage-separation, `SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE`).** A medição terminal da separação de armazenamento SQ8 (v6) vs f32 (v5) no regime out-of-RAM. **Medido a 16M** (DO m-8vcpu-64gb, sign-off council-benchmark): índice v6/SQ8 **3.52× menor** que v5/f32 (confirma o 3.5× do M85 a 16× a escala); **+21% cold-QPS a probes=32** (direcional, limite inferior). **Honesto:** o DoD ≥100M **NÃO foi atingido** — o ambuild pica ~4× o base em RAM (2 OOM-kills medidos a 30M: 47 GB, 64 GB anon-rss num box de 62 GB usáveis), 16M foi o maior build viável; a recall (0.291) é degenerada por dados sintéticos tie-saturados (SIFT1M real deu 0.98 no mesmo código, M84). Crossover QPS out-of-RAM fica direcional-não-provado; superioridade sobre ScaNN/AlloyDB **não é reivindicada** (teto de paradigma M73/M82 permanece). `docs/benchmarks/m88-billion-scale-verdict.{md,json}`, ADR `0038` (estende `0037`). Follow-up recomendado: ambuild streaming (derruba o teto ~4×-base) + dados bilhão-scale reais. (M88)
- **M88 Phase 1 — build IVF escalável.** kmeans-train sampling (subsample determinístico por stride, capado em `KMEANS_TRAIN_SAMPLE=1.1M`) + parallel full-N assignment (`assign_all_parallel`, `std::thread::scope`) — ataca o O(N·k·d) que era o gargalo real a 100M+ (custo de kmeans fixo ~1M-scale). **Byte-idêntico a ≤1M** (todos os testes + benchmarks 1M inalterados); **249 pg_tests GREEN**. Melhoria de produto (build escalável), não só p/ o M88. (M88)

### Deprecated

### Removed

### Fixed
- **M87 — teste de regressão do filtered ANN commitado.** O `filtered_ann_v5_iterative_preserves_recall` (parte dos 248 pg_tests GREEN reportados no M87, validado no run do M87) ficou uncommitted no release v0.75.0; agora está no tree. (M87)

### Security

## [0.75.0] - 2026-07-12

### Added
- **M87 (Roadmap v7 — filtered ANN + planner, veredito GO): iterative scan para TODO IVF (v3/v4/v5/v6).** O iterative do M52 era HNSW-only, então um `WHERE` seletivo COLAPSAVA o recall no IVF (os candidatos dos primeiros probes eram filtrados, o AM retornava false). Agora os scans IVF retornam `Vec` + recebem `probes`/`rerank_pool` como param, e o re-search iterativo cresce **probes** (alcança listas não-probed) E o **rerank pool** até emitir `max_scan_tuples` tids distintos (recall preservado); dedup-by-tid via o `emitted` HashSet do `amgettuple`. `amcostestimate` já era v5/v6-aware. **Medido a SIFT1M:** filtered recall@10 **0.894 @ 10% sel, 0.942 @ 30%** (sem o fix colapsaria); EXPLAIN confirma `Index Scan` para a query filtrada ordenada. `docs/benchmarks/m87-filtered-ann.{md,json}`. **248 pg_tests GREEN (247 + 1 M87), zero regressão.** Classe pgvector-relaxed_order; NÃO é o inline/adaptive filtering do AlloyDB (gap de paradigma). Fecha o escopo M85-M87.

### Deprecated

### Removed

### Fixed

### Security

## [0.74.0] - 2026-07-12

### Added
- **M86 (Roadmap v7 — SOAR spill, veredito HONEST-NEGATIVE no QPS SIFT1M): atribuição SOAR** (Sun et al. NeurIPS 2023, arXiv:2404.00774) atrás de `WITH (soar_lambda=N)` — cada vetor é spilled p/ uma 2ª lista escolhida pela loss de resíduo ortogonal-amplificado, então uma query com MENOS probes ainda o encontra. `ivf.rs::with_soar_spill` (~40 LoC), reloption `soar_lambda`; dedup-by-tid reusa o `emitted` HashSet do `amgettuple` (sem mudança de scan). **Medido a SIFT1M (A/B vs no-SOAR):** o lever centroid-probe é REAL (recall +0.12 a probes=4, +0.06 a probes=8), mas **NÃO dá ganho de QPS** (0.66-0.80× em todo ponto) — o bind do SIFT1M é o read da Fase 2 (M85), não o nº de probes; e a impl mínima dobrou o índice (f32 duplicado no layout v5 per-list). `docs/benchmarks/m86-soar-spill.{md,json}`. **247 pg_tests GREEN (246 + 1 SOAR), zero regressão.** Opt-in (default 0=off); veredito honest-negative no SIFT1M (o ganho projeta-se a bilhão-scale/M88). NÃO vence o ScaNN-biblioteca (M73/ADR-0035).

### Deprecated

### Removed

### Fixed

### Security

## [0.73.0] - 2026-07-11

### Added
- **M85 (Roadmap v7 — SQ8 refine tier, veredito GO memory-win): índice IVF-AQ v6 SQ8-REFINE** atrás de `WITH (separate_storage=1, refine=1)` — o rerank da Fase 2 lê códigos SQ8 (`dim` B/vec, 128B) em vez de f32 (512B). Novo quantizador `sq8.rs` (~90 LoC, sem lib — FAISS QT_8bit per-dim min/max, asymmetric decode-then-metric); layout v6 (`write_ivf_aq_split_sq8`/`read_ivf_aq_meta_split_sq8`/`read_sq8_at`/`ivf_is_v6`, reloption `refine`, cost/vacuum/pending v6-aware). **Medido a SIFT1M (A/B vs v5 f32): índice 3.5× MENOR (153 MB vs 528 MB) a ε≤2% de recall** (`docs/benchmarks/m85-sq8-refine.{md,json}`). **246 pg_tests GREEN (238 + 6 sq8 + 2 v6), zero regressão.** Honesto: o QPS-a-recall-casado é flat-to-marginal em warm-cache 1M (o decode SQ8 + a perda de recall compensam o ganho de I/O — caveat da pesquisa); o ganho de QPS/I/O compõe a bilhão-scale (M88, onde o índice 3.5× menor cabe em RAM e o f32 não). Perfil AlloyDB-SQ8-default; opt-in (v5 f32 exato continua default).

### Deprecated

### Removed

### Fixed

### Security

## [0.72.0] - 2026-07-11

### Added
- **M84 (Roadmap v7 — confirmação high-recall, veredito GO): o layout v5 storage-separated MANTÉM a vantagem a alta-recall.** Medido a SIFT1M (A/B same-data): frente de Pareto v5 vs v4 — recall 0.98 → **8.7×**, recall 0.998 → **5.0×**, recall 0.9985 → **8.1×**; todo ponto high-recall vence ≥3× (`docs/benchmarks/m84-recall-confirmation.{md,json}`). Tradeoff honesto: pool maior → mais random-reads f32 na Fase 2 → vantagem estreita no frontier extremo (motiva o M85 SQ8). recall v5==v4 lossless.

- **M83 (Roadmap v7 fase 0 — spike D3 GATE, veredito GO): índice IVF-AQ v5 STORAGE-SEPARATED** atrás de `WITH (separate_storage=1)` — os códigos AQ e os vetores f32 vivem em cadeias de páginas DISTINTAS, então o scan lê só os códigos compactos na Fase 1 (poda AH) e faz random-read do f32 só dos sobreviventes do rerank na Fase 2 (a alavanca que o ADR-0037/M82 nomeou). Novo `write_ivf_aq_split`/`read_ivf_aq_meta_split`/`read_vec_at` (`am/page.rs`), `scan_ivf_aq_split` (`am/scan.rs`), reloption `separate_storage` (`am/options.rs`); `main_index_pages`/VACUUM-gate/`amcostestimate` v5-aware. **Medido a SIFT1M (A/B same-data vs v4 interleaved): 2.7×–11.8× mais QPS a recall CASADO (6.2× @ probes=32), 3–14× menos buffer-accesses** (`docs/benchmarks/m83-split-storage-spike.{md,json}`). **238 pg_tests GREEN (236 + 2 v5), zero regressão; recall v5==v4 byte-idêntico (lossless).** Veredito GATE = **GO** para M84 (layout v5 produção). Caveats honestos: recall-teto ~0.80 deste run (rerank pool fixo em 64, investigação M84); ganho warm-cache é lower bound (bilhão-scale compõe, M88). NÃO vence o ScaNN-biblioteca (imposto de paradigma permanece, M73/ADR-0035).
- Deep research web-grounded (R0) do caminho **storage-separated ScaNN-fidelity** (a alavanca não-testada do ADR-0037): `docs/research/scann-storage-separation-2026-07.md`. Convergência de 4 SOTA (FAISS FastScan, AlloyDB ScaNN, VectorChord, pgvectorscale) — todos separam fisicamente códigos↔vetores brutos. Reformulação honesta do alvo (arXiv:2603.23710 SIGMOD 2026: 84.4% do tempo do ScaNN-in-PG é overhead de sistema; teto AlloyDB = ~4× sobre pgvector HNSW): meta ACHIEVABLE = classe AlloyDB-in-Postgres (~4–6× recuperável), jamais vencer o ScaNN-biblioteca. Roadmap v7 (M83 spike D3 gate → M84 layout v5 → M85 SQ8 refine → M86 SOAR → M87 filtered+planner → M88 bilhão-scale) adicionado ao `ROADMAP.md`.

### Deprecated

### Removed

### Fixed
- **M84 — rerank pool do scan AQ era um no-op latente:** `over_fetch().max(64)` ficava SEMPRE em 64 (over_fetch≤64, o `.max(64)` sempre vencia), então `theodb_hnsw.over_fetch` nunca alargava o pool de rerank AQ — a causa da recall-teto ~0.80 do M83. Corrigido para `64 * over_fetch()` (`am/scan.rs`, ambos os scans AQ v4/v5); default (over_fetch=1) inalterado em 64; over_fetch=8/32 → pool 512/2048 → recall sobe a 0.98/0.998. 238 pg_tests GREEN, zero regressão.

### Security

## [0.71.0] - 2026-07-11

### Added
- M82 (pg_scann fase 7 — veredito final): head-to-head MEDIDO do índice v4 IVF-AQ+AH como Access Method, dentro do
  Postgres, a SIFT1M completo (GT oficial válido a 1M) vs a baseline f32-IVF own-code na mesma tabela (rigor A/B
  same-data M46). Artefatos `docs/benchmarks/m82-pgscann-headtohead.{md,json}` + veredito `docs/adr/0037-m82-am-ivf-aq-measured-verdict.md`. **Achado honesto:** o índice v4 é funcionalmente correto (recall byte-idêntico ao f32-IVF exato — AH pruning lossless), mas **não entrega ganho de QPS** no AM (78.5 QPS @ recall 0.985, classe f32-IVF, ~24× abaixo do ScaNN) — os 5-7× in-memory do M75 são mascarados pelo custo I/O+probe do AM. Confirma e estende o veredito M73 (ADR-0035). Fecha o track pg_scann (M75→M82) e o Roadmap v6.

## [0.70.0] - 2026-07-11

### Added
- **pg_scann M81 — lifecycle transacional do índice IVF-AQ v4:** o `scan_ivf_aq` (`am/scan.rs`) agora **folda a região pending** (rows INSERTed pós-build, f32, scored exatamente) — antes eram silenciosamente perdidas; `main_index_pages`/`read_pending` ficaram v4-aware (`am/page.rs`). O VACUUM é **safe no-op** no índice v4 (`vacuum_rebuild` gate em `am/build.rs` — o rebuild f32 rejeitaria/corromperia; correção holds via fold do pending + MVCC re-check; compactação v4 = REINDEX, follow-up documentado). `amcostestimate` v4-aware (`am/cost.rs`). Provado: `ivf_aq_v4_folds_post_build_inserts` (INSERT pós-build aparece no scan) + **236 pg_tests GREEN, zero regressão**. Fecha ROADMAP M81.

## [0.69.0] - 2026-07-11

### Added
- **pg_scann M77+M78+M79+M80 — IVF-AQ+batched-AH no AM `theodb_ivfflat` (a capacidade que o M75 provou, agora em produção):** `CREATE INDEX ... USING theodb_ivfflat WITH (pq_subspaces=M)` persiste um layout **v4** (`am/page.rs::write_ivf_aq`) com os códigos AVQ 4-bit em blocks32 transpostos por inverted list (+ f32 para rerank + codebook), e o scan (`am/scan.rs::scan_ivf_aq`) faz probe → **`ah_score_block` batched (FastScan pshufb)** → rerank f32 exato — o scan 2-estágios provado no M75 (~5-7× QPS vs f32 a recall casado), lendo de página O(probes). Isolado do path v3 f32 (byte-idêntico, intocado). Provado: `ambuild_ivf_pq_subspaces_v4_scans_high_recall` (recall@10 ≥ 0.8 vs seqscan exato) + **235 pg_tests GREEN, zero regressão**. Fecha ROADMAP M77-M80. Honesto: benchmark recall×QPS a SIFT1M = M82 (exige otimizar o AVQ train super-linear); lifecycle aminsert/VACUUM do índice v4 = M81.

## [0.68.0] - 2026-07-11

### Added
- M76 (pg_scann Fase 1, AM scaffold) fechado por **Rule 9**: o AM `theodb_ivfflat` existente (registro IndexAmRoutine, ambuild, busca exata IVF, metapage+page+WAL GenericXLog, opclass, set-equal-vs-seqscan tests ~134 GREEN) **já é o scaffold** — o pg_scann ESTENDE o IVF AM (modo AQ+batched-AH), não cria AM novo. **Re-escopo honesto de M77-M82** (memória `pgscann-am-mostly-exists`): o delta real colapsa para (M77) layout block32 dos códigos AQ nas IVF-list-pages + (M79) o `scan_ivf_structured` usar o `ah_score_block` batched (o scan que o M75 provou ~5-7×); o resto (AVQ, aminsert, vacuum, cost, rerank-pool) já existe. Fecha ROADMAP M76.

## [0.67.0] - 2026-07-11

### Added
- M75 (pg_scann Fase 0, spike measurement-first): índice IVF-AQ+AH in-memory own-code (`theodb_rs/src/ann/ivf_aqah.rs`) — compõe (Rule 9) a partição IVF + o AVQ (`am/aq.rs`) + o kernel batched AH-LUT já existente (`vec/ah.rs`, layout transposed block32) num scan 2-estágios probe→AH→rerank. Pipeline provado correto (3 pg_tests GREEN). **Veredito D3 = GO (medido, SIFT real):** IVF-AQ+AH entrega **~5-7× o QPS do full-precision a recall casado** (captura ~5-7× dos ~25× do gap ScaNN M33) — 1º lever own-code que move o gap; reabre o eixo de QPS. Caveat honesto: medido a n=5000 (AVQ train naive super-linear bloqueia 1M in-session → otimização é M77). `docs/benchmarks/m75-ivf-aqah-spike.{md,json}`. Gate ABERTO: M76-M82 arrancam.
- DISCOVER cycle + ROADMAP v6 para o **pg_scann** (índice IVF-AQ+AH nativo — ScaNN own-code): blueprint web-grounded SHIPPABLE_WITH_CAVEATS (`.claude/knowledge-base/discoveries/blueprints/pg-scann-am-blueprint.md`, R0: AVQ paper + AlloyDB + arXiv:2603.23710 SIGMOD 2026) + 8 milestones M75-M82 (Fase 0 spike-gate D3 + 7 fases: AM scaffold → layout contíguo → AVQ → AH-scan → rerank → lifecycle → planner). Tese não-refutada (M59): AQ+AH sobre carrier IVF batch-scan; measurement-first (M75 é o gate, honest-negative é saída válida).

### Deprecated

### Removed

### Fixed

### Security

## [0.66.0] - 2026-07-10

### Added
- Veredito do lever condicional de quantização (M74, ADR-0036): RaBitQ é o lever viável não-refutado (core vendorizado, ADR-0032; spike D3 1M medido) — mas o ganho é **memória/billion-scale** (5.3MB @ 98.4%), NÃO superioridade de QPS. Decisão honesta (anti-sunk-cost/D3): não implementar o AM completo agora; full IVF-RaBitQ = follow-up gated por demanda billion-scale. Fecha ROADMAP M74 → **ROADMAP v5 (pilar vetorial P0) COMPLETO**.

## [0.65.0] - 2026-07-10

### Added
- Veredito MEDIDO do North Star vetorial (M73, ADR-0035 + `docs/benchmarks/m73-headtohead-verdict.{md,json}`): paridade own-code de recall classe-pgvector ALCANÇADA + throughput multi-cliente competitivo-a-superior (M72) + superioridade de QPS vs ScaNN/AlloyDB MEDIDA como não-alcançável por extensão PG permissiva (gap ~25-44× @ 0.99 é de paradigma). Estado medido final propagado ao CLAUDE.md North Star. Fecha ROADMAP M73.

## [0.64.0] - 2026-07-10

### Added
- Benchmark M72: QPS multi-cliente a 1M×128d (8 clientes concorrentes, ≥3 runs) — theodb_hnsw competitivo-a-superior vs pgvector a recall casado no regime clusterizado (+11% QPS @ ~0.91, build 3× mais rápido), com caveat honesto de corpus gaussian-mixture vs SIFT1M literal (`docs/benchmarks/m72-qps-multiclient.md`, `benchmarks/run_m72_multiclient.py`). Fecha ROADMAP M72.

## [0.63.0] - 2026-07-10

### Added
- **Veredito medido do pilar vetorial P0 + proposta de reposicionamento do North Star** (`docs/benchmarks/vector-pillar-verdict-2026-07.md` (NEW), `docs/benchmarks/rabitq-spike/rabitq_ivf_mstg_1m768d.log` (NEW), `docs/adr/0033-north-star-reposition-proposal.md` (NEW, PROPOSED)): fechamento da investigação de superioridade vetorial. Gap 2 (QPS) atacado com o SOTA permissivo (RaBitQ vendorizado, ADR-0032) e **medido a 1M×768d** (spike D3): MSTG-RaBitQ-mem = 8.2ms @ 98.4% recall (competitivo com full-precision ~10-15ms, **NÃO os 25× do ScaNN**); variante disk = 98.4% @ **5.3 MB residentes** (o ganho real do RaBitQ é MEMÓRIA, não QPS). Conclusão honesta (Regra 3/5): **superioridade de QPS vetorial sobre AlloyDB/ScaNN NÃO é alcançável como extensão Postgres permissiva** (o 25× do ScaNN é do AH-LUT anisotrópico + não pagar o imposto PG). Alvos honestos: paridade classe-pgvector (Gap 1, fix do select_from) + RaBitQ como feature de **memória/billion-scale** + AI-native/HTAP. Proposta ADR-0033 (requer assinatura do owner) reposiciona o North Star. Prior-art R0: rabitq-rs/RaBitQ-Library/LanceDB/Qdrant (permissivos, estudo+vendor); VectorChord/srvdb (AGPL, só estudo de design).
- **Vendorizado o CORE do `rabitq-rs` (Apache-2.0) para o futuro índice IVF-RaBitQ** (`theodb_rs/src/rabitq/vendor/` (NEW): `quantizer.rs`, `rotation.rs`, `fastscan.rs`, `fastscan_kernel.rs`, `simd.rs`, `math.rs` + `LICENSE` + `VENDORED.md`; `docs/adr/0032-vendor-rabitq-rs-core.md` (NEW)): ataque ao Gap 2 do pilar vetorial (superioridade de QPS vs ScaNN/AlloyDB). RaBitQ (arXiv:2405.12497, quantização 1-bit training-free com bound de erro provado; canônica `VectorDB-NTU/RaBitQ-Library` Apache-2.0, adotada por Milvus/Faiss/Elasticsearch) é o lever **não-refutado** (M57 SBQ + M59 anisotrópico falharam no carrier HNSW; o carrier certo é IVF, que já temos em `ann/ivf.rs`). Vendorizado o core do algoritmo (commit upstream `10b9a4e`), NÃO a camada de storage (substituída pela nossa IVF page-native + WAL). Regra 9 (não reinventar) + D1 (Apache→Apache, LICENSE+atribuição preservados). Arquivos inertes até o wiring (implement); gate D3 (spike local de recall/velocidade) antes do AM completo. ADR-0032.

### Deprecated

### Removed

### Fixed

### Security

## [0.62.0] - 2026-07-10

### Added
- **P0 bloqueador-raiz — 2 achados decisivos que reformulam o gap de recall** (`docs/benchmarks/p0-vector-superiority-root-blocker.md`, `docs/benchmarks/m60-raw/m60_efc_{sweep_100k,seq_vs_parallel_500k}768d.json`, knob `THEODB_HNSW_EF_CONSTRUCTION` em `theodb_rs/src/am/build.rs`): experimento efc×modo-de-build em droplet — (1) o "gap" é **degradação por ESCALA**, não defeito fixo: theodb recall@10 = **0.998 a 100k×768d** (excelente, ≈/> pgvector) e só cai a 0.974 a 500k; (2) a hipótese do **overwrite paralelo é REFUTADA** (7º lever): sequential 0.974 ≈ parallel 0.972 a 500k — o build sequencial (sem overwrite) tem o MESMO plateau. A degradação é inerente ao algoritmo de build a escala, nos dois modos. Notícia de produto: para ≤100k vetores o vetor do theodb está em paridade/superioridade com pgvector. Knob `THEODB_HNSW_EF_CONSTRUCTION` (benchmark-only, default 64 — comportamento inalterado; espelha `THEODB_HNSW_PARALLEL_THRESHOLD`).
- **M71 (discover) — blueprint de latência iso-recall do scan** (`.claude/knowledge-base/discoveries/blueprints/m71-scan-latency-blueprint.md`): diagnóstico dual-source (theodb↔pgvector) + SOTA (PANORAMA arXiv:2510.00566, Faiss FastScan, KScaNN arXiv:2511.03298) do gap de latência a iso-recall (theodb precisa ~5× o `ef` do pgvector p/ o mesmo recall). Levers ranqueados: (1) qualidade de grafo (multi-entry build já +29% QPS medido), (2) kernel de distância com early-out por limiar (onde theodb pode SUPERAR pgvector), (3) SIMD multi-accumulator + hoist da norma da query no cosseno. Rigor iso-recall (não QPS-sweep). Implement+benchmark exigem droplet.

### Deprecated

### Removed

### Fixed

### Security

## [0.61.0] - 2026-07-10

### Added
- **M60 — medição decisiva do recall do HNSW próprio vs pgvector a 500k×768d** (`docs/benchmarks/m60-hnsw-recall.md`, `docs/benchmarks/m60-raw/`, `benchmarks/run_m60_recall.py` (NEW), `benchmarks/run_m60_pgvector_control.py` (NEW), blueprint `m60-hnsw-recall-quality`): head-to-head no MESMO corpus gaussian-mixture (droplet c-8, pg17) — pgvector best recall@10 = **0.988**, theodb_hnsw f32 = 0.974, theodb SBQ (over_fetch=32) = **0.986**. Dois achados (Regra 3): (1) **o gate 0.99 é artefato do dado** — o próprio pgvector só chega a 0.988 (256 clusters apertados em 768d → teto de recall@10 < 0.99 para índices HNSW); a DoD do M60 deve virar **paridade-pgvector**, não 0.99 absoluto; (2) existe um gap real **~1.4pt** (f32 vs pgvector), com o SBQ já em quase-paridade. Duas hipóteses de fix do discover (descida de build por beam ef=1; multi-entry `ep←W`) foram **implementadas e REFUTADAS por medição** a 500k×768d (no-op no recall) — revertidas; 5 levers refutados no total. Fechamento do M60 via reenquadramento de DoD → ver a entrada em `Changed` (ADR-0030). O grafo multi-entry rendeu +29% de QPS a recall igual (achado registrado para o M71).
- Roadmap v5 "Superioridade vetorial P0 (MEDIDA)" definido (`ROADMAP-v5.md` + seção `# Roadmap v5` em `ROADMAP.md`): fecha o pilar P0 do North Star (`docs/adr/0002`) que segue parcial — superioridade vetorial comprovada por benchmark. Milestones: **M60** (fundação — recall HNSW ≥0.99 a escala, já aberto), **M71** (latência-superior do AM, scan hot-path v2), **M72** (QPS a 1M+ multi-cliente), **M73** (head-to-head MEDIDO vs ScaNN/AlloyDB — o veredito de superioridade), **M74** (CONDICIONAL — quantização SOTA só com lever não-refutado por M57/M59). Measurement-first + honesto (Regra 3/5): cada milestone tem gate executável e ACEITA honest-negative como conclusão; o v5 NÃO promete vencer o ScaNN (~25× gap de QPS medido no M33; M57 SBQ + M59 anisotrópica+AH já honest-negative) — promete o veredito medido de onde o TheoDB está vs o SOTA.

### Deprecated

### Removed

### Fixed

### Security

## [0.60.0] - 2026-07-09
### Removed
- **M70 — pgvector e pgvectorscale REMOVIDOS totalmente** (`theodb_rs/src/dtype.rs`, `am/mod.rs`, `theodb_rs.control`, `theodb.control`, `sql/*.sql`, `Dockerfile`): o tipo `vector` do TheoDB agora é **100% own-code** — o pgvector e o pgvectorscale saíram da distribuição (Dockerfile sem o stage pgvectorscale, sem o `make install` do pgvector; **pg_duckdb intocado**). Fecha o roadmap v4 "Independência do pgvector" e o pilar do North Star.

## [0.59.0] - 2026-07-09
### Added
- **M69 — tipo vetorial PRÓPRIO own-code `theodb.vector`** (`theodb_rs/src/dtype.rs` (NEW), `lib.rs`, `docs/adr/0028`): tipo `vector` own-code no schema `theodb`, com layout `#[repr(C)]` **byte-idêntico** ao `Vector` do pgvector (`varlena u32 · dim u16 · unused u16 · f32[]`; 8+4·dim bytes) — coexiste com `public.vector` (pgvector) SEM colisão (schemas distintos). I/O text (parse espelha `vector.c`, PostgreSQL License) + **typmod** (parse + enforce via length-coercion cast) + **recv/send binário** (wire big-endian, `unused`==0) + operadores `<->`/`<#>`/`<=>` (reuso dos kernels `vec.rs`) + casts `real[]`/`float8[]`/`text` + **cast binário `WITHOUT FUNCTION` bidirecional com o `vector` do pgvector** (habilita coexistência + a migração grátis do M70). Fundação para remover o pgvector (M70 fará `SET SCHEMA public` ⇒ drop-in). **Validado pg17 real:** 16/16 dtype pg_tests GREEN (paridade `vector_type`/`cast`/`copy` binário + byte-compat dim-variado + typmod + negative-cases + memória sem UAF) + 13/13 HNSW AM GREEN (**não tocou o AM, zero regressão P0**). Código ORIGINAL (VectorChord AGPL só estudo). Sem claim de performance (correção/paridade). Spike ADR-D3 (7/7). ADR-0028.
- Roadmap amended: added M69 Tipo vetorial próprio own-code (coexistindo com pgvector, gated por paridade) + M70 Remover pgvector (e pgvectorscale) totalmente (`/roadmap-feature own-vector-type-drop-pgvector`) — Roadmap v4 "Independência do pgvector"; decisão da fonte de verdade: blueprint SHIPPABLE `.claude/knowledge-base/discoveries/blueprints/own-vector-type-drop-pgvector-blueprint.md` (veredito A, decomposto em 2 milestones).

### Deprecated

### Removed

### Fixed

### Security

## [0.58.0] - 2026-07-09
### Added
- **M68 — observabilidade do query vetorial (`theodb.explain_scan` + `candidates_seen`)** (`theodb_rs/src/ann/scan_core.rs`, `am/hnsw_page.rs`, `am/autotune.rs`, `api.rs`, `docs/ops/vector-scan-diagnostics.md` (NEW)): fecha o pilar de operabilidade do scan ANN (opaco por natureza). **`theodb.explain_scan(index_table, vector_col, query, ef, k)`** — função diagnóstica que retorna, de UM scan real: `index_name`, `ef_effective`, `pages_read`, `candidates_seen`, `latency_us`, `results` (padrão Qdrant `/telemetry`/Milvus — **não** `amexplain`, que não existe no PG17/18). **`candidates_seen`** — tamanho do pool navegado no beam, capturado own-code em `ground_search_nodes` (`visited.len()` antes do drop) e propagado ao thread_local `SCAN_CANDIDATES` (irmão do `SCAN_PAGES_READ` do M67); distingue "grafo caro de navegar" (candidates alto) de "I/O pesado / spill" (pages alto). `theodb.scan_stats` agora retorna 4-tupla (`pages_read, candidates_seen, latency_us, results`); catálogo heap `theodb._index_scan_stats` ganha `sum_candidates`; `theodb.index_scan_stats` expõe `avg_candidates` (pilar (c) do wiring-triad = catálogo consultável, crash-safe M35 — não histograma Prometheus, adiado por YAGNI). REVOKE FROM PUBLIC. **Doc de operação** `docs/ops/vector-scan-diagnostics.md`: playbook recall-baixo/latência-alta + tabela sinal→causa→ação. **pg_tests GREEN** (`explain_scan_shows_index_and_candidates`, `scan_stats_records_real_pages_read` estendido p/ 4-tupla + `sum_candidates>0`). Observabilidade → validado por teste determinístico, **sem benchmark de performance** (nenhum claim "Nx"). ADR-0027.

### Deprecated

### Removed

### Fixed

### Security

## [0.57.0] - 2026-07-09
### Added
- **M67 — auto-tune de índices vetoriais (`theodb.recommend_ef` + coletor de stats)** (`theodb_rs/src/am/autotune.rs` (NEW), `am/mod.rs`, `am/hnsw_page.rs`, `api.rs`, `benchmarks/run_m67_autotune.py` (NEW)): **recomendador determinístico** `theodb.recommend_ef(index, vec_col, samples, recall_target, k)` — bisecção monotônica sobre recall(ef) (monotônico, Malkov & Yashunin) contra GT exato amostrado (seqscan), retorna o menor ef que atinge o alvo (ctid como id estável; MAX_EF se inatingível). **Coletor** `theodb.scan_stats(tbl,col,query,ef,k)` — mede o **pages_read REAL** (thread_local que o traverse HNSW bumpa — 1 add in-memory, sem page write) + latência, persiste no catálogo heap `theodb._index_scan_stats` (FORA das páginas do índice — crash-safe, M35); `theodb.index_scan_stats(rel)` lê os agregados. REVOKE FROM PUBLIC. **5 pg_test GREEN** (stack real) + 12 pytest (MAE/RQUT/convergência). **Benchmark (10k sintético) — CONVERGED com nuance honesta:** o recomendador converge na média (recall 0.986 ≥ alvos), MAS (1) corpus fácil demais (baseline ef=64 dá recall 1.0; todos os alvos → ef=10 — não estressa a curva ef; SIFT1M mostraria o scaling), (2) RQUT 12% de cauda (mean-optimal, não tail-safe — v2). **NÃO auto-tune online** (deferido por evidência ADR-0026 — oscilação; SOTA é early-termination acadêmico DARTH/Ada-ef). **amcostestimate:** fórmula M48 (f(ef)) retida + auditabilidade via scan_stats; calibração-in-planning DEFERIDA por risco EC-3 (SPI no planning abortaria TODO o planejamento). `docs/benchmarks/m67-autotune.{md,json}`, ADR-0026.

### Deprecated

### Removed

### Fixed

### Security

## [0.56.0]