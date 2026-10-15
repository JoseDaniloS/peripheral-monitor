export type Peripheral = {
  id: string;
  name: string;
  manufacturer: string | null;
  vendorId: number;
  productId: number;
  serialNumber: string | null;
  path: string;
  deviceClass: string | null;
  batteryPercent: number | null;
  batteryCharging: boolean;
  transport: "USB" | "Bluetooth";
};
/** occurredAt: milissegundos desde a época Unix. */
export type DeviceEvent = { kind: "connected" | "disconnected"; device: Peripheral; occurredAt: number };
export type Settings = { notifyConnection: boolean; notifyBattery: boolean };

/** Ação de um botão do mouse AJAZZ (espelha `mouse_buttons::ButtonAction`). */
export type ButtonAction =
  | { type: "default" }
  | { type: "keyboard"; modifiers: number; key: number }
  | { type: "media"; usage: number }
  | { type: "rapidFire"; interval: number; count: number }
  | { type: "macro" }
  | { type: "unknown"; raw: [number, number, number] };
export type MouseButton = { slot: number; action: ButtonAction };
export type MacroStep = { kind: "keyDown" | "keyUp" | "mouseDown" | "mouseUp"; value: number; delayMs: number };
