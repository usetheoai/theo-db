# Escrevendo workflows neste repositório

Regras que valem para todo arquivo em `.github/workflows/`. Quem revisa um workflow lê isto;
quem escreve um, também. Fora daqui elas viram folclore, e folclore não sobrevive a uma
rotatividade de autor.

## 1. Toda `uses:` de terceiro é fixada por SHA de 40 caracteres

```yaml
- uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
```

O SHA é o contrato; o comentário com a tag é para o humano. Nunca só a tag.

**Por quê.** Uma tag é um ponteiro mutável. Quem controla o repositório da action pode
reapontá-la, e o consumidor passa a executar código novo **sem um diff em lugar nenhum** —
nem no PR, nem no histórico, nem na revisão. Um SHA é imutável por construção: o código que
rodou ontem é o código que roda hoje.

**`actions/*` e `docker/*` entram na regra.** Pertencer ao GitHub ou à Docker não torna uma
tag imutável — torna o mantenedor mais confiável, que é uma propriedade diferente. A regra é
sobre o mecanismo, não sobre a reputação.

**Quem mantém os SHAs atualizados.** O `dependabot.yml` deste diretório, semanalmente: ele
reescreve SHA **e** comentário de tag juntos. Fixar por SHA sem alguém avisando de versão
nova troca um risco por outro — a action envelhece até virar a vulnerabilidade que não foi
aplicada.

**Ao adicionar uma action nova**, resolva o SHA em vez de copiá-lo de um exemplo:

```bash
gh api repos/<owner>/<repo>/git/ref/tags/<tag> --jq '.object.sha'
# se `.object.type` for "tag" (tag anotada), desreferencie:
gh api repos/<owner>/<repo>/git/tags/<sha> --jq '.object.sha'
```

**Imagens de container** seguem o mesmo princípio por digest — `aquasec/trivy@sha256:…` no
`publish-image.yml` é o precedente, e a razão está escrita lá: consumir a ferramenta pela
imagem oficial fixada evita acrescentar mais uma referência de action à superfície.

## 2. Nenhum job invoca caminho que não existe no disco

`scripts/check-workflow-paths.sh` reprova. Um job que instala requisito de uma árvore
inexistente falha tarde, no runner, depois de já ter gasto minutos.

## 3. A fronteira produto/avaliador não se cruza

Este repositório é o **banco**. O `theodb-bench` é o **avaliador**, e é um projeto
independente que o theo-db consome. A esteira daqui não roda suítes de medição do avaliador,
e nenhuma constante de versão do avaliador é fixada aqui.

## 4. Nada de constante de patch upstream fixada à mão

`18.4` escrito em três lugares foi uma manutenção que ninguém lembra de fazer. Derive da
imagem ou do `pg_config`.
