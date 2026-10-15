# Como contribuir

Obrigado pelo interesse no Peripheral Monitor! Contribuições são bem-vindas.

## Fluxo

A branch `main` é protegida: ninguém envia commits direto para ela. Toda mudança entra por pull request.

1. Faça um fork do repositório e crie uma branch a partir da `main` (`git checkout -b minha-mudanca`).
2. Faça as alterações e rode as verificações abaixo.
3. Abra um pull request para a `main` preenchendo o modelo.
4. A CI precisa passar e o mantenedor ([@JoseDaniloS](https://github.com/JoseDaniloS)) revisa e aprova antes do merge.

Para PRs de contas novas, o GitHub só roda a CI depois que o mantenedor libera; isso é intencional.

## Antes de abrir o PR

```powershell
npm ci
npm run build
cd src-tauri
cargo test --lib
cargo clippy --lib -- -D warnings
```

Os testes marcados com `#[ignore]` falam com o hardware de verdade (mouse AJAZZ). Só rode se tiver o dispositivo, e descreva no PR o que testou.

## O que é revisado

- O código compila e os testes passam (a CI confere).
- A mudança faz o que o PR descreve, e nada além disso.
- **Segurança:** nada de downloads, execução de programas, rede, telemetria ou acesso a arquivos fora do necessário. Comandos enviados a dispositivos USB/HID precisam ser documentados (origem do protocolo) e, de preferência, só leitura.
- Novas dependências precisam de justificativa.
- Gravações no mouse ou teclado precisam ser conferidas por releitura e reversíveis.

PRs que alterem a CI (`.github/`) ou o processo de build recebem revisão extra.

## Licença

Ao contribuir, você concorda que sua contribuição é distribuída sob a [GPL-3.0](LICENSE), a mesma licença do projeto.
