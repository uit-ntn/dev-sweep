import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { CleanItemResult, RuleInfo, ScanProgress, ScanRequest, ScanSummary } from "../types";

export const inTauri = (): boolean => isTauri();

function requireTauri() {
  if (!inTauri()) {
    throw new Error("Filesystem access requires the DevSweep desktop app (run `npm run tauri dev`).");
  }
}

export async function listRules(): Promise<RuleInfo[]> {
  if (!inTauri()) return [];
  return invoke<RuleInfo[]>("list_rules");
}

export async function validateRoot(path: string): Promise<string> {
  requireTauri();
  return invoke<string>("validate_root", { path });
}

export async function startScan(request: ScanRequest): Promise<ScanSummary> {
  requireTauri();
  return invoke<ScanSummary>("start_scan", { request });
}

export async function cancelScan(): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_scan");
}

export async function cleanItems(ids: string[]): Promise<CleanItemResult[]> {
  requireTauri();
  return invoke<CleanItemResult[]>("clean_items", { ids });
}

export async function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  if (!inTauri()) return () => {};
  return listen<ScanProgress>("scan://progress", (e) => cb(e.payload));
}

/** Let the user pick one or more folders. Returns [] if cancelled. */
export async function pickFolders(): Promise<string[]> {
  requireTauri();
  const res = await open({ directory: true, multiple: true, title: "Choose folders to scan" });
  if (res == null) return [];
  return Array.isArray(res) ? res : [res];
}
