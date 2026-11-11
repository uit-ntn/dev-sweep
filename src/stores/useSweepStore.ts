import { create } from "zustand";
import * as api from "../lib/api";
import { load, save } from "../lib/storage";
import type {
  Candidate,
  CleanReport,
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
  theme: Theme;
}

/** v1 prefs also held exclusions; they are migrated to the backend once. */
interface LegacyPrefs extends Prefs {
  exclusions?: string[];
}

const PREFS_KEY = "devsweep.prefs.v1";
const HISTORY_KEY = "devsweep.history.v1";
const HISTORY_LIMIT = 50;

const initialPrefs = load<LegacyPrefs>(PREFS_KEY, { roots: [], theme: "system" });

/** Only items that are cleanable at all can be selected. */
export const isSelectable = (c: Candidate) => c.blockedReason == null;

interface SweepState extends Prefs {
  page: Page;
  rules: RuleInfo[];

  /** Backend-owned exclusion list (source of truth lives in Rust). */
  exclusions: string[];
  exclusionError: string | null;

  scanning: boolean;
  progress: ScanProgress | null;
  liveCandidates: Candidate[];
  summary: ScanSummary | null;
  /** Set when exclusions changed after `summary` was produced. */
  summaryStale: boolean;
  scanError: string | null;

  filters: Filters;
  sort: SortKey;
  selected: Set<string>;
  /** Caution items the user individually confirmed on the review screen. */
  acknowledged: Set<string>;
  inspectedId: string | null;

  cleaning: boolean;
  cleanReport: CleanReport | null;
  cleanError: string | null;

  history: HistoryEntry[];

  setPage(p: Page): void;
  init(): Promise<void>;
  addRoots(paths: string[]): Promise<string[]>;
  removeRoot(path: string): void;
  addExclusions(paths: string[]): Promise<void>;
  removeExclusion(path: string): Promise<void>;
  setTheme(t: Theme): void;

  startScan(): Promise<void>;
  cancelScan(): Promise<void>;
  handleProgress(p: ScanProgress): void;

  setFilters(f: Partial<Filters>): void;
  setSort(s: SortKey): void;
  toggleSelected(id: string): void;
  /** Bulk selection: only adds bulk-selectable items (never Caution). */
  selectBulk(candidates: Candidate[], on: boolean): void;
  clearSelection(): void;
  toggleAcknowledged(id: string): void;
  inspect(id: string | null): void;

  runCleanup(): Promise<void>;
  resetCleanup(): void;
  clearHistory(): void;
}

function persistPrefs(s: Prefs) {
  save(PREFS_KEY, { roots: s.roots, theme: s.theme });
}

function pushHistory(history: HistoryEntry[], entry: HistoryEntry): HistoryEntry[] {
  const next = [entry, ...history].slice(0, HISTORY_LIMIT);
  save(HISTORY_KEY, next);
  return next;
}

