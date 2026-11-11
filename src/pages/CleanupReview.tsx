import { useMemo, useState } from "react";
import { Button, Card, Empty, Notice, PageHeader, RiskBadge } from "../components/ui";
import { formatBytes, formatCount, formatSignedBytes } from "../lib/format";
import { useSweepStore } from "../stores/useSweepStore";
import type { CleanStatus } from "../types";

const STATUS_STYLE: Record<CleanStatus, string> = {
  trashed: "text-low",
  skipped: "text-review",
  failed: "text-caution",
};
const STATUS_LABEL: Record<CleanStatus, string> = {
  trashed: "Moved to Trash",
  skipped: "Skipped",
  failed: "Failed",
};

export function CleanupReview() {
  const candidates = useSweepStore((s) => s.liveCandidates);
  const selected = useSweepStore((s) => s.selected);
  const acknowledged = useSweepStore((s) => s.acknowledged);
  const toggleSelected = useSweepStore((s) => s.toggleSelected);
  const toggleAcknowledged = useSweepStore((s) => s.toggleAcknowledged);
  const cleaning = useSweepStore((s) => s.cleaning);
  const report = useSweepStore((s) => s.cleanReport);
  const error = useSweepStore((s) => s.cleanError);
  const runCleanup = useSweepStore((s) => s.runCleanup);
  const resetCleanup = useSweepStore((s) => s.resetCleanup);
  const setPage = useSweepStore((s) => s.setPage);
  const scanning = useSweepStore((s) => s.scanning);
  const stale = useSweepStore((s) => s.summaryStale);
  const [confirmed, setConfirmed] = useState(false);

  const items = useMemo(
    () => candidates.filter((c) => selected.has(c.id)).sort((a, b) => b.sizeBytes - a.sizeBytes),
    [candidates, selected],
  );
  const totalBytes = items.reduce((s, c) => s + c.sizeBytes, 0);
  const totalFiles = items.reduce((s, c) => s + c.fileCount, 0);
  const caution = items.filter((c) => c.risk === "caution");
  const unconfirmedCaution = caution.filter((c) => !acknowledged.has(c.id)).length;
  const anyIncomplete = items.some((c) => !c.sizeComplete);

  if (report) {
    const counts = { trashed: 0, skipped: 0, failed: 0 };
    let moved = 0;
    for (const r of report.results) {
      counts[r.status]++;
      if (r.status === "trashed") moved += r.sizeBytes;
    }
    return (
      <div>
        <PageHeader
          title="Cleanup results"
          subtitle={`${counts.trashed} moved to Trash, ${counts.skipped} skipped, ${counts.failed} failed.`}
          actions={
            <Button
              onClick={() => {
                resetCleanup();
                setConfirmed(false);
                setPage("scanner");
              }}
            >
              Back to results
            </Button>
          }
        />
        <div className="mb-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Card>
            <div className="text-xs uppercase tracking-wide text-muted">Estimated size moved to Trash</div>
            <div className="mt-1 text-2xl font-semibold tabular-nums">{formatBytes(moved)}</div>
            <div className="mt-1 text-xs text-muted">Logical size measured at scan time, not space freed.</div>
          </Card>
          <Card>
            <div className="text-xs uppercase tracking-wide text-muted">Free space (measured by Windows)</div>
            {report.freeSpace.length === 0 ? (
              <div className="mt-1 text-sm text-muted">Not measured on this platform.</div>
            ) : (
              <ul className="mt-1 space-y-1 text-sm">
                {report.freeSpace.map((v) => (
                  <li key={v.volume} className="tabular-nums">
                    <span className="font-medium">{v.volume}</span> {formatBytes(v.freeBefore)} →{" "}
                    {formatBytes(v.freeAfter)} ({formatSignedBytes(v.freeAfter - v.freeBefore)})
                  </li>
                ))}
              </ul>
            )}
            <div className="mt-1 text-xs text-muted">
              Items in the Recycle Bin still use disk space until it is emptied, and other programs change free space
              too.
            </div>
          </Card>
        </div>
        <Card className="p-0">
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-muted">
              <tr className="border-b border-border">
                <th className="px-3 py-2 font-medium">Status</th>
                <th className="py-2 font-medium">Path</th>
                <th className="py-2 font-medium">Details</th>
                <th className="px-3 py-2 text-right font-medium">Est. size</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {report.results.map((r) => (
                <tr key={r.id}>
                  <td className={`whitespace-nowrap px-3 py-2 font-medium ${STATUS_STYLE[r.status]}`}>
                    {STATUS_LABEL[r.status]}
                  </td>
                  <td className="max-w-0 py-2 pr-3">
                    <div className="truncate font-mono text-xs" title={r.path}>
                      {r.path || r.id}
                    </div>
                  </td>
                  <td className="py-2 pr-3 text-muted">{r.message}</td>
                  <td className="whitespace-nowrap px-3 py-2 text-right tabular-nums">{formatBytes(r.sizeBytes)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      </div>
    );
  }

  return (
    <div>
      <PageHeader
        title="Cleanup review"
        subtitle="Check exactly what will be moved to the Trash. Nothing happens until you confirm."
      />

      {items.length === 0 ? (
        <Card>
          <Empty>
            No items selected.{" "}
            <button className="text-accent underline" onClick={() => setPage("scanner")}>
              Go to the scanner
            </button>{" "}
            and tick the items you want to remove.
          </Empty>
        </Card>
      ) : (
        <div className="space-y-4">
          {stale && (
            <Notice tone="error">
              Exclusions changed after this scan, so these results can no longer be cleaned. Scan again.
            </Notice>
          )}
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <Card>
              <div className="text-xs uppercase tracking-wide text-muted">Selected</div>
              <div className="mt-1 text-2xl font-semibold tabular-nums">{items.length}</div>
            </Card>
            <Card>
              <div className="text-xs uppercase tracking-wide text-muted">Estimated size</div>
              <div className="mt-1 text-2xl font-semibold tabular-nums">
                {anyIncomplete ? "≥ " : ""}
                {formatBytes(totalBytes)}
              </div>
              <div className="text-xs text-muted">Logical size at scan time</div>
            </Card>
            <Card>
              <div className="text-xs uppercase tracking-wide text-muted">Operation</div>
              <div className="mt-1 font-semibold">Move to Recycle Bin / Trash</div>
              <div className="text-xs text-muted">{formatCount(totalFiles)} files · no permanent delete</div>
            </Card>
          </div>

          {caution.length > 0 && (
            <Notice tone="warn">
              {caution.length} selected item{caution.length > 1 ? "s are" : " is"} marked <strong>Caution</strong>:
              the name is ambiguous or the folder may hold the only copy of some work (e.g. notebook checkpoints).
              Confirm each one below after inspecting it; unconfirmed Caution items are skipped.
            </Notice>
          )}

          <Card className="p-0">
            <table className="w-full text-sm">
              <thead className="text-left text-xs text-muted">
                <tr className="border-b border-border">
                  <th className="w-8 px-3 py-2"></th>
                  <th className="py-2 font-medium">Item</th>
                  <th className="py-2 font-medium">Risk</th>
                  <th className="px-3 py-2 text-right font-medium">Est. size</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {items.map((c) => (
                  <tr key={c.id}>
                    <td className="px-3 py-2 align-top">
                      <input
                        type="checkbox"
                        checked
                        disabled={cleaning}
                        onChange={() => toggleSelected(c.id)}
                        aria-label={`Keep ${c.path} selected`}
                      />
                    </td>
                    <td className="max-w-0 py-2 pr-3">
                      <div className="font-medium">{c.displayName}</div>
                      <div className="truncate font-mono text-xs text-muted" title={c.path}>
                        {c.path}
                      </div>
                      {c.risk === "caution" && (
                        <label className="mt-1.5 flex items-start gap-2 text-xs">
                          <input
                            type="checkbox"
                            className="mt-0.5"
                            checked={acknowledged.has(c.id)}
                            disabled={cleaning}
                            onChange={() => toggleAcknowledged(c.id)}
                          />
                          <span>I opened this folder and confirm it contains nothing I need.</span>
                        </label>
                      )}
                    </td>
                    <td className="py-2 align-top">
                      <RiskBadge risk={c.risk} />
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-right align-top tabular-nums">
                      {c.sizeComplete ? "" : "≥ "}
                      {formatBytes(c.sizeBytes)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>

          <Card className="space-y-3">
            <Notice>
              Before each item is touched, DevSweep re-checks that it still exists, is not a link, is the same folder
              that was scanned, sits inside your scan root, does not contain or sit inside an excluded folder, and still
              matches its project rule. Anything that changed is skipped. Recreating dependencies or build output may
              later need network access, time and a working toolchain.
            </Notice>
            <label className="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
                disabled={cleaning || stale}
              />
              <span>
                I have reviewed these {items.length} items and want to move them (about {formatBytes(totalBytes)}) to
                the Trash.
              </span>
            </label>
            {unconfirmedCaution > 0 && (
              <p className="text-xs text-review">
                {unconfirmedCaution} Caution item{unconfirmedCaution > 1 ? "s are" : " is"} not confirmed and will be
                skipped.
              </p>
            )}
            {error && <Notice tone="error">{error}</Notice>}
            <div className="flex gap-2">
              <Button
                variant="danger"
                disabled={!confirmed || cleaning || scanning || stale}
                onClick={async () => {
                  await runCleanup();
                  setConfirmed(false);
                }}
              >
                {cleaning ? "Moving to Trash…" : `Move ${items.length} items to Trash`}
              </Button>
              <Button variant="ghost" onClick={() => setPage("scanner")} disabled={cleaning}>
                Back
              </Button>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}
