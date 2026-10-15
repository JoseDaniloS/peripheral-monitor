//! Bateria dos mouses AJAZZ (AJ179P MC e família).
//!
//! Protocolo documentado pelo projeto Ajazzy (github.com/Geovane2dd/Ajazzy),
//! capturado do driver oficial da AJAZZ e confirmado num AJ179P MC
//! (dev_id "179P", receptor 249A:5C2F):
//!
//! - Interface vendor MI_02, relatórios de 32 bytes sem report ID.
//! - Consulta GET 0x10 (informações do dispositivo): `10 00 00 00 00…00`.
//!   Só lê; não altera nenhuma configuração do mouse.
//! - Resposta: `10 00 01 <n> <payload 27 bytes> <checksum>`, com o dev_id
//!   ASCII em payload[0..4], carregando em payload[8] (byte 12) e a bateria
//!   (0–100 %) em payload[9] (byte 13).
//! - O mouse também envia sozinho `C0 01 <bateria> <carregando>` ao entrar ou
//!   sair do suporte de carga (pinos) e ao conectar/desconectar o cabo.
//! - Enquanto carrega, o mouse informa 100 % fixo; o valor real só volta
//!   depois que ele sai do carregador. Por isso `Tracker` ignora esse 100.

use std::time::{Duration, Instant};

pub const REPORT_LEN: usize = 32;
const QUERY_DEVICE_INFO: u8 = 0x10;
const STATUS_PUSH: u8 = 0xC0;
const CHARGING_BYTE: usize = 4 + 8;
const BATTERY_BYTE: usize = 4 + 9;

const DONGLES: &[(u16, u16)] = &[(0x249A, 0x5C2F), (0x248A, 0x5C2F)];
const WIRED: &[(u16, u16)] = &[(0x248A, 0x5C2E), (0x248A, 0x5D2E), (0x248A, 0x5E2E)];

/// VID/PIDs em que o mouse AJAZZ pode aparecer (receptor 2.4G ou cabo).
pub fn is_ajazz(vendor_id: u16, product_id: u16) -> bool {
    DONGLES.iter().chain(WIRED).any(|&(v, p)| v == vendor_id && p == product_id)
}

/// O mouse aparece nesses IDs quando está ligado pelo cabo USB.
pub fn is_wired(vendor_id: u16, product_id: u16) -> bool {
    WIRED.contains(&(vendor_id, product_id))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    /// Última leitura real; `None` se o app abriu com o mouse já carregando.
    pub percent: Option<u8>,
    pub charging: bool,
}

/// Uma leitura bruta do mouse, vinda da resposta 0x10 ou do push 0xC0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    pub raw: u8,
    pub charging: bool,
}

/// Acompanha as leituras e descarta o 100 % falso informado durante a carga.
#[derive(Default)]
pub struct Tracker {
    percent: Option<u8>,
    charging: bool,
}

impl Tracker {
    pub fn apply(&mut self, status: Status) {
        let was_charging = self.charging;
        self.charging = status.charging;
        // O primeiro 100 logo após sair do carregador também é falso.
        let fake_full = status.raw == 100 && (status.charging || was_charging);
        if status.raw <= 100 && !fake_full {
            self.percent = Some(status.raw);
        }
    }

    pub fn battery(&self) -> Battery {
        Battery { percent: self.percent, charging: self.charging }
    }
}

/// Conexão aberta com a interface vendor do mouse.
pub struct Link {
    dev: hidapi::HidDevice,
}

impl Link {
    /// Abre o receptor (ou, sem ele, o mouse no cabo). `None` se não houver mouse.
    pub fn open() -> Option<Link> {
        let api = hidapi::HidApi::new().ok()?;
        let mut candidates: Vec<_> = api
            .device_list()
            .filter(|d| d.interface_number() == 2 && is_ajazz(d.vendor_id(), d.product_id()))
            .collect();
        candidates.sort_by_key(|d| is_wired(d.vendor_id(), d.product_id()));
        candidates.iter().find_map(|info| info.open_device(&api).ok()).map(|dev| Link { dev })
    }

    /// Envia a consulta 0x10; a resposta chega por `next`.
    pub fn request_info(&self) -> bool {
        // Byte 0 é o report ID (0 = sem IDs); depois vêm os 32 bytes do protocolo.
        let mut request = [0u8; REPORT_LEN + 1];
        request[1] = QUERY_DEVICE_INFO;
        self.dev.write(&request).is_ok()
    }

