//! Botões programáveis do mouse AJAZZ (AJ179P MC e família).
//!
//! Protocolo documentado pelo projeto Ajazzy (docs/reverse-engineering.md):
//!
//! - Consulta GET 0x11 devolve a tabela de botões: 9 slots de 3 bytes
//!   `[marcador, b1, b2]`, um por botão (0 esquerdo, 1 direito, 2 meio,
//!   3 lateral de trás, 4 lateral da frente, 5 DPI).
//! - SET família 0x09, opcode 0x0F regrava a tabela inteira.
//! - Marcadores: 0x10 padrão (`b1 = 1 << slot`), 0x70 tecla do teclado
//!   (`b1` = modificadores, `b2` = código HID), 0x80 tecla de mídia (uso
//!   Consumer de 16 bits), 0x30 disparo rápido, 0x90 macro (`23 01`).
//! - Macro: família 0x08, conteúdo `[0x03, passos de 4 bytes…]` dividido em
//!   blocos de 27 bytes; enviado antes de marcar o botão com 0x90. O mouse
//!   guarda um único conteúdo de macro.
//!
//! Toda gravação é conferida relendo a tabela: pelo receptor 2.4G algumas
//! gravações são confirmadas pelo mouse mas não ficam salvas.

use crate::ajazz::{Link, REPORT_LEN};
use serde::{Deserialize, Serialize};
use std::{sync::Mutex, thread, time::Duration};

const QUERY_KEY_TABLE: u8 = 0x11;
const FAMILY_KEY_TABLE: u8 = 0x09;
const OPCODE_KEY_TABLE: u8 = 0x0F;
const FAMILY_MACRO: u8 = 0x08;
const PAYLOAD_LEN: usize = 27;
const MAX_MACRO_CHUNKS: usize = 15;
const TIMEOUT: Duration = Duration::from_millis(1500);

/// Botões que o app deixa remapear. Esquerdo, direito e DPI ficam fixos para
/// o mouse nunca ficar inutilizável.
pub const EDITABLE_SLOTS: [usize; 3] = [2, 3, 4];

