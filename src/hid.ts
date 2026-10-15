// Códigos da página HID Keyboard (USB HID Usage Tables, seção 10) e da página
// Consumer, usados nos botões e macros do mouse AJAZZ.

export type HidKey = { usage: number; label: string; code?: string };

const letters: HidKey[] = Array.from({ length: 26 }, (_, i) => {
  const ch = String.fromCharCode(65 + i);
  return { usage: 0x04 + i, label: ch, code: `Key${ch}` };
});
const digits: HidKey[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0].map((d, i) => ({ usage: 0x1e + i, label: String(d), code: `Digit${d}` }));
const fKeys: HidKey[] = Array.from({ length: 24 }, (_, i) => ({
  usage: i < 12 ? 0x3a + i : 0x68 + (i - 12),
  label: `F${i + 1}`,
  code: `F${i + 1}`,
}));
const numpad: HidKey[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0].map((d, i) => ({ usage: 0x59 + i, label: `Num ${d}`, code: `Numpad${d}` }));

export const KEYS: HidKey[] = [
  ...letters, ...digits, ...fKeys, ...numpad,
  { usage: 0x28, label: "Enter", code: "Enter" },
  { usage: 0x29, label: "Esc", code: "Escape" },
  { usage: 0x2a, label: "Backspace", code: "Backspace" },
  { usage: 0x2b, label: "Tab", code: "Tab" },
  { usage: 0x2c, label: "Espaço", code: "Space" },
  { usage: 0x2d, label: "-", code: "Minus" },
  { usage: 0x2e, label: "=", code: "Equal" },
  { usage: 0x2f, label: "´", code: "BracketLeft" },
  { usage: 0x30, label: "[", code: "BracketRight" },
  { usage: 0x31, label: "]", code: "Backslash" },
  { usage: 0x33, label: "Ç", code: "Semicolon" },
  { usage: 0x34, label: "~", code: "Quote" },
  { usage: 0x35, label: "'", code: "Backquote" },
  { usage: 0x36, label: ",", code: "Comma" },
  { usage: 0x37, label: ".", code: "Period" },
  { usage: 0x38, label: ";", code: "Slash" },
  { usage: 0x39, label: "Caps Lock", code: "CapsLock" },
  { usage: 0x46, label: "Print Screen", code: "PrintScreen" },
  { usage: 0x47, label: "Scroll Lock", code: "ScrollLock" },
  { usage: 0x48, label: "Pause", code: "Pause" },
  { usage: 0x49, label: "Insert", code: "Insert" },
  { usage: 0x4a, label: "Home", code: "Home" },
  { usage: 0x4b, label: "Page Up", code: "PageUp" },
  { usage: 0x4c, label: "Delete", code: "Delete" },
  { usage: 0x4d, label: "End", code: "End" },
  { usage: 0x4e, label: "Page Down", code: "PageDown" },
  { usage: 0x4f, label: "→", code: "ArrowRight" },
  { usage: 0x50, label: "←", code: "ArrowLeft" },
  { usage: 0x51, label: "↓", code: "ArrowDown" },
  { usage: 0x52, label: "↑", code: "ArrowUp" },
  { usage: 0x53, label: "Num Lock", code: "NumLock" },
  { usage: 0x54, label: "Num /", code: "NumpadDivide" },
  { usage: 0x55, label: "Num *", code: "NumpadMultiply" },
  { usage: 0x56, label: "Num -", code: "NumpadSubtract" },
  { usage: 0x57, label: "Num +", code: "NumpadAdd" },
  { usage: 0x58, label: "Num Enter", code: "NumpadEnter" },
  { usage: 0x63, label: "Num ,", code: "NumpadDecimal" },
  { usage: 0x85, label: "Num .", code: "NumpadComma" },
  { usage: 0x64, label: "\\", code: "IntlBackslash" },
  { usage: 0x87, label: "/", code: "IntlRo" },
  { usage: 0x65, label: "Menu", code: "ContextMenu" },
];

