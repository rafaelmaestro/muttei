<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand/logo/horizontal-dark.svg">
  <img src="brand/logo/horizontal.svg" alt="Muttei" height="64">
</picture>

Seu microfone em um atalho. Muta o microfone de qualquer app no Linux (GNOME/X11 + PipeWire) e mostra o estado na tela.

- **Atalho global:** Ctrl+Shift+M muta e desmuta. Também dá para configurar segurar para falar, segurar para tossir e atalhos de câmera, mão e sair do Meet.
- **Indicadores:** pílula "No ar"/"Mudo" em todas as telas, ícone na tray e ponto vermelho fixo enquanto estiver mudo.
- **Regras:** muta ao travar a tela e quando um app da lista abre.
- **Celular:** controle pela Wi-Fi, sem instalar nada. O celular abre uma página, pareia por QR e código de 4 dígitos.
- **Google Meet:** a extensão em `extension/` faz o ícone de mic acompanhar o estado e executa câmera, mão e sair.

## Rodar

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential pkg-config
cargo install tauri-cli --version '^2'
cd src-tauri && cargo tauri dev
```

Para a extensão, abra `chrome://extensions`, ligue o modo desenvolvedor e use "Carregar sem compactação" com a pasta `extension/`.

## Marca

O logo, as variações e o guia de uso ficam em [`brand/`](brand/GUIDELINES.md).
