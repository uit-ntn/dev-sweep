import { create } from "zustand";
import * as api from "../lib/api";
import { load, save } from "../lib/storage";
import type {
  Candidate,
  CleanItemResult,
  HistoryEntry,
  Page,
  Risk,
  RuleInfo,
  ScanProgress,
  ScanSummary,
} from "../types";

export type SortKey = "size" | "path" | "modified";
export type Theme = "system" | "light" | "dark";

export interface Filters {
  text: string;
  ecosystems: string[]; // empty = all
  risks: Risk[]; // empty = all
  minBytes: number;
}

interface Prefs {
  roots: string[];
  exclusions: string[];
  theme: Theme;
}

const PREFS_KEY = "devsweep.prefs.v1";
const HISTORY_KEY = "devsweep.history.v1";
const HISTORY_LIMIT = 50;

const initialPrefs = load<Prefs>(PREFS_KEY, { roots: [], exclusions: [], theme: "system" });

interface SweepState extends Prefs {
  page: Page;
  rules: RuleInfo[];

  scanning: boolean;
  progress: ScanProgress | null;
  liveCandidates: Candidate[];
  summary: ScanSummary | null;
  scanError: string | null;

  filters: Filters;
  sort: SortKey;
  selected: Set<string>;
  inspectedId: string | null;

  cleaning: boolean;
  cleanResults: CleanItemResult[] | null;
  cleanError: string | null;

  history: HistoryEntry[];

  setPage(p: Page): void;
  loadRules(): Promise<void>;
  addRoots(paths: string[]): Promise<string[]>;
  removeRoot(path: string): void;
  addExclusions(paths: string[]): void;
  removeExclusion(path: string): void;
  setTheme(t: Theme): void;

  startScan(): Promise<void>;
  cancelScan(): Promise<void>;
  handleProgress(p: ScanProgress): void;

  setFilters(f: Partial<Filters>): void;
  setSort(s: SortKey): void;
  toggleSelected(id: string): void;
  setSelected(ids: string[], on: boolean): void;
  clearSelection(): void;
  inspect(id: string | null): void;

  runCleanup(): Promise<void>;
  resetCleanup(): void;
  clearHistory(): void;
}

function persistPrefs(s: Prefs) {
  save(PREFS_KEY, { roots: s.roots, exclusions: s.exclusions, theme: s.theme });
}

function pushHistory(history: HistoryEntry[], entry: HistoryEntry): HistoryEntry[] {
  const next = [entry, ...history].slice(0, HISTORY_LIMIT);
  save(HISTORY_KEY, next);
  return next;
}

