import { useMemo } from "react";
import { RootsEditor } from "../components/RootsEditor";
import { Button, Card, Empty, PageHeader, Stat } from "../components/ui";
import { formatBytes, formatDate, formatDuration } from "../lib/format";
import { useSweepStore } from "../stores/useSweepStore";

const ECOSYSTEM_LABEL: Record<string, string> = {
  node: "JavaScript / Node",
  python: "Python",
  rust: "Rust",
  java: "Java",
};

export function Dashboard() {
  const summary = useSweepStore((s) => s.summary);
  const candidates = useSweepStore((s) => s.liveCandidates);
  const roots = useSweepStore((s) => s.roots);
  const scanning = useSweepStore((s) => s.scanning);
  const history = useSweepStore((s) => s.history);
  const startScan = useSweepStore((s) => s.startScan);
  const setPage = useSweepStore((s) => s.setPage);

  const total = candidates.reduce((sum, c) => sum + c.sizeBytes, 0);
  const lastScan = history.find((h) => h.kind === "scan");
  const reclaimed = history.filter((h) => h.kind === "cleanup").reduce((s, h) => s + h.bytes, 0);

  const byEcosystem = useMemo(() => {
    const m = new Map<string, { bytes: number; count: number }>();
    for (const c of candidates) {
      const cur = m.get(c.ecosystem) ?? { bytes: 0, count: 0 };
      cur.bytes += c.sizeBytes;
      cur.count += 1;
      m.set(c.ecosystem, cur);
    }
    return [...m.entries()].sort((a, b) => b[1].bytes - a[1].bytes);
  }, [candidates]);
  const maxBytes = byEcosystem[0]?.[1].bytes ?? 0;

  async function scan() {
    setPage("scanner");
    await startScan();
  }

  return (
    <div>
      <PageHeader
        title="Dashboard"
        subtitle="Find re-creatable dependencies, build output and caches across your projects."
        actions={
          <Button variant="primary" onClick={scan} disabled={scanning || roots.length === 0}>
            {scanning ? "Scanning…" : "Start scan"}
          </Button>
        }
      />

      <div className="mb-5 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Stat
          label="Reclaimable (estimate)"
          value={formatBytes(total)}
          hint={summary ? `${candidates.length} items found` : "Run a scan to see"}
        />
        <Stat label="Scan roots" value={roots.length} hint="Folders you chose" />
        <Stat
          label="Last scan"
          value={lastScan ? new Date(lastScan.at).toLocaleDateString() : "—"}
          hint={summary ? `${summary.dirsVisited.toLocaleString()} folders in ${formatDuration(summary.elapsedMs)}` : undefined}
        />
        <Stat label="Moved to Trash" value={formatBytes(reclaimed)} hint="All time, this device" />
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <h2 className="mb-3 font-semibold">Folders to scan</h2>
          <RootsEditor />
        </Card>

        <Card>
          <h2 className="mb-3 font-semibold">By ecosystem</h2>
          {byEcosystem.length === 0 ? (
            <Empty>No results yet.</Empty>
          ) : (
            <ul className="space-y-3">
              {byEcosystem.map(([eco, v]) => (
                <li key={eco}>
                  <div className="mb-1 flex justify-between text-sm">
                    <span>{ECOSYSTEM_LABEL[eco] ?? eco}</span>
                    <span className="tabular-nums text-muted">
                      {formatBytes(v.bytes)} · {v.count}
                    </span>
                  </div>
                  <div className="h-2 overflow-hidden rounded bg-surface-2">
                    <div
                      className="h-full rounded bg-accent"
                      style={{ width: `${maxBytes ? Math.max(2, (v.bytes / maxBytes) * 100) : 0}%` }}
                    />
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card className="lg:col-span-2">
          <h2 className="mb-3 font-semibold">Recent activity</h2>
          {history.length === 0 ? (
            <Empty>Nothing yet. History stores only dates, counts and sizes — never file names.</Empty>
          ) : (
            <table className="w-full text-sm">
              <thead className="text-left text-xs text-muted">
                <tr>
                  <th className="pb-2 font-medium">When</th>
                  <th className="pb-2 font-medium">Action</th>
                  <th className="pb-2 text-right font-medium">Items</th>
                  <th className="pb-2 text-right font-medium">Size</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {history.slice(0, 8).map((h) => (
                  <tr key={h.at + h.kind}>
                    <td className="py-1.5">{formatDate(h.at)}</td>
                    <td className="py-1.5">
                      {h.kind === "scan" ? "Scan" : "Cleanup"}
                      {h.failed ? <span className="text-caution"> · {h.failed} not removed</span> : null}
                    </td>
                    <td className="py-1.5 text-right tabular-nums">{h.items}</td>
                    <td className="py-1.5 text-right tabular-nums">{formatBytes(h.bytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </div>
    </div>
  );
}
