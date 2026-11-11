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
  sizeBytes: number;
  fileCount: number;
  unreadable: number;
  lastModified: number | null;
}

export interface ScanRequest {
  roots: string[];
  exclusions: string[];
}

export interface ScanProgress {
  dirsVisited: number;
  candidatesFound: number;
  bytesFound: number;
  errors: number;
  currentPath: string;
  newCandidates: Candidate[];
}

export interface ScanSummary {
  roots: string[];
  candidates: Candidate[];
  dirsVisited: number;
  errors: number;
  totalBytes: number;
  cancelled: boolean;
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

/** Privacy-conscious history entry: aggregate numbers only, no paths. */
export interface HistoryEntry {
  at: number;
  kind: "scan" | "cleanup";
  items: number;
  bytes: number;
  failed?: number;
}

export type Page = "dashboard" | "scanner" | "review" | "settings";