/// Uma gravação por vez no mouse.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ButtonAction {
    /// Função de fábrica do botão.
    Default,
    /// Tecla do teclado com modificadores (bit0 Ctrl, bit1 Shift, bit2 Alt, bit3 Win).
    Keyboard { modifiers: u8, key: u8 },
    /// Uso da página HID Consumer (volume, play/pause, navegador…).
    Media { usage: u16 },
    /// Clique repetido: intervalo e quantidade.
    RapidFire { interval: u8, count: u8 },
    /// Executa a macro gravada no mouse.
    Macro,
    /// Valor que o app não reconhece; preservado como está.
    Unknown { raw: [u8; 3] },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MouseButton {
    pub slot: usize,
    pub action: ButtonAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MacroKind { KeyDown, KeyUp, MouseDown, MouseUp }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroStep {
    pub kind: MacroKind,
    /// Código HID da tecla, ou bit do botão do mouse (1 esquerdo, 2 direito, 4 meio).
    pub value: u8,
    /// Pausa depois deste passo, em ms.
    pub delay_ms: u16,
}

fn checksum(report: &mut [u8; REPORT_LEN]) {
    report[REPORT_LEN - 1] = report[4..REPORT_LEN - 1].iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
}

fn decode(slot: usize, raw: [u8; 3]) -> ButtonAction {
    match raw {
        [0x10, b, 0] if b == 1 << slot => ButtonAction::Default,
        [0x70, modifiers, key] => ButtonAction::Keyboard { modifiers, key },
        [0x80, lo, hi] => ButtonAction::Media { usage: u16::from_le_bytes([lo, hi]) },
        [0x30, interval, count] => ButtonAction::RapidFire { interval, count },
        [0x90, ..] => ButtonAction::Macro,
        _ => ButtonAction::Unknown { raw },
    }
}

fn encode(slot: usize, action: &ButtonAction) -> [u8; 3] {
    match *action {
        ButtonAction::Default => [0x10, 1 << slot, 0],
        ButtonAction::Keyboard { modifiers, key } => [0x70, modifiers, key],
        ButtonAction::Media { usage } => { let [lo, hi] = usage.to_le_bytes(); [0x80, lo, hi] }
        ButtonAction::RapidFire { interval, count } => [0x30, interval, count],
        ButtonAction::Macro => [0x90, 0x23, 0x01],
        ButtonAction::Unknown { raw } => raw,
    }
}

fn slot_bytes(table: &[u8; REPORT_LEN], slot: usize) -> [u8; 3] {
    let off = 4 + slot * 3;
    [table[off], table[off + 1], table[off + 2]]
}

/// Transforma a resposta da consulta 0x11 no comando SET com `changes` aplicadas.
fn build_table_write(reply: &[u8; REPORT_LEN], changes: &[(usize, [u8; 3])]) -> [u8; REPORT_LEN] {
    let mut report = *reply;
    report[0] = FAMILY_KEY_TABLE;
    report[1] = 0x00;
    report[2] = 0x01;
    report[3] = OPCODE_KEY_TABLE;
    for &(slot, bytes) in changes {
        let off = 4 + slot * 3;
        report[off..off + 3].copy_from_slice(&bytes);
    }
    checksum(&mut report);
    report
}

/// Relatórios da família 0x08 com o conteúdo da macro, em blocos de 27 bytes.
fn build_macro_reports(steps: &[MacroStep]) -> Result<Vec<[u8; REPORT_LEN]>, String> {
    let mut stream = vec![0x03u8];
    for step in steps {
        let kind = match step.kind {
            MacroKind::MouseDown => 0x10,
            MacroKind::MouseUp => 0x90,
            MacroKind::KeyDown => 0x30,
            MacroKind::KeyUp => 0xB0,
        };
        let [lo, hi] = step.delay_ms.to_le_bytes();
        stream.extend_from_slice(&[kind, lo, hi, step.value]);
    }
    let total = stream.len().div_ceil(PAYLOAD_LEN);
    if total > MAX_MACRO_CHUNKS {
        return Err(format!("Macro longa demais: no máximo {} passos.", (MAX_MACRO_CHUNKS * PAYLOAD_LEN - 1) / 4));
    }
    Ok(stream
        .chunks(PAYLOAD_LEN)
        .enumerate()
        .map(|(index, chunk)| {
            let mut report = [0u8; REPORT_LEN];
            report[0] = FAMILY_MACRO;
            report[2] = if total == 1 { 0x01 } else { ((index as u8) << 4) | total as u8 };
            report[3] = chunk.len() as u8;
            report[4..4 + chunk.len()].copy_from_slice(chunk);
            checksum(&mut report);
            report
        })
        .collect())
}

fn open() -> Result<Link, String> {
    Link::open().ok_or_else(|| "Mouse AJAZZ não encontrado. Confira se o receptor está conectado.".into())
}

fn read_table(link: &Link) -> Result<[u8; REPORT_LEN], String> {
    let mut query = [0u8; REPORT_LEN];
    query[0] = QUERY_KEY_TABLE;
    link.transact(&query, TIMEOUT)?.ok_or_else(|| "O mouse não respondeu. Mexa nele para acordá-lo e tente de novo.".into())
}

fn buttons_from(table: &[u8; REPORT_LEN]) -> Vec<MouseButton> {
    EDITABLE_SLOTS.iter().map(|&slot| MouseButton { slot, action: decode(slot, slot_bytes(table, slot)) }).collect()
}

/// Lê a configuração atual dos botões editáveis.
pub fn read_buttons() -> Result<Vec<MouseButton>, String> {
    let _guard = WRITE_LOCK.lock().map_err(|_| "Mouse ocupado")?;
    let link = open()?;
    Ok(buttons_from(&read_table(&link)?))
}

/// Grava a ação de um botão (e a macro, se for o caso) e confere relendo a tabela.
pub fn write_button(slot: usize, action: &ButtonAction, macro_steps: Option<&[MacroStep]>) -> Result<Vec<MouseButton>, String> {
    if !EDITABLE_SLOTS.contains(&slot) { return Err("Esse botão não pode ser alterado.".into()); }
    let _guard = WRITE_LOCK.lock().map_err(|_| "Mouse ocupado")?;
    let link = open()?;
    let table = read_table(&link)?;

    let mut changes = vec![(slot, encode(slot, action))];
    if *action == ButtonAction::Macro {
        let steps = macro_steps.filter(|s| !s.is_empty()).ok_or("Grave pelo menos uma tecla na macro.")?;
        // O eco de cada bloco só serve de ritmo; a confirmação vem da releitura.
        for report in build_macro_reports(steps)? { link.transact(&report, TIMEOUT)?; }
        // O mouse guarda uma macro só: outro botão com macro passaria a tocar esta.
        for &other in EDITABLE_SLOTS.iter().filter(|&&s| s != slot) {
            if decode(other, slot_bytes(&table, other)) == ButtonAction::Macro {
                changes.push((other, encode(other, &ButtonAction::Default)));
            }
        }
    }
    write_and_verify(&link, &table, &changes)
}

/// Volta os botões editáveis para a função de fábrica.
pub fn reset_buttons() -> Result<Vec<MouseButton>, String> {
    let _guard = WRITE_LOCK.lock().map_err(|_| "Mouse ocupado")?;
    let link = open()?;
    let table = read_table(&link)?;
    let changes: Vec<_> = EDITABLE_SLOTS.iter().map(|&s| (s, encode(s, &ButtonAction::Default))).collect();
    write_and_verify(&link, &table, &changes)
}

fn write_and_verify(link: &Link, table: &[u8; REPORT_LEN], changes: &[(usize, [u8; 3])]) -> Result<Vec<MouseButton>, String> {
    link.transact(&build_table_write(table, changes), TIMEOUT)?;
    // O firmware pode demorar um instante para refletir a mudança na leitura.
    for attempt in 0..3 {
        thread::sleep(Duration::from_millis(300 + attempt * 400));
        let after = read_table(link)?;
        if changes.iter().all(|&(slot, bytes)| slot_bytes(&after, slot) == bytes) {
            return Ok(buttons_from(&after));
        }
    }
    Err("O mouse recebeu o comando, mas não salvou. Tente de novo com o mouse acordado ou conectado pelo cabo.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> [u8; REPORT_LEN] {
        let v: Vec<u8> = s.split_whitespace().map(|h| u8::from_str_radix(h, 16).unwrap()).collect();
        v.try_into().unwrap()
    }

    /// Tabela de fábrica lida do AJ179P MC.
    const FACTORY: &str = "11 00 01 12 10 01 00 10 02 00 10 04 00 10 08 00 10 10 00 40 01 00 00 00 00 00 00 00 00 00 00 B0";

    #[test]
    fn decodes_factory_table() {
        let table = hex(FACTORY);
        assert!(buttons_from(&table).iter().all(|b| b.action == ButtonAction::Default));
        assert_eq!(decode(5, slot_bytes(&table, 5)), ButtonAction::Unknown { raw: [0x40, 0x01, 0x00] });
    }

    #[test]
    fn builds_table_write_preserving_other_slots() {
        let table = hex(FACTORY);
        let f13 = ButtonAction::Keyboard { modifiers: 0, key: 0x68 };
        let report = build_table_write(&table, &[(4, encode(4, &f13))]);
        assert_eq!(&report[..4], &[0x09, 0x00, 0x01, 0x0F]);
        assert_eq!(slot_bytes(&report, 4), [0x70, 0x00, 0x68]);
        assert_eq!(slot_bytes(&report, 5), [0x40, 0x01, 0x00]);
        assert_eq!(slot_bytes(&report, 0), [0x10, 0x01, 0x00]);
        let sum = report[4..31].iter().fold(0u8, |a, b| a.wrapping_add(*b));
        assert_eq!(report[31], sum);
    }

    #[test]
    fn action_roundtrip() {
        for action in [
            ButtonAction::Default,
            ButtonAction::Keyboard { modifiers: 0b0001, key: 0x06 },
            ButtonAction::Media { usage: 0x00E9 },
            ButtonAction::RapidFire { interval: 20, count: 3 },
            ButtonAction::Macro,
        ] {
            assert_eq!(decode(3, encode(3, &action)), action);
        }
    }

    #[test]
    fn macro_single_and_chunked() {
        let ctrl_c = [
            MacroStep { kind: MacroKind::KeyDown, value: 0xE0, delay_ms: 10 },
            MacroStep { kind: MacroKind::KeyDown, value: 0x06, delay_ms: 10 },
            MacroStep { kind: MacroKind::KeyUp, value: 0x06, delay_ms: 10 },
            MacroStep { kind: MacroKind::KeyUp, value: 0xE0, delay_ms: 10 },
        ];
        let reports = build_macro_reports(&ctrl_c).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(&reports[0][..9], &[0x08, 0x00, 0x01, 17, 0x03, 0x30, 10, 0, 0xE0]);

        let long = [ctrl_c[0]; 10]; // 1 + 40 bytes = 2 blocos
        let reports = build_macro_reports(&long).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!((reports[0][2], reports[0][3]), (0x02, 27));
        assert_eq!((reports[1][2], reports[1][3]), (0x12, 14));

        assert!(build_macro_reports(&[ctrl_c[0]; 200]).is_err());
    }

    /// Grava F13 no botão lateral da frente, confere e restaura a fábrica.
    /// Escreve no mouse de verdade: `cargo test write_and_restore -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn write_and_restore_on_real_mouse() {
        println!("antes:   {:?}", read_buttons().unwrap());
        let f13 = ButtonAction::Keyboard { modifiers: 0, key: 0x68 };
        println!("gravado: {:?}", write_button(4, &f13, None).unwrap());
        println!("fábrica: {:?}", reset_buttons().unwrap());
    }

    /// Grava uma macro (F13) no botão lateral da frente, confere e restaura a fábrica.
    #[test]
    #[ignore]
    fn write_macro_and_restore_on_real_mouse() {
        let steps = [
            MacroStep { kind: MacroKind::KeyDown, value: 0x68, delay_ms: 20 },
            MacroStep { kind: MacroKind::KeyUp, value: 0x68, delay_ms: 20 },
        ];
        println!("macro:   {:?}", write_button(4, &ButtonAction::Macro, Some(&steps)).unwrap());
        println!("fábrica: {:?}", reset_buttons().unwrap());
    }
}