export const useSweepStore = create<SweepState>((set, get) => ({
  ...initialPrefs,
  page: "dashboard",
  rules: [],

  scanning: false,
  progress: null,
  liveCandidates: [],
  summary: null,
  scanError: null,

  filters: { text: "", ecosystems: [], risks: [], minBytes: 0 },
  sort: "size",
  selected: new Set(),
  inspectedId: null,

  cleaning: false,
  cleanResults: null,
  cleanError: null,

  history: load<HistoryEntry[]>(HISTORY_KEY, []),

  setPage: (page) => set({ page }),

  loadRules: async () => {
    try {
      set({ rules: await api.listRules() });
    } catch {
      /* rules are informational only */
    }
  },

  addRoots: async (paths) => {
    const errors: string[] = [];
    const roots = [...get().roots];
    for (const p of paths) {
      try {
        const normalized = await api.validateRoot(p);
        if (!roots.includes(normalized)) roots.push(normalized);
      } catch (e) {
        errors.push(`${p}: ${String(e)}`);
      }
    }
    set({ roots });
    persistPrefs(get());
    return errors;
  },

  removeRoot: (path) => {
    set({ roots: get().roots.filter((r) => r !== path) });
    persistPrefs(get());
  },

  addExclusions: (paths) => {
    const exclusions = Array.from(new Set([...get().exclusions, ...paths]));
    set({ exclusions });
    persistPrefs(get());
  },

  removeExclusion: (path) => {
    set({ exclusions: get().exclusions.filter((e) => e !== path) });
    persistPrefs(get());
  },

  setTheme: (theme) => {
    set({ theme });
    persistPrefs(get());
  },

  startScan: async () => {
    const { roots, exclusions, scanning } = get();
    if (scanning || roots.length === 0) return;
    set({
      scanning: true,
      progress: null,
      liveCandidates: [],
      summary: null,
      scanError: null,
      selected: new Set(),
      inspectedId: null,
      cleanResults: null,
      cleanError: null,
    });
    try {
      const summary = await api.startScan({ roots, exclusions });
      set({
        summary,
        liveCandidates: summary.candidates,
        history: pushHistory(get().history, {
          at: Date.now(),
          kind: "scan",
          items: summary.candidates.length,
          bytes: summary.totalBytes,
        }),
      });
    } catch (e) {
      set({ scanError: String(e) });
    } finally {
      set({ scanning: false });
    }
  },

  cancelScan: async () => {
    await api.cancelScan();
  },

  handleProgress: (p) => {
    if (!get().scanning) return;
    set((s) => ({
      progress: p,
      liveCandidates: p.newCandidates.length
        ? [...s.liveCandidates, ...p.newCandidates]
        : s.liveCandidates,
    }));
  },

  setFilters: (f) => set({ filters: { ...get().filters, ...f } }),
  setSort: (sort) => set({ sort }),

  toggleSelected: (id) => {
    const selected = new Set(get().selected);
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    set({ selected });
  },

  setSelected: (ids, on) => {
    const selected = new Set(get().selected);
    for (const id of ids) {
      if (on) selected.add(id);
      else selected.delete(id);
    }
    set({ selected });
  },

  clearSelection: () => set({ selected: new Set() }),
  inspect: (inspectedId) => set({ inspectedId }),

  runCleanup: async () => {
    const ids = Array.from(get().selected);
    if (ids.length === 0 || get().cleaning) return;
    set({ cleaning: true, cleanError: null, cleanResults: null });
    try {
      const results = await api.cleanItems(ids);
      const trashed = new Set(results.filter((r) => r.status === "trashed").map((r) => r.id));
      const bytes = results
        .filter((r) => r.status === "trashed")
        .reduce((sum, r) => sum + r.sizeBytes, 0);
      const summary = get().summary;
      set({
        cleanResults: results,
        selected: new Set(ids.filter((id) => !trashed.has(id))),
        liveCandidates: get().liveCandidates.filter((c) => !trashed.has(c.id)),
        summary: summary && {
          ...summary,
          candidates: summary.candidates.filter((c) => !trashed.has(c.id)),
          totalBytes: summary.totalBytes - bytes,
        },
        history: pushHistory(get().history, {
          at: Date.now(),
          kind: "cleanup",
          items: trashed.size,
          bytes,
          failed: results.length - trashed.size,
        }),
      });
    } catch (e) {
      set({ cleanError: String(e) });
    } finally {
      set({ cleaning: false });
    }
  },

  resetCleanup: () => set({ cleanResults: null, cleanError: null }),

  clearHistory: () => {
    save(HISTORY_KEY, []);
    set({ history: [] });
  },
}));

/** Candidates after filters and sorting are applied (pure; memoize at call site). */
export function filterAndSort(candidates: Candidate[], filters: Filters, sort: SortKey): Candidate[] {
  const { text, ecosystems, risks, minBytes } = filters;
  const q = text.trim().toLowerCase();
  const list = candidates.filter(
    (c) =>
      (ecosystems.length === 0 || ecosystems.includes(c.ecosystem)) &&
      (risks.length === 0 || risks.includes(c.risk)) &&
      c.sizeBytes >= minBytes &&
      (q === "" || c.path.toLowerCase().includes(q) || c.displayName.toLowerCase().includes(q)),
  );
  const sorted = [...list];
  switch (sort) {
    case "size":
      sorted.sort((a, b) => b.sizeBytes - a.sizeBytes);
      break;
    case "path":
      sorted.sort((a, b) => a.path.localeCompare(b.path));
      break;
    case "modified":
      sorted.sort((a, b) => (a.lastModified ?? 0) - (b.lastModified ?? 0));
      break;
  }
  return sorted;
}
