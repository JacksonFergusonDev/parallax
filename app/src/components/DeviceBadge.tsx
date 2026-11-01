import type { DeviceInfo } from "../api";

export function DeviceBadge({ device }: { device: DeviceInfo | null }) {
  return device ? (
    <div
      className={`badge connected ${device.read_only ? "read-only" : ""}`}
      title={
        device.read_only
          ? `Firmware ${device.firmware}. Read-only: ${device.read_only}`
          : `Firmware ${device.firmware}`
      }
    >
      <span className="dot" />
      {device.model}
      {device.read_only && <span className="muted"> · read-only</span>}
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
