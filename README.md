# Peripheral Monitor

Aplicativo de bandeja para Windows (Tauri 2 + React + Rust) que avisa por notificação quando um periférico USB ou Bluetooth é conectado ou desconectado, e acompanha a bateria do mouse AJAZZ AJ179P MC.

## Funcionalidades
- Detecta qualquer dispositivo USB pela SetupAPI do Windows, acordado por `CM_Register_Notification` (com varredura de segurança a cada 30 s).
- Um periférico aparece uma vez só, mesmo expondo várias interfaces HID (interfaces `&MI_xx` e root hubs são ignorados). Bluetooth HID vem do `hidapi`.
- Notificações enviadas pelo Rust: funcionam com a janela fechada. Fechar a janela esconde o app na bandeja; "Sair" no menu do ícone encerra.
- Inicia com o Windows (minimizado na bandeja) e roda em instância única. Preferências salvas em `settings.json` na pasta de configuração do app.
- Bateria do AJAZZ AJ179P MC (receptor 249A:5C2F): consulta GET `0x10` na interface vendor MI_02 a cada 2 min e quando o mouse/cabo conecta; alerta em 20 % e 10 %. Protocolo documentado pelo projeto [Ajazzy](https://github.com/Geovane2dd/Ajazzy) e confirmado neste modelo (dev_id `179P`).

## Requisitos
Windows 10/11, Node.js LTS, Rust stable (MSVC), Visual Studio Build Tools com C++ e Windows SDK, WebView2.

## Rodar
```powershell
npm install
npm run tauri dev
```

## Gerar instalador
```powershell
npm run tauri build
```

## Testes
```powershell
cd src-tauri
cargo test                              # testes de unidade
cargo test -- --ignored --nocapture     # lista os dispositivos e lê a bateria desta máquina
```

## Limitações conhecidas
- Carregando (suporte de pinos ou cabo) vem do aviso `C0 01 <bateria> <carregando>` que o mouse envia na hora e do byte 12 da resposta `0x10`. Durante a carga o mouse informa 100 % fixo, então o app mostra a última leitura real até ele sair do carregador.
- Com o mouse dormindo o receptor pode não responder; a última leitura é mantida enquanto o receptor estiver conectado.
- Dispositivos virtuais (ex.: controle Xbox emulado por drivers como ViGEm) podem aparecer como uma entrada extra.

## Contribuindo
Contribuições entram somente por pull request: a `main` é protegida, a CI precisa passar e o mantenedor revisa cada PR. Veja o [CONTRIBUTING.md](CONTRIBUTING.md).

## Licença
[GPL-3.0](LICENSE). O protocolo do mouse AJAZZ foi documentado pelo projeto [Ajazzy](https://github.com/Geovane2dd/Ajazzy), também GPL-3.0.