    /// Envia um relatório de 32 bytes e espera o eco/resposta com o mesmo
    /// `byte0` (família ou id da consulta) e modo 0x01.
    /// `Ok(None)` se o mouse não respondeu dentro do prazo.
    pub fn transact(&self, report: &[u8; REPORT_LEN], timeout: Duration) -> Result<Option<[u8; REPORT_LEN]>, String> {
        let mut request = [0u8; REPORT_LEN + 1];
        request[1..].copy_from_slice(report);
        self.dev.write(&request).map_err(|e| format!("Falha ao enviar ao mouse: {e}"))?;
        let mut buf = [0u8; 64];
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let n = self.dev.read_timeout(&mut buf, 200).map_err(|e| format!("Falha ao ler do mouse: {e}"))?;
            if n >= REPORT_LEN && buf[0] == report[0] && buf[2] & 0x0F != 0 {
                let mut reply = [0u8; REPORT_LEN];
                reply.copy_from_slice(&buf[..REPORT_LEN]);
                return Ok(Some(reply));
            }
        }
        Ok(None)
    }

    /// Espera até `timeout` por um relatório. `Err` se o dispositivo sumiu.
    pub fn next(&self, timeout: Duration) -> Result<Option<Status>, ()> {
        let mut buf = [0u8; 64];
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let n = self.dev.read_timeout(&mut buf, 200).map_err(|_| ())?;
            if n >= REPORT_LEN {
                if let Some(status) = parse_report(&buf[..REPORT_LEN]) { return Ok(Some(status)); }
            }
        }
        Ok(None)
    }
}

fn parse_report(report: &[u8]) -> Option<Status> {
    match report[0] {
        STATUS_PUSH => Some(Status { raw: report[2], charging: report[3] == 1 }),
        QUERY_DEVICE_INFO if report[2] == 0x01 => {
            let checksum = report[4..REPORT_LEN - 1].iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
            // Sem os bytes de bateria entre os válidos, o mouse não respondeu de fato.
            if checksum != report[REPORT_LEN - 1] || (report[3] as usize) <= BATTERY_BYTE - 4 { return None; }
            Some(Status { raw: report[BATTERY_BYTE], charging: report[CHARGING_BYTE] == 1 })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|h| u8::from_str_radix(h, 16).unwrap()).collect()
    }

    // Relatórios reais de um AJ179P MC, receptor 249A:5C2F.
    const INFO_IDLE: &str = "10 00 01 0B 31 37 39 50 84 01 11 01 00 4D 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 D6";
    const INFO_DOCKED: &str = "10 00 01 0B 31 37 39 50 84 01 11 01 01 64 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 EE";
    const PUSH_DOCKED: &str = "C0 01 64 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00";
    const PUSH_UNDOCKED_100: &str = "C0 01 64 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00";
    const PUSH_UNDOCKED_77: &str = "C0 01 4D 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00";

    #[test]
    fn parses_info_reply_and_push() {
        assert_eq!(parse_report(&hex(INFO_IDLE)), Some(Status { raw: 77, charging: false }));
        assert_eq!(parse_report(&hex(INFO_DOCKED)), Some(Status { raw: 100, charging: true }));
        assert_eq!(parse_report(&hex(PUSH_DOCKED)), Some(Status { raw: 100, charging: true }));
    }

    #[test]
    fn rejects_empty_or_corrupt_reply() {
        let empty = hex("10 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(parse_report(&empty), None);
        let mut bad_sum = hex(INFO_IDLE);
        bad_sum[31] ^= 1;
        assert_eq!(parse_report(&bad_sum), None);
    }

    #[test]
    fn dock_cycle_keeps_real_percent() {
        // Sequência capturada: no suporte, fora do suporte.
        let mut t = Tracker::default();
        let mut step = |r: &str| { t.apply(parse_report(&hex(r)).unwrap()); t.battery() };
        assert_eq!(step(INFO_IDLE), Battery { percent: Some(77), charging: false });
        assert_eq!(step(PUSH_DOCKED), Battery { percent: Some(77), charging: true });
        assert_eq!(step(INFO_DOCKED), Battery { percent: Some(77), charging: true });
        assert_eq!(step(PUSH_UNDOCKED_100), Battery { percent: Some(77), charging: false });
        assert_eq!(step(PUSH_UNDOCKED_77), Battery { percent: Some(77), charging: false });
    }

    #[test]
    fn opened_while_charging_has_no_percent_yet() {
        let mut t = Tracker::default();
        t.apply(Status { raw: 100, charging: true });
        assert_eq!(t.battery(), Battery { percent: None, charging: true });
    }

    /// Lê a bateria do mouse conectado: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn prints_battery() {
        let link = Link::open().expect("mouse AJAZZ não encontrado");
        assert!(link.request_info());
        println!("{:?}", link.next(Duration::from_secs(1)));
    }
}
