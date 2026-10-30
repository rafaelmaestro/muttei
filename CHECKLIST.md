# Muttei: checklist

Legenda: `[x]` feito, `[ ]` pendente, `[-]` fora do escopo. Notas de verificação ficam na frente de cada item.

## 0. Pré-requisitos
- [x] Headers de dev instalados (`pkg-config --exists webkit2gtk-4.1`) · ok
- [x] `tauri-cli` v2 instalado · v2.12.4
- [x] Repo git ligado ao `github.com/rafaelmaestro/muttei`

## 1. Protótipo (Artifact)
https://claude.ai/artifact/CaqrnFcSV88Hp2UaVBwaWB (v1, 30/10)
- [x] Desktop GNOME simulado com tray e menu
- [x] Ctrl+Shift+M: pílula, tray vermelha, ponto fixo, som
- [x] Ajustes > Atalho (captura de tecla real)
- [x] Ajustes > Regras (simular lock e abertura de app)
- [x] Ajustes > Celular (QR, código, interface, dispositivos)
- [x] Ajustes > Meet (status, "Conectar outro navegador" 2 min)
- [x] Ajustes > Geral (som, ponto, canto, autostart)
- [x] Celular simulado com pareamento e botão de mic
- [x] Aba do Meet simulada sincronizando o ícone
- [x] Layout aprovado pelo usuário · app segue o protótipo; `.note` virou block (no protótipo quebrava texto com mono)

## 2. Scaffold + settings
- [x] Projeto Tauri 2 (`src-tauri/`, `ui/`) · sem bundler, `withGlobalTauri`
- [x] `.gitignore`
- [x] `settings.rs` (JSON em `~/.config/muttei/settings.json`, atalhos como mapa ação → tecla) · 2 testes
- [x] Instância única · rodar de novo abre os Ajustes

## 3. Núcleo
- [x] `audio.rs`: set-mute/get-volume via `wpctl`
- [x] Teste: parser do `wpctl` com e sem `[MUTED]`
- [x] Polling de 500 ms para sync externo · mute externo via wpctl refletiu
- [x] Atalho global Ctrl+Shift+M configurável · testado com Ctrl+Alt+J
- [x] Desregistrar o atalho durante a captura; erro de registro aparece na UI · Ctrl+Space recusado com aviso do IBus
- [x] Pílula "No ar" / "Mudo" (~1,5 s, sem foco) · abre no monitor do cursor; janela fica com 200px (mínimo do WebKitGTK), transparente; click-through **não verificado**
- [x] Tray: ícone vermelho quando mudo; menu Mutar/Ajustes/Sair · ícone sólido; mudo = disco vermelho (corte some a 20px); menu precisa de teste manual
- [x] Ponto fixo no canto configurável, com opção de desligar · posicionado depois do load (outer_size só vale com a janela realizada)
- [x] Fontes locais em `ui/fonts/` (OFL), servidas também para o celular; sem dependência de internet no login
- [x] Um lock só em volta de wpctl + estado, para o polling não sobrescrever um toggle
- [x] Som via `pw-play`, com opção de desligar · device-removed/added.oga
- [x] Autostart · cria ~/.config/autostart/Muttei.desktop apontando para o binário que rodou
- [x] Intro na primeira execução

## 4. Regras
- [ ] Spike: confirmar `ActiveChanged (true,)` com Super+L · **manual**
- [x] Travar a tela muta (nunca desmuta) · implementado; falta validar com Super+L
- [x] Abrir um app muta (`/proc/*/exe` + argv0, só na transição) · testado com binário probe
- [x] Teste: transição ausente → presente

## 5. Celular
- [x] Filtro de interfaces (`lo`, `docker*`, `br-*`, `veth*`, `tun*`, `wg*`, `virbr*`)
- [x] Teste: filtro de interfaces
- [x] Seletor manual de interface + porta fixa persistida
- [x] Servidor só com o toggle ligado e bind só no IP da LAN · ss: 192.168.0.157:47821
- [x] QR code
- [x] Pareamento: código de 4 dígitos → token; regenera após 5 erros · corrigido deadlock no mutex do pareamento
- [x] Token validado em toda mensagem; lista de dispositivos com revogar · token forjado → revoked; revogar empurra revoked na hora
- [x] Teste: pareamento
- [x] Bloqueio de 60 s após 5 erros (sem isso dava para força bruta na LAN) · teste + WS simulado
- [x] Servidor do celular refaz o bind quando a rede ou o IP mudam (checa a cada 5 s) (código errado, regeneração, token inválido)
- [x] `phone.html`: botão de mic, estado ao vivo, vibração · testado via WS simulado; falta celular real
- [x] Aviso de `ufw allow` quando o ufw estiver ativo

## 6. Meet
- [x] Servidor WS só em 127.0.0.1, Origin `chrome-extension://` · origem web recusada
- [x] Aprovação da primeira extensão + janela de 2 min para outra · testado com clientes WS simulados
- [x] `background.js`: WS com ping de 20 s
- [x] `content.js`: botão de mic por `data-is-muted` + aria-label pt/en · falta testar no Meet real (Chrome 151 ignora --load-extension)
- [x] Sync app → Meet sem loop · aplica uma vez por estado e por botão

## 7. Verificação end-to-end
- [ ] Click-through: clicar no `×` de uma janela maximizada no notebook (canto do ponto) e numa aba sob a área da pílula
- [ ] Menu da tray: Mutar/Desmutar, Ajustes, Sair
- [x] Ctrl+Shift+M → `[MUTED]` + pílula + tray + ponto + som; reverte
- [x] Mute pelo GNOME reflete em ~500 ms · testado via wpctl
- [ ] Super+L muta
- [x] `gnome-calculator` na regra muta ao abrir · testado com binário probe no lugar
- [ ] Celular pareia e muta, com a VPN conectada
- [ ] Extensão no Chrome em `meet.google.com/new` acompanha
- [ ] Autostart sobe na tray após reiniciar a sessão

## Futuro (mapeado, fora da v1)
- [-] Push-to-talk (Ctrl+Space): press/release, restaurar estado anterior, debounce do autorepeat do X11, conflito com IBus
- [-] Push-to-cough (Ctrl+Shift+Space): mesmo mecanismo invertido
- [-] Câmera on/off (Ctrl+Shift+O): clique no `content.js` + botão no celular
- [-] Sair da chamada (Ctrl+Shift+H): clique no `content.js` + botão no celular
- [-] Levantar a mão (Ctrl+Shift+K): só no Teams
- [-] Integração com Teams: API de terceiros (WebSocket local)
- [-] Trial / licença / pagamento
