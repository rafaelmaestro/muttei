# Muttei na Chrome Web Store

Este arquivo reúne os textos e as respostas para colar no painel de desenvolvedor da Chrome Web Store.

## Pacote
- Gere o zip com `./scripts/package-extension.sh`, que cria `dist/muttei-extension-<versão>.zip`.
- A cada envio, suba `version` em `manifest.json`. A loja recusa uma versão repetida.

## Página da extensão
- **Nome:** Muttei
- **Resumo** (até 132 caracteres): Faz o microfone do Google Meet acompanhar o Muttei e aceita câmera, mão e sair pelo atalho ou pelo celular.
- **Categoria:** Ferramentas
- **Idioma:** Português (Brasil)
- **Descrição:**

  > O Muttei é um app de mudo global para Linux e Windows. Um atalho, Ctrl+Shift+M por padrão, fecha e abre o microfone do sistema em qualquer janela.
  >
  > Esta extensão liga o Google Meet ao Muttei:
  > • o ícone de microfone do Meet acompanha o estado do Muttei;
  > • câmera, levantar a mão e sair da chamada funcionam pelo atalho do Muttei ou pelo celular;
  > • a regra "Mutar ao entrar numa chamada" sabe quando você entrou.
  >
  > Precisa do app Muttei instalado e aberto no computador. A extensão só conversa com ele, em 127.0.0.1, e não envia nada para a internet.

- **Ícone:** `icon128.png`
- **Capturas** (1280×800): `store/screenshot-1.png`, `store/screenshot-2.png`
- **Tile promocional pequeno** (440×280): `store/promo-440x280.png`

## Privacidade (aba "Práticas de privacidade")
- **Finalidade única:** sincronizar os controles de microfone, câmera, mão e sair do Google Meet com o app Muttei, que roda no próprio computador.
- **Justificativa do acesso a `https://meet.google.com/*`:** o script de conteúdo encontra os botões de microfone, câmera, mão e sair na página do Meet para clicar neles e ler o estado de cada um.
- **Código remoto:** não. Todo o código está no pacote.
- **Dados coletados:** nenhum. Marque "Não coleto nem uso dados do usuário".
- **Certificações:**
  - não vende nem transfere dados a terceiros;
  - não usa dados para fins não relacionados à finalidade única;
  - não usa dados para crédito ou empréstimo.
- **Política de privacidade:** exigida só quando há coleta de dados. Se o painel pedir, use o link da seção "Privacidade" do README.

## Depois de publicada
- A loja dá um ID novo para a extensão, diferente do ID da versão carregada sem compactação. O Muttei aceita qualquer extensão de navegador: a primeira é aprovada sozinha, e as outras passam por Ajustes > Meet > Conectar outro navegador.
- Troque o link "Chrome Web Store" no README pelo endereço da página publicada.
