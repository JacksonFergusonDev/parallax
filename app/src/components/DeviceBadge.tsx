import type { DeviceInfo } from "../api";

/** A tiny e-reader whose screen lights up while one is connected. */
function ReaderIcon() {
  return (
    <svg
      className="reader-icon"
      viewBox="0 0 14 18"
      width="12"
      height="16"
      aria-hidden="true"
    >
      <rect
        x="0.75"
        y="0.75"
        width="12.5"
        height="16.5"
        rx="2.25"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      <rect
        className="reader-screen"
        x="3"
        y="3"
        width="8"
        height="10"
        rx="0.75"
      />
      <rect
        className="reader-line"
        x="4.5"
        y="5"
        width="5"
        height="1"
        rx="0.5"
      />
      <rect
        className="reader-line"
        x="4.5"
        y="7.5"
        width="5"
        height="1"
        rx="0.5"
      />
      <rect
        className="reader-line"
        x="4.5"
        y="10"
        width="3.5"
        height="1"
        rx="0.5"
      />
    </svg>
  );
}

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
      <ReaderIcon />
      {device.model}
      {device.read_only && <span className="muted"> · read-only</span>}
      <span className="dot" />
    </div>
  ) : (
    <div
      className="badge"
      title="Connect your Kobo with USB and tap Connect on its screen"
    >
      <ReaderIcon />
      No device
    </div>
  );
}
