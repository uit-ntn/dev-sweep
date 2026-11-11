import { useEffect, useState } from "react";
import { Button, Card, Empty, Notice, PageHeader } from "../components/ui";
import { inTauri, onAnalysisProgress } from "../lib/api";
import { formatBytes, formatCount, formatDate, formatDuration, formatInterval, formatSignedBytes } from "../lib/format";
import { useDiskStore } from "../stores/useDiskStore";
import { useSweepStore } from "../stores/useSweepStore";
import type { Comparison, DirDelta, NodeInfo, SnapshotMeta, VolumeInfo } from "../types";

/** Read-only disk usage view. There is deliberately no delete action here. */
export function DiskAnalyzer() {
  const tab = useDiskStore((s) => s.tab);
  const setTab = useDiskStore((s) => s.setTab);
  const loadVolumes = useDiskStore((s) => s.loadVolumes);
  const loadSnapshots = useDiskStore((s) => s.loadSnapshots);
  const handleProgress = useDiskStore((s) => s.handleProgress);

  useEffect(() => {
    void loadVolumes();
    void loadSnapshots();
    let unlisten: (() => void) | undefined;
    let disposed = false;
    onAnalysisProgress(handleProgress).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadVolumes, loadSnapshots, handleProgress]);

  return (
    <div>
      <PageHeader
        title="Disk Analyzer"
        subtitle="See what uses space on a drive. Read-only: nothing here can delete files."
      />
      {!inTauri() && <Notice tone="warn">Running in a browser preview. Open the desktop app to analyze drives.</Notice>}
      <div className="mb-4 flex gap-1 border-b border-border" role="tablist">
        {(
          [
            ["browse", "Analyze & browse"],
            ["history", "Snapshots & growth"],
          ] as const
        ).map(([key, label]) => (
          <button
            key={key}
            role="tab"
            aria-selected={tab === key}
            onClick={() => setTab(key)}
            className={`-mb-px border-b-2 px-3 py-2 text-sm ${
              tab === key ? "border-accent font-medium text-text" : "border-transparent text-muted hover:text-text"
            }`}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === "browse" ? <Browse /> : <History />}
    </div>
  );
}

// ---------------------------------------------------------------------------

function UsageBar({ used, total }: { used: number; total: number }) {
  const pct = total > 0 ? Math.min(100, (used / total) * 100) : 0;
  return (
    <div className="h-2 overflow-hidden rounded bg-surface-2" aria-hidden>
      <div className={`h-full rounded ${pct > 90 ? "bg-caution" : "bg-accent"}`} style={{ width: `${pct}%` }} />
    </div>
  );
}

function VolumeCard({ v, selected, onSelect, disabled }: { v: VolumeInfo; selected: boolean; onSelect: () => void; disabled: boolean }) {
  return (
    <button
      onClick={onSelect}
      disabled={disabled}
      aria-pressed={selected}
      className={`rounded-lg border p-3 text-left transition disabled:cursor-not-allowed ${
        selected ? "border-accent bg-accent-soft" : "border-border bg-surface hover:bg-surface-2"
      }`}
    >
      <div className="flex items-baseline justify-between gap-2">
        <span className="font-semibold">
          {v.root} {v.label && <span className="font-normal text-muted">{v.label}</span>}
        </span>
        <span className="text-xs text-muted">
          {v.fileSystem}
          {v.removable ? " · removable" : ""}
        </span>
      </div>
      <div className="my-2">
        <UsageBar used={v.totalBytes - v.freeBytes} total={v.totalBytes} />
      </div>
      <div className="text-xs tabular-nums text-muted">
        {formatBytes(v.freeBytes)} free of {formatBytes(v.totalBytes)}
      </div>
    </button>
  );
}

function Browse() {
  const d = useDiskStore();
  const addRoots = useSweepStore((s) => s.addRoots);
  const setPage = useSweepStore((s) => s.setPage);
  const [handoffError, setHandoffError] = useState<string | null>(null);

  async function scanForCleanable(n: NodeInfo) {
    setHandoffError(null);
    // Goes through the normal workspace flow: root validation (drive roots,
    // home and system folders are refused), exclusions, review, confirmation.
    const errors = await addRoots([n.path]);
    if (errors.length) setHandoffError(errors.join("\n"));
    else setPage("dashboard");
  }

  const summary = d.result?.summary;
  const view = d.view;
  const maxChild = view?.children[0]?.bytes ?? 0;

  return (
    <div className="space-y-4">
      <Card>
        <h2 className="mb-3 font-semibold">Local drives</h2>
        {d.volumesLoaded && d.volumes.length === 0 ? (
          <Empty>No local drives found (the Disk Analyzer currently supports Windows).</Empty>
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {d.volumes.map((v) => (
              <VolumeCard
                key={v.root}
                v={v}
                selected={d.selectedRoot === v.root}
                disabled={d.analyzing}
                onSelect={() => d.selectVolume(v.root)}
              />
            ))}
          </div>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {d.analyzing ? (
            <Button variant="danger" onClick={d.cancel}>
              Cancel analysis
            </Button>
          ) : (
            <Button variant="primary" onClick={d.analyze} disabled={!d.selectedRoot || !inTauri()}>
              Analyze {d.selectedRoot ?? "drive"}
            </Button>
          )}
          <span className="text-xs text-muted">
            Uses your current permissions; never asks for administrator rights. Large drives can take several minutes.
          </span>
        </div>
      </Card>

      {d.analyzing && (
        <Card>
          <div className="mb-2 h-1 overflow-hidden rounded bg-surface-2">
            <div className="h-full w-1/3 animate-pulse rounded bg-accent" />
          </div>
          <div className="flex flex-wrap gap-x-6 gap-y-1 text-sm tabular-nums">
            <span>{formatCount(d.progress?.dirsRead ?? 0)} folders</span>
            <span>{formatCount(d.progress?.filesSeen ?? 0)} files</span>
            <span>{formatBytes(d.progress?.bytesSeen ?? 0)}</span>
            {(d.progress?.dirsUnreadable ?? 0) > 0 && (
              <span className="text-review">{formatCount(d.progress!.dirsUnreadable)} unreadable</span>
            )}
          </div>
          <div className="mt-1 truncate font-mono text-xs text-muted">{d.progress?.currentPath}</div>
        </Card>
      )}

      {d.error && <Notice tone="error">{d.error}</Notice>}
      {handoffError && <Notice tone="error">Cannot scan this folder for cleanable items: {handoffError}</Notice>}

      {summary && (
        <Notice tone={summary.incomplete ? "warn" : "info"}>
          {summary.cancelled && "Analysis cancelled — sizes below are partial and no snapshot was saved. "}
          Read {formatCount(summary.dirsRead)} folders and {formatCount(summary.files)} files (
          {formatBytes(summary.totalBytes)} logical size) in {formatDuration(summary.finishedAtMs - summary.startedAtMs)}.
          {summary.dirsUnreadable > 0 &&
            ` ${formatCount(summary.dirsUnreadable)} folders could not be read with your permissions, so totals are lower bounds.`}
          {summary.linksSkipped > 0 && ` ${formatCount(summary.linksSkipped)} links/junctions were not followed.`}
          {summary.truncated && " The folder limit was reached; very deep trees are summarized."}
          {d.result?.snapshot && " Snapshot saved."}
          {d.result?.snapshotError && ` Snapshot not saved: ${d.result.snapshotError}.`}
        </Notice>
      )}

      {view && (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
          <Card className="min-w-0">
            <nav aria-label="Breadcrumb" className="mb-3 flex flex-wrap items-center gap-1 text-sm">
              {view.breadcrumb.map((c, i) => (
                <span key={c.id} className="flex items-center gap-1">
                  {i > 0 && <span className="text-muted">›</span>}
                  {i === view.breadcrumb.length - 1 ? (
                    <span className="font-medium">{c.name}</span>
                  ) : (
                    <button className="text-accent hover:underline" onClick={() => d.openFolder(c.id)}>
                      {c.name}
                    </button>
                  )}
                </span>
              ))}
            </nav>
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2 text-sm">
              <span className="tabular-nums">
                <strong>{formatBytes(view.node.bytes)}</strong> in {formatCount(view.node.files)} files
                {view.node.incomplete && <span className="text-review"> · incomplete</span>}
              </span>
              <span className="flex gap-2">
                <Button onClick={() => d.reveal({ kind: "folder", id: view.node.id })}>Open in Explorer</Button>
                {view.node.id !== 0 && <Button onClick={() => scanForCleanable(view.node)}>Scan for cleanable items</Button>}
              </span>
            </div>
            {view.children.length === 0 ? (
              <Empty>No sub-folders.</Empty>
            ) : (
              <ul className="divide-y divide-border">
                {view.children.map((c) => (
                  <li key={c.id} className="py-2">
                    <div className="flex items-center gap-2">
                      {c.hasChildren ? (
                        <button className="min-w-0 flex-1 truncate text-left font-medium text-accent hover:underline" title={c.path} onClick={() => d.openFolder(c.id)}>
                          {c.name}
                        </button>
                      ) : (
                        <span className="min-w-0 flex-1 truncate font-medium" title={c.path}>
                          {c.name}
                        </span>
                      )}
                      {c.unreadable && <span className="text-xs text-review">unreadable</span>}
                      {!c.unreadable && c.incomplete && <span className="text-xs text-review">incomplete</span>}
                      <span className="w-24 text-right text-sm tabular-nums">{formatBytes(c.bytes)}</span>
                      <button
                        className="text-xs text-muted hover:text-text"
                        onClick={() => d.reveal({ kind: "folder", id: c.id })}
                        aria-label={`Open ${c.name} in Explorer`}
                        title="Open in Explorer"
                      >
                        Open
                      </button>
                    </div>
                    <div className="mt-1">
                      <UsageBar used={c.bytes} total={maxChild} />
                    </div>
                  </li>
                ))}
              </ul>
            )}
            {(view.moreChildren > 0 || view.ownFilesBytes > 0) && (
              <p className="mt-3 text-xs text-muted">
                {view.moreChildren > 0 &&
                  `${formatCount(view.moreChildren)} smaller folders (${formatBytes(view.moreChildrenBytes)}) not shown. `}
                {view.ownFilesBytes > 0 && `Files directly in this folder: ${formatBytes(view.ownFilesBytes)}.`}
              </p>
            )}
          </Card>

          <Card className="min-w-0">
            <h2 className="mb-1 font-semibold">Largest files here</h2>
            <p className="mb-3 text-xs text-muted">From the 1,000 largest files on the drive.</p>
            {d.files.length === 0 ? (
              <Empty>None of the drive's largest files are in this folder.</Empty>
            ) : (
              <ul className="divide-y divide-border">
                {d.files.map((f) => (
                  <li key={f.id} className="flex items-center gap-2 py-1.5">
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm" title={f.path}>
                        {f.name}
                      </div>
                      <div className="truncate font-mono text-xs text-muted" title={f.path}>
                        {f.path}
                      </div>
                    </div>
                    <span className="text-sm tabular-nums">{formatBytes(f.bytes)}</span>
                    <button
                      className="text-xs text-muted hover:text-text"
                      onClick={() => d.reveal({ kind: "file", id: f.id })}
                      aria-label={`Show ${f.name} in Explorer`}
                    >
                      Show
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </Card>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------

function History() {
  const d = useDiskStore();
  const [confirmClear, setConfirmClear] = useState(false);
  const first = d.snapshots.find((s) => s.id === d.compareIds[0]);

  return (
    <div className="space-y-4">
      <Card>
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 className="font-semibold">Saved snapshots</h2>
            <p className="text-xs text-muted">
              Saved on this device after each completed analysis (newest 30 kept). Pick two of the same drive to
              compare.
            </p>
          </div>
          <div className="flex gap-2">
            <Button variant="primary" disabled={d.compareIds.length !== 2} onClick={d.compare}>
              Compare selected
            </Button>
            <Button
              variant={confirmClear ? "danger" : "secondary"}
              disabled={d.snapshots.length === 0}
              onClick={async () => {
                if (!confirmClear) return setConfirmClear(true);
                setConfirmClear(false);
                await d.clearSnapshots();
              }}
              onBlur={() => setConfirmClear(false)}
            >
              {confirmClear ? "Click again to delete all snapshots" : "Clear snapshot history"}
            </Button>
          </div>
        </div>
        {d.snapshots.length === 0 ? (
          <Empty>No snapshots yet. Analyze a drive to save one.</Empty>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-muted">
              <tr className="border-b border-border">
                <th className="w-8 py-2"></th>
                <th className="py-2 font-medium">Drive</th>
                <th className="py-2 font-medium">Taken</th>
                <th className="py-2 text-right font-medium">Free space</th>
                <th className="py-2 text-right font-medium">Readable data</th>
                <th className="py-2 text-right font-medium">Unreadable</th>
                <th className="py-2"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {d.snapshots.map((s) => {
                const otherVolume = first != null && !sameVolume(first, s);
                return (
                  <tr key={s.id} className={otherVolume ? "opacity-60" : ""}>
                    <td className="py-1.5">
                      <input
                        type="checkbox"
                        checked={d.compareIds.includes(s.id)}
                        onChange={() => d.toggleCompare(s.id)}
                        aria-label={`Select snapshot from ${formatDate(s.takenAtMs)}`}
                        title={otherVolume ? "Different drive — selecting starts a new pair" : undefined}
                      />
                    </td>
                    <td className="py-1.5">
                      {s.volume.root} <span className="text-muted">{s.volume.label}</span>
                    </td>
                    <td className="py-1.5">{formatDate(s.takenAtMs)}</td>
                    <td className="py-1.5 text-right tabular-nums">{formatBytes(s.freeBytes)}</td>
                    <td className="py-1.5 text-right tabular-nums">{formatBytes(s.logicalBytes)}</td>
                    <td className="py-1.5 text-right tabular-nums">
                      {s.dirsUnreadable > 0 ? <span className="text-review">{formatCount(s.dirsUnreadable)} folders</span> : "—"}
                    </td>
                    <td className="py-1.5 text-right">
                      <button className="text-xs text-muted hover:text-caution" onClick={() => d.deleteSnapshot(s.id)}>
                        Delete
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </Card>
      {d.compareError && <Notice tone="error">{d.compareError}</Notice>}
      {d.comparison && <ComparisonView c={d.comparison} />}
    </div>
  );
}

function sameVolume(a: SnapshotMeta, b: SnapshotMeta) {
  return a.volume.serial === b.volume.serial && a.volume.root === b.volume.root;
}

function ComparisonView({ c }: { c: Comparison }) {
  const used = -c.freeDelta;
  const interval = formatInterval(c.intervalMs);
  return (
    <div className="space-y-4">
      <Card>
        <h2 className="mb-1 font-semibold">
          {c.older.volume.root} · {formatDate(c.older.takenAtMs)} → {formatDate(c.newer.takenAtMs)}
        </h2>
        <p className="mb-3 text-xs text-muted">Actual time between scans: {interval}.</p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div>
            <div className="text-xs uppercase tracking-wide text-muted">Used space on the drive (measured by Windows)</div>
            <div className="mt-1 text-xl font-semibold tabular-nums">
              {used >= 0 ? `Grew ${formatBytes(used)}` : `Shrank ${formatBytes(-used)}`} in {interval}
            </div>
            <div className="text-xs text-muted">
              Free space {formatBytes(c.older.freeBytes)} → {formatBytes(c.newer.freeBytes)}. Includes everything on the
              drive: other users, system files, Recycle Bin, restore points.
            </div>
          </div>
          <div>
            <div className="text-xs uppercase tracking-wide text-muted">Readable folders (logical size)</div>
            <div className="mt-1 text-xl font-semibold tabular-nums">{formatSignedBytes(c.logicalDelta)}</div>
            <div className="text-xs text-muted">
              Sum of file sizes this account could read. Differs from used space because of compression, cluster slack,
              hard links and unreadable areas.
            </div>
          </div>
        </div>
      </Card>
      {c.partial && (
        <Notice tone="warn">
          <strong>Partial comparison.</strong> {c.partialReasons.join("; ")}.
          {c.unreadableSkipped > 0 &&
            ` ${formatCount(c.unreadableSkipped)} folders inside unreadable areas were left out — they are not counted as added or deleted.`}
        </Notice>
      )}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <DeltaList title="Folders that grew the most" root={c.older.volume.root} rows={c.growers} />
        <DeltaList title="Folders that shrank the most" root={c.older.volume.root} rows={c.shrinkers} />
      </div>
    </div>
  );
}

function DeltaList({ title, root, rows }: { title: string; root: string; rows: DirDelta[] }) {
  return (
    <Card className="min-w-0">
      <h2 className="mb-3 font-semibold">{title}</h2>
      {rows.length === 0 ? (
        <Empty>No significant changes.</Empty>
      ) : (
        <ul className="divide-y divide-border">
          {rows.map((r) => (
            <li key={r.path} className="flex items-center gap-2 py-1.5 text-sm">
              <span className="min-w-0 flex-1 truncate font-mono text-xs" title={root + r.path}>
                {root + r.path}
              </span>
              {r.status !== "changed" && (
                <span className="text-xs text-muted">{r.status === "added" ? "new" : "gone"}</span>
              )}
              <span
                className={`w-28 text-right tabular-nums ${r.delta > 0 ? "text-review" : "text-low"}`}
                title={r.approximate ? "Approximate: the folder may have existed below the snapshot size threshold, or readability changed" : undefined}
              >
                {r.approximate ? "≈ " : ""}
                {formatSignedBytes(r.delta)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
