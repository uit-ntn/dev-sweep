import { create } from "zustand";
import * as api from "../lib/api";
import type {
  AnalysisProgress,
  AnalyzeResult,
  Comparison,
  DirView,
  FileInfo,
  RevealTarget,
  SnapshotMeta,
  VolumeInfo,
} from "../types";

export type DiskTab = "browse" | "history";

interface DiskState {
  tab: DiskTab;
  volumes: VolumeInfo[];
  volumesLoaded: boolean;
  selectedRoot: string | null;

  analyzing: boolean;
  progress: AnalysisProgress | null;
  result: AnalyzeResult | null;
  view: DirView | null;
  files: FileInfo[];
  error: string | null;

  snapshots: SnapshotMeta[];
  compareIds: string[];
  comparison: Comparison | null;
  compareError: string | null;

  setTab(t: DiskTab): void;
  loadVolumes(): Promise<void>;
  selectVolume(root: string): void;
  analyze(): Promise<void>;
  cancel(): Promise<void>;
  handleProgress(p: AnalysisProgress): void;
  openFolder(nodeId: number): Promise<void>;
  reveal(target: RevealTarget): Promise<void>;

  loadSnapshots(): Promise<void>;
  toggleCompare(id: string): void;
  compare(): Promise<void>;
  deleteSnapshot(id: string): Promise<void>;
  clearSnapshots(): Promise<void>;
}

export const useDiskStore = create<DiskState>((set, get) => ({
  tab: "browse",
  volumes: [],
  volumesLoaded: false,
  selectedRoot: null,

  analyzing: false,
  progress: null,
  result: null,
  view: null,
  files: [],
  error: null,

  snapshots: [],
  compareIds: [],
  comparison: null,
  compareError: null,

  setTab: (tab) => set({ tab }),

  loadVolumes: async () => {
    try {
      const volumes = await api.listVolumes();
      set({
        volumes,
        volumesLoaded: true,
        selectedRoot: get().selectedRoot ?? volumes[0]?.root ?? null,
      });
    } catch (e) {
      set({ error: String(e), volumesLoaded: true });
    }
  },

  selectVolume: (root) => {
    if (!get().analyzing) set({ selectedRoot: root });
  },

  analyze: async () => {
    const root = get().selectedRoot;
    if (!root || get().analyzing) return;
    set({ analyzing: true, progress: null, result: null, view: null, files: [], error: null });
    try {
      const result = await api.analyzeVolume(root);
      set({ result });
      await get().openFolder(0);
      void get().loadSnapshots();
      void get().loadVolumes(); // refresh free space
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ analyzing: false });
    }
  },

  cancel: async () => {
    await api.cancelAnalysis();
  },

  handleProgress: (p) => {
    if (get().analyzing) set({ progress: p });
  },

  openFolder: async (nodeId) => {
    const result = get().result;
    if (!result) return;
    const id = result.summary.analysisId;
    try {
      const [view, files] = await Promise.all([api.analysisDir(id, nodeId, 100), api.analysisTopFiles(id, nodeId, 50)]);
      set({ view, files, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  reveal: async (target) => {
    const result = get().result;
    if (!result) return;
    try {
      await api.revealInExplorer(result.summary.analysisId, target);
    } catch (e) {
      set({ error: `Could not open in Explorer: ${String(e)}` });
    }
  },

  loadSnapshots: async () => {
    try {
      const snapshots = await api.listSnapshots();
      const known = new Set(snapshots.map((s) => s.id));
      set({ snapshots, compareIds: get().compareIds.filter((id) => known.has(id)) });
    } catch (e) {
      set({ compareError: String(e) });
    }
  },

  toggleCompare: (id) => {
    const { compareIds, snapshots } = get();
    if (compareIds.includes(id)) {
      set({ compareIds: compareIds.filter((x) => x !== id), comparison: null });
      return;
    }
    const snap = snapshots.find((s) => s.id === id);
    const first = snapshots.find((s) => s.id === compareIds[0]);
    // Only snapshots of the same volume can be paired.
    if (snap && first && (first.volume.serial !== snap.volume.serial || first.volume.root !== snap.volume.root)) {
      set({ compareIds: [id], comparison: null, compareError: null });
      return;
    }
    set({ compareIds: [...compareIds, id].slice(-2), comparison: null, compareError: null });
  },

  compare: async () => {
    const [a, b] = get().compareIds;
    if (!a || !b) return;
    try {
      set({ comparison: await api.compareSnapshots(a, b, 50), compareError: null });
    } catch (e) {
      set({ comparison: null, compareError: String(e) });
    }
  },

  deleteSnapshot: async (id) => {
    try {
      await api.deleteSnapshot(id);
      set({ comparison: null });
      await get().loadSnapshots();
    } catch (e) {
      set({ compareError: String(e) });
    }
  },

  clearSnapshots: async () => {
    try {
      await api.clearSnapshots();
      set({ comparison: null, compareIds: [] });
      await get().loadSnapshots();
    } catch (e) {
      set({ compareError: String(e) });
    }
  },
}));
