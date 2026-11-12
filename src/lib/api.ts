import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AiDataItem,
  AnalysisProgress,
  AnalyzeResult,
  AppCacheSize,
  AppInventory,
  CleanReport,
  Comparison,
  DirView,
  FileInfo,
  RevealTarget,
  RuleInfo,
  ScanProgress,
  ScanRequest,
  ScanSummary,
  SnapshotMeta,
  SupportedBrowser,
  VolumeInfo,
} from "../types";

export const inTauri = (): boolean => isTauri();

function requireTauri() {
  if (!inTauri()) {
    throw new Error("Filesystem access requires the DevSweep desktop app (run `npm run tauri dev`).");
  }
}

export async function listInstalledApps(): Promise<AppInventory> {
  requireTauri();
  return invoke<AppInventory>("list_installed_apps");
}

export async function measureAppCache(browser: SupportedBrowser): Promise<AppCacheSize[]> {
  requireTauri();
  return invoke<AppCacheSize[]>("measure_app_cache", { browser });
}

export async function inspectAiData(): Promise<AiDataItem[]> {
  requireTauri();
  return invoke<AiDataItem[]>("inspect_ai_data");
}

export async function inspectAiProject(): Promise<{ path: string; items: AiDataItem[] } | null> {
  requireTauri();
  const path = await open({ directory: true, multiple: false, title: "Choose a project folder" });
  if (typeof path !== "string") return null;
  return { path, items: await invoke<AiDataItem[]>("inspect_ai_project", { path }) };
}

// ----- Workspace -----------------------------------------------------------

export async function listRules(): Promise<RuleInfo[]> {
  if (!inTauri()) return [];
  return invoke<RuleInfo[]>("list_rules");
}

export async function validateRoot(path: string): Promise<string> {
  requireTauri();
  return invoke<string>("validate_root", { path });
}

export async function getExclusions(): Promise<string[]> {
  if (!inTauri()) return [];
  return invoke<string[]>("get_exclusions");
}

/** Replace the backend-owned exclusion list; returns the normalized list. */
export async function setExclusions(paths: string[]): Promise<string[]> {
  requireTauri();
  return invoke<string[]>("set_exclusions", { paths });
}

export async function startScan(request: ScanRequest): Promise<ScanSummary> {
  requireTauri();
  return invoke<ScanSummary>("start_scan", { request });
}

export async function cancelScan(): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_scan");
}

export async function cleanItems(
  sessionId: string,
  ids: string[],
  acknowledgedCaution: string[],
): Promise<CleanReport> {
  requireTauri();
  return invoke<CleanReport>("clean_items", { request: { sessionId, ids, acknowledgedCaution } });
}

export async function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  if (!inTauri()) return () => {};
  return listen<ScanProgress>("scan://progress", (e) => cb(e.payload));
}

/** Let the user pick one or more folders. Returns [] if cancelled. */
export async function pickFolders(title = "Choose folders to scan"): Promise<string[]> {
  requireTauri();
  const res = await open({ directory: true, multiple: true, title });
  if (res == null) return [];
  return Array.isArray(res) ? res : [res];
}

// ----- Disk Analyzer (read-only) -------------------------------------------

export async function listVolumes(): Promise<VolumeInfo[]> {
  if (!inTauri()) return [];
  return invoke<VolumeInfo[]>("list_volumes");
}

export async function analyzeVolume(root: string): Promise<AnalyzeResult> {
  requireTauri();
  return invoke<AnalyzeResult>("analyze_volume", { root });
}

export async function cancelAnalysis(): Promise<void> {
  if (!inTauri()) return;
  await invoke("cancel_analysis");
}

export async function analysisDir(analysisId: string, nodeId: number, limit = 100): Promise<DirView> {
  return invoke<DirView>("analysis_dir", { analysisId, nodeId, limit });
}

export async function analysisTopFiles(analysisId: string, nodeId: number, limit = 50): Promise<FileInfo[]> {
  return invoke<FileInfo[]>("analysis_top_files", { analysisId, nodeId, limit });
}

export async function revealInExplorer(analysisId: string, target: RevealTarget): Promise<void> {
  return invoke("reveal_in_explorer", { analysisId, target });
}

export async function onAnalysisProgress(cb: (p: AnalysisProgress) => void): Promise<UnlistenFn> {
  if (!inTauri()) return () => {};
  return listen<AnalysisProgress>("disk://progress", (e) => cb(e.payload));
}

export async function listSnapshots(): Promise<SnapshotMeta[]> {
  if (!inTauri()) return [];
  return invoke<SnapshotMeta[]>("list_snapshots");
}

export async function deleteSnapshot(id: string): Promise<void> {
  return invoke("delete_snapshot", { id });
}

export async function clearSnapshots(): Promise<number> {
  return invoke<number>("clear_snapshots");
}

export async function compareSnapshots(firstId: string, secondId: string, limit = 50): Promise<Comparison> {
  return invoke<Comparison>("compare_snapshots", { firstId, secondId, limit });
}
