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
