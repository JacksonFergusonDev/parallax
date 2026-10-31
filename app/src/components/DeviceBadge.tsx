import type { DeviceInfo } from "../api";

export function DeviceBadge({ device }: { device: DeviceInfo | null }) {
  return device ? (
    <div className="badge connected" title={`Firmware ${device.firmware}`}>
      <span className="dot" />
      {device.model}
    </div>
  ) : (
    <div
      className="badge"
      title="Connect your Kobo with USB and tap Connect on its screen"
    >
      <span className="dot" />
      No device
    </div>
  );
}
