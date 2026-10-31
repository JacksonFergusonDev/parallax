// Typed wrappers over the Tauri commands in src-tauri/src/lib.rs. Shapes
// mirror the serde output of the Rust types.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface DeviceInfo {
  kind: "kobo";
  model: string;
  firmware: string;
  serial: string;
  mount: string;
  capabilities: {
    native_collections: boolean;
    series: boolean;
    reading_progress: boolean;
    reading_time: boolean;
  };
}

export type Progress =
  | { stage: "converting"; index: number; total: number; file: string }
  | { stage: "copying"; index: number; total: number; file: string }
  | {
      stage: "done";
      index: number;
      total: number;
      file: string;
      device_path: string;
    }
  | {
      stage: "failed";
      index: number;
      total: number;
      file: string;
      error: string;
    };

export interface SentInfo {
  device_path: string;
  content_id: string;
  bytes: number;
}

export interface SendOutcome {
  source: string;
  result: { Ok: SentInfo } | { Err: string };
}

export const listDevices = () => invoke<DeviceInfo[]>("list_devices");

export const sendToDevice = (mount: string, files: string[]) =>
  invoke<SendOutcome[]>("send_to_device", { mount, files });

export const ejectDevice = (mount: string) =>
  invoke<void>("eject_device", { mount });

export const onSendProgress = (
  handler: (p: Progress) => void,
): Promise<UnlistenFn> =>
  listen<Progress>("send-progress", (e) => handler(e.payload));
