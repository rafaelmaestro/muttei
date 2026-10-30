<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand/logo/horizontal-dark.svg">
  <img src="brand/logo/horizontal.svg" alt="Muttei" height="64">
</picture>

Seu microfone em um atalho. Muta o microfone de qualquer app no Linux (GNOME, X11 ou Wayland, com PipeWire) e mostra o estado na tela.

- **Atalho global:** Ctrl+Shift+M muta e desmuta. Também dá para configurar segurar para falar, segurar para tossir e atalhos de câmera, mão e sair do Meet.
- **Indicadores:** pílula "No ar"/"Mudo" em todas as telas, ícone na tray e um indicador fixo no canto enquanto estiver mudo (chave, selo, etiqueta ou halo).
- **Regras:** muta ao travar a tela e quando um app da lista abre.
- **Celular:** controle pela Wi-Fi, sem instalar nada. O celular abre uma página, pareia por QR e código de 4 dígitos.
- **Google Meet:** a extensão em `extension/` faz o ícone de mic acompanhar o estado e executa câmera, mão e sair.

## Wayland

No Wayland o app roda pelo XWayland, que mantém a pílula e o indicador no lugar. Os atalhos dependem da sessão:

| Sessão | Atalhos | Segurar para falar/tossir |
|---|---|---|
| X11 | grab do X11 | sim |
| Wayland, GNOME 48+ ou KDE | portal `GlobalShortcuts` (o sistema pede aprovação) | sim |
| Wayland, GNOME 46/47 | atalhos personalizados do GNOME que chamam `muttei --action <ação>` | não: o GNOME não informa quando a tecla é solta |

Para forçar um backend, use `MUTTEI_SHORTCUTS=x11|portal|gnome`.

## Rodar

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential pkg-config
cargo install tauri-cli --version '^2'
cd src-tauri && cargo tauri dev
```

Para a extensão, abra `chrome://extensions`, ligue o modo desenvolvedor e use "Carregar sem compactação" com a pasta `extension/`.

## Marca

O logo, as variações e o guia de uso ficam em [`brand/`](brand/GUIDELINES.md).