export const MODIFIER_KEYS: HidKey[] = [
  { usage: 0xe0, label: "Ctrl", code: "ControlLeft" },
  { usage: 0xe1, label: "Shift", code: "ShiftLeft" },
  { usage: 0xe2, label: "Alt", code: "AltLeft" },
  { usage: 0xe3, label: "Win", code: "MetaLeft" },
  { usage: 0xe4, label: "Ctrl dir.", code: "ControlRight" },
  { usage: 0xe5, label: "Shift dir.", code: "ShiftRight" },
  { usage: 0xe6, label: "AltGr", code: "AltRight" },
  { usage: 0xe7, label: "Win dir.", code: "MetaRight" },
];

const byCode = new Map([...KEYS, ...MODIFIER_KEYS].map(k => [k.code, k]));
const byUsage = new Map([...KEYS, ...MODIFIER_KEYS].map(k => [k.usage, k]));

export const keyFromCode = (code: string) => byCode.get(code);
export const keyLabel = (usage: number) => byUsage.get(usage)?.label ?? `0x${usage.toString(16).padStart(2, "0")}`;
export const isModifierCode = (code: string) => MODIFIER_KEYS.some(k => k.code === code);

/** Máscara do relatório HID: bit0 Ctrl, bit1 Shift, bit2 Alt, bit3 Win. */
export const MOD = { ctrl: 1, shift: 2, alt: 4, win: 8 } as const;

export function modifiersFromEvent(e: { ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }): number {
  return (e.ctrlKey ? MOD.ctrl : 0) | (e.shiftKey ? MOD.shift : 0) | (e.altKey ? MOD.alt : 0) | (e.metaKey ? MOD.win : 0);
}

export function shortcutLabel(modifiers: number, key: number): string {
  const parts: string[] = [];
  if (modifiers & (MOD.ctrl | 0x10)) parts.push("Ctrl");
  if (modifiers & (MOD.shift | 0x20)) parts.push("Shift");
  if (modifiers & (MOD.alt | 0x40)) parts.push("Alt");
  if (modifiers & (MOD.win | 0x80)) parts.push("Win");
  if (key) parts.push(keyLabel(key));
  return parts.join(" + ") || "—";
}

/** Teclas sem equivalente no teclado comum, para escolher numa lista. */
export const SPECIAL_KEYS: HidKey[] = [...fKeys.slice(12), byCode.get("PrintScreen")!, byCode.get("Pause")!, byCode.get("ContextMenu")!];

export const MEDIA_KEYS: { usage: number; label: string }[] = [
  { usage: 0x00cd, label: "Play / Pause" },
  { usage: 0x00b5, label: "Próxima faixa" },
  { usage: 0x00b6, label: "Faixa anterior" },
  { usage: 0x00b7, label: "Parar" },
  { usage: 0x00e9, label: "Aumentar volume" },
  { usage: 0x00ea, label: "Diminuir volume" },
  { usage: 0x00e2, label: "Mudo" },
  { usage: 0x0223, label: "Navegador: página inicial" },
  { usage: 0x0224, label: "Navegador: voltar" },
  { usage: 0x0225, label: "Navegador: avançar" },
  { usage: 0x0227, label: "Navegador: atualizar" },
  { usage: 0x0221, label: "Pesquisar" },
  { usage: 0x018a, label: "E-mail" },
  { usage: 0x0192, label: "Calculadora" },
  { usage: 0x0194, label: "Meu computador" },
];

export const mediaLabel = (usage: number) => MEDIA_KEYS.find(m => m.usage === usage)?.label ?? `Mídia 0x${usage.toString(16)}`;

/** Tecla do evento; sem `code` (alguns teclados virtuais), usa a letra/dígito de `key`. */
export function keyFromEvent(e: { code: string; key: string }): HidKey | undefined {
  return byCode.get(e.code) ?? (/^[a-z0-9]$/i.test(e.key) ? byCode.get(/\d/.test(e.key) ? `Digit${e.key}` : `Key${e.key.toUpperCase()}`) : undefined);
}

/** Teclas modificadoras ativas no evento, como códigos HID (para macros). */
export function heldModifierUsages(e: { ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }): number[] {
  return [e.ctrlKey && 0xe0, e.shiftKey && 0xe1, e.altKey && 0xe2, e.metaKey && 0xe3].filter((u): u is number => typeof u === "number");
}
