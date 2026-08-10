# benchmarks/artifacts

Saída bruta das medições: os JSON, JSONL, CSV, logs e flamegraphs que os runners deste
diretório escrevem. É **dado**, não conhecimento.

Esta separação é o motivo de o diretório existir. A documentação do projeto vive em
[`wiki/`](../../wiki/index.md) como um acervo em Open Knowledge Format, onde cada arquivo é
um conceito com frontmatter tipado. Despejar um dump de 25 MB de latências por query nesse
acervo o pioraria para o leitor sem melhorar em nada a rastreabilidade do número: o veredito
que interpreta a medição é um conceito, o arquivo que a registra é um artefato, e são coisas
diferentes.

## Quem escreve aqui

Os runners sob `benchmarks/` e alguns scripts sob `scripts/` — via `--out`, `--out-json` ou
a variável de ambiente `OUT`. Os defaults já apontam para cá; passar um caminho explícito
continua funcionando.

## Relação com os vereditos

Cada medição publicada tem um conceito correspondente em
[`wiki/benchmarks/`](../../wiki/benchmarks/index.md), que carrega o método, os números e os
limites declarados. **Nenhuma afirmação de performance do projeto vale sem esse conceito** —
o artefato bruto sozinho não é o veredito.

## Histórico

Até 2026-08-07 estes artefatos viviam sob `docs/benchmarks/`, junto dos documentos que os
citavam. Quando `docs/` foi convertida em `wiki/` e removida do repositório, os artefatos
que existiam até ali **não** foram movidos para cá — eles permanecem apenas no histórico
git, recuperáveis com:

```bash
git show f7c7b93:docs/benchmarks/<arquivo>
```

Ou seja: este diretório começa vazio e é preenchido pelas medições daqui para frente.
