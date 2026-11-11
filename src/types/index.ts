// Mirrors the serialized Rust types in src-tauri/src.

export type Risk = "low" | "review" | "caution";
export type Category = "dependencies" | "environment" | "cache" | "build_output" | "checkpoints";

export interface RuleInfo {
  id: string;
  display_name: string;
  ecosystem: string;
  match_names: string[];
  category: Category;
  risk: Risk;
  default_selected: boolean;
  description: string;
  context_hint: string;
}

export interface Candidate {
  /** Session-scoped id: `<sessionId>:c<n>`. */
  id: string;
  path: string;
  name: string;
  projectDir: string;
  root: string;
  ruleId: string;
  displayName: string;
  ecosystem: string;
  category: Category;
  risk: Risk;
  /** Logical size at scan time (estimate). */
  sizeBytes: number;
  fileCount: number;
  unreadable: number;
  sizeComplete: boolean;
  lastModified: number | null;
  /** May be included by "select all visible". False for Caution / blocked items. */
  bulkSelectable: boolean;
  /** Set when the item can never be cleaned (e.g. contains an excluded folder). */
  blockedReason: string | null;
}

export interface ScanRequest {
  roots: string[];
}

export interface ScanProgress {
  sessionId: string;
  dirsVisited: number;
  candidatesFound: number;
  bytesFound: number;
  errors: number;
  currentPath: string;
  newCandidates: Candidate[];
}

export interface ScanSummary {
  sessionId: string;
  roots: string[];
  exclusions: string[];
  candidates: Candidate[];
  dirsVisited: number;
  errors: number;
  totalBytes: number;
  cancelled: boolean;
  incomplete: boolean;
  elapsedMs: number;
}

export type CleanStatus = "trashed" | "skipped" | "failed";

export interface CleanItemResult {
  id: string;
  path: string;
  status: CleanStatus;
  message: string;
  sizeBytes: number;
}

export interface VolumeSpace {
  volume: string;
  freeBefore: number;
  freeAfter: number;
}

export interface CleanReport {
  sessionId: string;
  results: CleanItemResult[];
  /** Free space measured by the OS; empty when it could not be measured. */
  freeSpace: VolumeSpace[];
}

/** Privacy-conscious history entry: aggregate numbers only, no paths. */
export interface HistoryEntry {
  at: number;
  kind: "scan" | "cleanup";
  items: number;
  /** Estimated logical bytes (found, or moved to Trash). */
  bytes: number;
  failed?: number;
}

// ----- Disk Analyzer -------------------------------------------------------

export interface VolumeInfo {
  root: string;
  label: string;
  fileSystem: string;
  serial: number;
  totalBytes: number;
  freeBytes: number;
  removable: boolean;
}

export interface AnalysisProgress {
  analysisId: string;
  dirsRead: number;
  dirsUnreadable: number;
  filesSeen: number;
  bytesSeen: number;
  currentPath: string;
}

export interface AnalysisSummary {
  analysisId: string;
  root: string;
  totalBytes: number;
  files: number;
  dirsRead: number;
  dirsUnreadable: number;
  linksSkipped: number;
  cancelled: boolean;
  truncated: boolean;
  incomplete: boolean;
  startedAtMs: number;
  finishedAtMs: number;
}

export interface SnapshotMeta {
  id: string;
  volume: { root: string; serial: number; label: string };
  takenAtMs: number;
  totalBytes: number;
  freeBytes: number;
  logicalBytes: number;
  dirsUnreadable: number;
  entries: number;
}

export interface AnalyzeResult {
  summary: AnalysisSummary;
  snapshot: SnapshotMeta | null;
  snapshotError: string | null;
}

export interface NodeInfo {
  id: number;
  name: string;
  path: string;
  bytes: number;
  files: number;
  hasChildren: boolean;
  unreadable: boolean;
  incomplete: boolean;
}

export interface DirView {
  analysisId: string;
  node: NodeInfo;
  breadcrumb: { id: number; name: string }[];
  children: NodeInfo[];
  moreChildren: number;
  moreChildrenBytes: number;
  ownFilesBytes: number;
}

export interface FileInfo {
  id: number;
  name: string;
  path: string;
  folderId: number;
  bytes: number;
}

export type RevealTarget = { kind: "folder"; id: number } | { kind: "file"; id: number };

export type DeltaStatus = "changed" | "added" | "removed";

export interface DirDelta {
  path: string;
  olderBytes: number | null;
  newerBytes: number | null;
  delta: number;
  status: DeltaStatus;
  approximate: boolean;
}

export interface Comparison {
  older: SnapshotMeta;
  newer: SnapshotMeta;
  intervalMs: number;
  freeDelta: number;
  logicalDelta: number;
  partial: boolean;
  partialReasons: string[];
  growers: DirDelta[];
  shrinkers: DirDelta[];
  unreadableSkipped: number;
}

export type Page = "dashboard" | "scanner" | "review" | "disk" | "settings";
