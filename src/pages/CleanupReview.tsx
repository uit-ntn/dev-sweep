import { useMemo, useState } from "react";
import { Button, Card, Empty, Notice, PageHeader, RiskBadge } from "../components/ui";
import { formatBytes, formatCount } from "../lib/format";
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
  const toggleSelected = useSweepStore((s) => s.toggleSelected);
  const cleaning = useSweepStore((s) => s.cleaning);
  const results = useSweepStore((s) => s.cleanResults);
  const error = useSweepStore((s) => s.cleanError);
  const runCleanup = useSweepStore((s) => s.runCleanup);
  const resetCleanup = useSweepStore((s) => s.resetCleanup);
  const setPage = useSweepStore((s) => s.setPage);
  const scanning = useSweepStore((s) => s.scanning);
  const [confirmed, setConfirmed] = useState(false);

  const items = useMemo(
    () => candidates.filter((c) => selected.has(c.id)).sort((a, b) => b.sizeBytes - a.sizeBytes),
    [candidates, selected],
  );
  const totalBytes = items.reduce((s, c) => s + c.sizeBytes, 0);
  const totalFiles = items.reduce((s, c) => s + c.fileCount, 0);
  const cautionCount = items.filter((c) => c.risk === "caution").length;

  if (results) {
    const counts = { trashed: 0, skipped: 0, failed: 0 };
    let freed = 0;
    for (const r of results) {
      counts[r.status]++;
      if (r.status === "trashed") freed += r.sizeBytes;
    }
    return (
      <div>
        <PageHeader
          title="Cleanup results"
          subtitle={`${counts.trashed} moved to Trash (${formatBytes(freed)}), ${counts.skipped} skipped, ${counts.failed} failed.`}
          actions={
            <>
              <Button onClick={() => { resetCleanup(); setConfirmed(false); setPage("scanner"); }}>Back to results</Button>
            </>
          }
        />
        <Notice>
          Items were moved to the Recycle Bin / Trash. Restoring them is usually possible from there, but not
          guaranteed (e.g. on some removable or network drives). Freed space appears once the Trash is emptied.
        </Notice>
        <Card className="mt-4 p-0">
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-muted">
              <tr className="border-b border-border">
                <th className="px-3 py-2 font-medium">Status</th>
                <th className="py-2 font-medium">Path</th>
                <th className="py-2 font-medium">Details</th>
                <th className="px-3 py-2 text-right font-medium">Size</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {results.map((r) => (
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
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <Card>
              <div className="text-xs uppercase tracking-wide text-muted">Selected</div>
              <div className="mt-1 text-2xl font-semibold tabular-nums">{items.length}</div>
            </Card>
            <Card>
              <div className="text-xs uppercase tracking-wide text-muted">Estimated space</div>
              <div className="mt-1 text-2xl font-semibold tabular-nums">{formatBytes(totalBytes)}</div>
            </Card>
            <Card>
              <div className="text-xs uppercase tracking-wide text-muted">Operation</div>
              <div className="mt-1 font-semibold">Move to Recycle Bin / Trash</div>
              <div className="text-xs text-muted">{formatCount(totalFiles)} files · no permanent delete</div>
            </Card>
          </div>

          {cautionCount > 0 && (
            <Notice tone="warn">
              {cautionCount} selected item{cautionCount > 1 ? "s are" : " is"} marked <strong>Caution</strong>: the
              name is ambiguous or the folder may contain unique work (e.g. notebook checkpoints). Make sure you have
              inspected {cautionCount > 1 ? "them" : "it"}.
            </Notice>
          )}

          <Card className="p-0">
            <table className="w-full text-sm">
              <thead className="text-left text-xs text-muted">
                <tr className="border-b border-border">
                  <th className="w-8 px-3 py-2"></th>
                  <th className="py-2 font-medium">Item</th>
                  <th className="py-2 font-medium">Risk</th>
                  <th className="px-3 py-2 text-right font-medium">Size</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {items.map((c) => (
                  <tr key={c.id}>
                    <td className="px-3 py-2">
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
                    </td>
                    <td className="py-2">
                      <RiskBadge risk={c.risk} />
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-right tabular-nums">{formatBytes(c.sizeBytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>

          <Card className="space-y-3">
            <Notice>
              Before each item is touched, DevSweep re-checks that it still exists, is not a link, is the same folder
              that was scanned, sits inside your scan root and still matches its project rule. Anything that changed is
              skipped. Recreating dependencies or build output may later need network access, time and a working
              toolchain.
            </Notice>
            <label className="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
                disabled={cleaning}
              />
              <span>
                I have reviewed these {items.length} items and want to move them ({formatBytes(totalBytes)}) to the
                Trash.
              </span>
            </label>
            {error && <Notice tone="error">{error}</Notice>}
            <div className="flex gap-2">
              <Button
                variant="danger"
                disabled={!confirmed || cleaning || scanning}
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
