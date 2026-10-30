# Muttei: guia de uso do logo

## 1. O logo
- **Ideia:** a cápsula do microfone é o trilho de um interruptor, e a bolinha vermelha é a chave. O vermelho da chave e do "ei" é o mesmo sinal de "mudo" que o app usa na tray e no ponto fixo.
- **Versões:** horizontal (principal), empilhada, só o símbolo e só o nome.
- **Arquivos:**
  - `logo/*.svg` são as mestras vetoriais.
  - `png/` tem os rasters prontos.
  - `web/` tem o favicon, os ícones PWA e o manifest.
  - `presentation/` tem a prancha com mockups.
  - Todos os arquivos saem de `build.py`. Para gerar de novo, rode `python3 brand/build.py`; só precisa da stdlib.

| Arquivo | Uso |
|---|---|
| `horizontal.svg` / `horizontal-dark.svg` | README, site, cabeçalhos (fundo claro / escuro) |
| `stacked.svg` / `stacked-dark.svg` | Espaços quadrados, splash, adesivo |
| `symbol.svg` / `symbol-dark.svg` | Avatar, ícones a partir de 48 px |
| `symbol-small.svg` / `symbol-small-dark.svg` | 16–32 px: traços mais grossos e chave maior |
| `*-black.svg` / `*-white.svg` | Uma cor: a chave vira furo (gravação, carimbo, bordado) |
| `wordmark.svg` / `wordmark-dark.svg` | Só o nome, quando o símbolo já aparece por perto |
| `app-icon.svg` / `app-icon-maskable.svg` | Ícone do app (Tauri) e PWA maskable |
| `tray.svg` / `tray-muted.svg` | Ícones da barra do GNOME: no ar / mudo |

## 2. Área de proteção
Mantenha livre, em volta do logo, **1 chave** de distância. A chave é o diâmetro da bolinha vermelha, cerca de 17% da altura do símbolo. A distância cresce e diminui junto com o logo.

## 3. Tamanho mínimo
| Versão | Tela | Impressão |
|---|---|---|
| Horizontal | 96 px de largura | 25 mm |
| Símbolo | 16 px (use `symbol-small`) | 6 mm |

Abaixo de 32 px, use sempre o `symbol-small`.

## 4. Cor
| Nome | HEX | RGB | CMYK (aprox.) | Papel |
|---|---|---|---|---|
| Vermelho mudo | `#ff5555` | 255 85 85 | 0 67 67 0 | Chave e "ei". Só para eles e para o estado "mudo" |
| Tinta | `#282a36` | 40 42 54 | 26 22 0 79 | Logo em fundo claro; fundo do ícone |
| Papel | `#f8f8f2` | 248 248 242 | 0 0 2 3 | Logo em fundo escuro |

**Fundos aprovados:**
- colorido sobre branco ou sobre fundo claro;
- versão `-dark` sobre `#282a36` ou mais escuro;
- preto sobre branco;
- branco sobre foto escura, numa área calma.

Os valores CMYK são conversões aproximadas. Antes de imprimir, faça uma prova na gráfica. O Pantone ainda não foi definido.

Na versão `-dark`, o traço é 5% mais fino (19 em vez de 20) e a chave é um pouco maior. Isso compensa o efeito de que formas claras sobre fundo escuro parecem mais pesadas.

## 5. Tipografia
- **Nome no logo:** JetBrains Mono Bold, convertida em contornos dentro das SVGs. Não redigite o nome.
- **Interface:** IBM Plex Sans (texto) e JetBrains Mono (atalhos e dados).
- **Licenças:** as duas usam a SIL Open Font License, que permite uso em logo. Os arquivos estão em `ui/fonts/`.

## 6. Não faça
- Não estique, não achate e não gire o logo.
- Não pinte a chave de outra cor. O verde ou o roxo confundem o sinal de "mudo".
- Não pinte o "mutt" de vermelho.
- Não adicione sombra, contorno, gradiente ou brilho.
- Não mude o tamanho nem a posição das partes de um lockup.
- Não use a versão colorida sobre fundo vermelho ou movimentado. Nesses casos use a versão de uma cor.
- Não volte ao mic cortado por uma diagonal: esse é o ícone genérico do sistema.

## 7. Pendências
- **Marca registrada:** nenhuma busca foi feita. Faça uma busca profissional antes de publicar.
- **Pantone:** a definir, com prova impressa.
