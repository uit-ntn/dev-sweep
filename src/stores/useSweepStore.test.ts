import { beforeEach, describe, expect, it, vi } from "vitest";

// The store talks to Tauri; nothing in these tests should reach it.
vi.mock("../lib/api", () => ({
  inTauri: () => false,
  listRules: vi.fn(async () => []),
  getExclusions: vi.fn(async () => []),
  setExclusions: vi.fn(async (p: string[]) => p),
  validateRoot: vi.fn(async (p: string) => p),
  startScan: vi.fn(),
  cancelScan: vi.fn(),
  cleanItems: vi.fn(),
}));

import * as api from "../lib/api";
import type { Candidate, ScanSummary } from "../types";
import { useSweepStore } from "./useSweepStore";

function cand(id: string, over: Partial<Candidate> = {}): Candidate {
  return {
    id,
    path: `/p/${id}`,
    name: id,
    projectDir: "/p",
    root: "/p",
    ruleId: "node.dependencies",
    displayName: "Node.js dependencies",
    ecosystem: "node",
    category: "dependencies",
    risk: "review",
    sizeBytes: 100,
    fileCount: 1,
    unreadable: 0,
    sizeComplete: true,
    lastModified: null,
    bulkSelectable: true,
    blockedReason: null,
    ...over,
  };
}

const items = [
  cand("s1:c1"),
  cand("s1:c2", { risk: "low" }),
  cand("s1:c3", { risk: "caution", ruleId: "jupyter.checkpoints", bulkSelectable: false }),
  cand("s1:c4", { risk: "caution", ruleId: "js.build-output", bulkSelectable: false }),
  cand("s1:c5", { blockedReason: "contains excluded folder /p/keep", bulkSelectable: false }),
];

const summary: ScanSummary = {
  sessionId: "s1",
  roots: ["/p"],
  exclusions: [],
  candidates: items,
  dirsVisited: 1,
  errors: 0,
  totalBytes: 400,
  cancelled: false,
  incomplete: false,
  elapsedMs: 1,
};

beforeEach(() => {
  useSweepStore.setState({
    liveCandidates: items,
    summary,
    summaryStale: false,
    selected: new Set(),
    acknowledged: new Set(),
    scanning: false,
    cleaning: false,
  });
});

describe("select all visible", () => {
  it("never selects Caution or blocked items", () => {
    useSweepStore.getState().selectBulk(items, true);
    expect([...useSweepStore.getState().selected].sort()).toEqual(["s1:c1", "s1:c2"]);
  });

  it("deselecting in bulk also clears individually picked Caution items", () => {
    const s = useSweepStore.getState();
    s.toggleSelected("s1:c3");
    s.toggleAcknowledged("s1:c3");
    useSweepStore.getState().selectBulk(items, false);
    expect(useSweepStore.getState().selected.size).toBe(0);
    expect(useSweepStore.getState().acknowledged.size).toBe(0);
  });
});

describe("individual selection", () => {
  it("allows Caution items one by one", () => {
    useSweepStore.getState().toggleSelected("s1:c3");
    expect(useSweepStore.getState().selected.has("s1:c3")).toBe(true);
  });

  it("refuses blocked items", () => {
    useSweepStore.getState().toggleSelected("s1:c5");
    expect(useSweepStore.getState().selected.has("s1:c5")).toBe(false);
  });
});

describe("cleanup request", () => {
  it("sends the session id and only acknowledged Caution ids", async () => {
    const clean = vi.mocked(api.cleanItems);
    clean.mockResolvedValue({ sessionId: "s1", results: [], freeSpace: [] });
    const s = useSweepStore.getState();
    s.toggleSelected("s1:c1");
    s.toggleSelected("s1:c3");
    s.toggleSelected("s1:c4");
    s.toggleAcknowledged("s1:c3");
    await useSweepStore.getState().runCleanup();
    expect(clean).toHaveBeenCalledWith("s1", ["s1:c1", "s1:c3", "s1:c4"], ["s1:c3"]);
  });

  it("is blocked when exclusions changed after the scan", async () => {
    const clean = vi.mocked(api.cleanItems);
    clean.mockClear();
    useSweepStore.setState({ summaryStale: true, selected: new Set(["s1:c1"]) });
    await useSweepStore.getState().runCleanup();
    expect(clean).not.toHaveBeenCalled();
  });

  it("ignores a report for a different session", async () => {
    const clean = vi.mocked(api.cleanItems);
    clean.mockResolvedValue({
      sessionId: "s0",
      results: [{ id: "s1:c1", path: "", status: "trashed", message: "", sizeBytes: 100 }],
      freeSpace: [],
    });
    useSweepStore.setState({ selected: new Set(["s1:c1"]) });
    await useSweepStore.getState().runCleanup();
    expect(useSweepStore.getState().liveCandidates).toHaveLength(items.length);
  });
});