export const useSweepStore = create<SweepState>((set, get) => ({
  roots: initialPrefs.roots,
  theme: initialPrefs.theme,
  page: "dashboard",
  rules: [],

  exclusions: [],
  exclusionError: null,

  scanning: false,
  progress: null,
  liveCandidates: [],
  summary: null,
  summaryStale: false,
  scanError: null,

  filters: { text: "", ecosystems: [], risks: [], minBytes: 0 },
  sort: "size",
  selected: new Set(),
  acknowledged: new Set(),
  inspectedId: null,

  cleaning: false,
  cleanReport: null,
  cleanError: null,

  history: load<HistoryEntry[]>(HISTORY_KEY, []),

  setPage: (page) => set({ page }),

  init: async () => {
    try {
      set({ rules: await api.listRules() });
    } catch {
      /* rules are informational only */
    }
    try {
      let exclusions = await api.getExclusions();
      const legacy = initialPrefs.exclusions ?? [];
      if (exclusions.length === 0 && legacy.length > 0 && api.inTauri()) {
        exclusions = await api.setExclusions(legacy);
      }
      persistPrefs(get()); // drops the legacy field
      set({ exclusions });
    } catch (e) {
      set({ exclusionError: String(e) });
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

  addExclusions: async (paths) => {
    await applyExclusions(Array.from(new Set([...get().exclusions, ...paths])));
  },

  removeExclusion: async (path) => {
    await applyExclusions(get().exclusions.filter((e) => e !== path));
  },

  setTheme: (theme) => {
    set({ theme });
    persistPrefs(get());
  },

  startScan: async () => {
    const { roots, scanning, cleaning } = get();
    if (scanning || cleaning || roots.length === 0) return;
    set({
      scanning: true,
      progress: null,
      liveCandidates: [],
      summary: null,
      summaryStale: false,
      scanError: null,
      selected: new Set(),
      acknowledged: new Set(),
      inspectedId: null,
      cleanReport: null,
      cleanError: null,
    });
    try {
      const summary = await api.startScan({ roots });
      set({
        summary,
        liveCandidates: summary.candidates,
        history: pushHistory(get().history, {
          at: Date.now(),
          kind: "scan",
          items: summary.candidates.filter(isSelectable).length,
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
    const s = get();
    if (!s.scanning) return;
    // Ignore late events from an earlier scan session.
    if (s.progress && s.progress.sessionId !== p.sessionId) return;
    set({
      progress: p,
      liveCandidates: p.newCandidates.length ? [...s.liveCandidates, ...p.newCandidates] : s.liveCandidates,
    });
  },

  setFilters: (f) => set({ filters: { ...get().filters, ...f } }),
  setSort: (sort) => set({ sort }),

  toggleSelected: (id) => {
    const c = get().liveCandidates.find((x) => x.id === id);
    if (!c || !isSelectable(c)) return;
    const selected = new Set(get().selected);
    const acknowledged = new Set(get().acknowledged);
    if (selected.has(id)) {
      selected.delete(id);
      acknowledged.delete(id);
    } else selected.add(id);
    set({ selected, acknowledged });
  },

  selectBulk: (candidates, on) => {
    const selected = new Set(get().selected);
    const acknowledged = new Set(get().acknowledged);
    for (const c of candidates) {
      if (on) {
        if (c.bulkSelectable && isSelectable(c)) selected.add(c.id);
      } else {
        selected.delete(c.id);
        acknowledged.delete(c.id);
      }
    }
    set({ selected, acknowledged });
  },

  clearSelection: () => set({ selected: new Set(), acknowledged: new Set() }),

  toggleAcknowledged: (id) => {
    const acknowledged = new Set(get().acknowledged);
    if (acknowledged.has(id)) acknowledged.delete(id);
    else acknowledged.add(id);
    set({ acknowledged });
  },

  inspect: (inspectedId) => set({ inspectedId }),

  runCleanup: async () => {
    const { summary, selected, acknowledged, cleaning, scanning, summaryStale } = get();
    const ids = Array.from(selected);
    if (!summary || ids.length === 0 || cleaning || scanning || summaryStale) return;
    set({ cleaning: true, cleanError: null, cleanReport: null });
    try {
      const report = await api.cleanItems(
        summary.sessionId,
        ids,
        ids.filter((id) => acknowledged.has(id)),
      );
      // Only apply results to the session that owns them.
      if (get().summary?.sessionId !== report.sessionId) {
        set({ cleanReport: report });
        return;
      }
      const trashed = new Set(report.results.filter((r) => r.status === "trashed").map((r) => r.id));
      const bytes = report.results
        .filter((r) => r.status === "trashed")
        .reduce((sum, r) => sum + r.sizeBytes, 0);
      const cur = get().summary!;
      set({
        cleanReport: report,
        selected: new Set(ids.filter((id) => !trashed.has(id))),
        acknowledged: new Set([...get().acknowledged].filter((id) => !trashed.has(id))),
        liveCandidates: get().liveCandidates.filter((c) => !trashed.has(c.id)),
        summary: {
          ...cur,
          candidates: cur.candidates.filter((c) => !trashed.has(c.id)),
          totalBytes: cur.totalBytes - bytes,
        },
        history: pushHistory(get().history, {
          at: Date.now(),
          kind: "cleanup",
          items: trashed.size,
          bytes,
          failed: report.results.length - trashed.size,
        }),
      });
    } catch (e) {
      set({ cleanError: String(e) });
    } finally {
      set({ cleaning: false });
    }
  },

  resetCleanup: () => set({ cleanReport: null, cleanError: null }),

  clearHistory: () => {
    save(HISTORY_KEY, []);
    set({ history: [] });
  },
}));

async function applyExclusions(next: string[]) {
  const { summary, exclusions: before } = useSweepStore.getState();
  try {
    const exclusions = await api.setExclusions(next);
    const changed = JSON.stringify([...before].sort()) !== JSON.stringify([...exclusions].sort());
    useSweepStore.setState({
      exclusions,
      exclusionError: null,
      // The backend invalidated the previous scan; mirror that in the UI.
      summaryStale: useSweepStore.getState().summaryStale || (changed && summary != null),
    });
  } catch (e) {
    useSweepStore.setState({ exclusionError: String(e) });
  }
}

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
