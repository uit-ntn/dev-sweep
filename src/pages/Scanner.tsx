import { useMemo, type ReactNode } from "react";
import { Button, Card, Empty, Notice, PageHeader, RiskBadge, riskLabel } from "../components/ui";
import { formatAge, formatBytes, formatCount, formatDuration } from "../lib/format";
import { filterAndSort, isSelectable, useSweepStore, type SortKey } from "../stores/useSweepStore";
import type { Candidate, Risk } from "../types";

const RISKS: Risk[] = ["low", "review", "caution"];
const SIZE_OPTIONS = [
  { label: "Any size", value: 0 },
  { label: "≥ 1 MiB", value: 1 << 20 },
  { label: "≥ 100 MiB", value: 100 << 20 },
  { label: "≥ 1 GiB", value: 1 << 30 },
];

function Chip({ on, onClick, children }: { on: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      aria-pressed={on}
      className={`rounded-full border px-2.5 py-0.5 text-xs transition ${
        on ? "border-accent bg-accent-soft text-text" : "border-border text-muted hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}

function toggle<T>(list: T[], v: T): T[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

export function Scanner() {
  const s = useSweepStore();
  const visible = useMemo(
    () => filterAndSort(s.liveCandidates, s.filters, s.sort),
    [s.liveCandidates, s.filters, s.sort],
  );
  const ecosystems = useMemo(
    () => Array.from(new Set(s.liveCandidates.map((c) => c.ecosystem))).sort(),
    [s.liveCandidates],
  );
  const inspected = s.liveCandidates.find((c) => c.id === s.inspectedId) ?? null;
  const selectedBytes = s.liveCandidates.filter((c) => s.selected.has(c.id)).reduce((a, c) => a + c.sizeBytes, 0);
  // "Select all visible" only ever covers bulk-selectable items: Caution and
  // blocked items must be ticked one by one.
  const bulkVisible = visible.filter((c) => c.bulkSelectable && isSelectable(c));
  const allVisibleSelected = bulkVisible.length > 0 && bulkVisible.every((c) => s.selected.has(c.id));
  const excludedFromBulk = visible.length - bulkVisible.length;
  const canSelect = !s.scanning && !s.summaryStale;

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Scanner"
        subtitle="Nothing is modified while scanning. Select items to review them before cleanup."
        actions={
          s.scanning ? (
            <Button variant="danger" onClick={s.cancelScan}>
              Cancel scan
            </Button>
          ) : (
            <>
              <Button onClick={s.startScan} disabled={s.roots.length === 0}>
                {s.summary ? "Rescan" : "Start scan"}
              </Button>
              <Button variant="primary" disabled={s.selected.size === 0} onClick={() => s.setPage("review")}>
                Review {s.selected.size || ""} selected · ~{formatBytes(selectedBytes)}
              </Button>
            </>
          )
        }
      />

      <ScanStatus />

      <Card className="mb-3 space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="search"
            placeholder="Filter by path or type…"
            value={s.filters.text}
            onChange={(e) => s.setFilters({ text: e.target.value })}
            className="min-w-48 flex-1 rounded-md border border-border bg-bg px-3 py-1.5 text-sm"
            aria-label="Filter by path"
          />
          <select
            value={s.filters.minBytes}
            onChange={(e) => s.setFilters({ minBytes: Number(e.target.value) })}
            className="rounded-md border border-border bg-bg px-2 py-1.5 text-sm"
            aria-label="Minimum size"
          >
            {SIZE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          <select
            value={s.sort}
            onChange={(e) => s.setSort(e.target.value as SortKey)}
            className="rounded-md border border-border bg-bg px-2 py-1.5 text-sm"
            aria-label="Sort by"
          >
            <option value="size">Largest first</option>
            <option value="modified">Least recently modified</option>
            <option value="path">Path A–Z</option>
          </select>
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="mr-1 text-xs text-muted">Risk</span>
          {RISKS.map((r) => (
            <Chip key={r} on={s.filters.risks.includes(r)} onClick={() => s.setFilters({ risks: toggle(s.filters.risks, r) })}>
              {riskLabel[r]}
            </Chip>
          ))}
          {ecosystems.length > 0 && <span className="ml-3 mr-1 text-xs text-muted">Ecosystem</span>}
          {ecosystems.map((e) => (
            <Chip
              key={e}
              on={s.filters.ecosystems.includes(e)}
              onClick={() => s.setFilters({ ecosystems: toggle(s.filters.ecosystems, e) })}
            >
              {e}
            </Chip>
          ))}
        </div>
      </Card>

      <div className="flex min-h-0 flex-1 gap-3">
        <Card className="min-h-0 flex-1 overflow-auto p-0">
          {visible.length === 0 ? (
            <Empty>
              {s.scanning
                ? "Looking for matches…"
                : s.summary
                  ? "No items match the current filters."
                  : "Run a scan to see results."}
            </Empty>
          ) : (
            <table className="w-full text-sm">
              <thead className="sticky top-0 z-10 bg-surface text-left text-xs text-muted">
                <tr className="border-b border-border">
                  <th className="w-8 px-3 py-2">
                    <input
                      type="checkbox"
                      aria-label="Select all visible items that allow bulk selection"
                      title={
                        excludedFromBulk > 0
                          ? `${excludedFromBulk} Caution or blocked items are never selected in bulk`
                          : "Select all visible"
                      }
                      disabled={!canSelect || bulkVisible.length === 0}
                      checked={allVisibleSelected}
                      onChange={(e) => s.selectBulk(visible, e.target.checked)}
                    />
                  </th>
                  <th className="py-2 font-medium">Item</th>
                  <th className="py-2 font-medium">Risk</th>
                  <th className="py-2 text-right font-medium">Modified</th>
                  <th className="px-3 py-2 text-right font-medium" title="Logical size at scan time">
                    Est. size
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {visible.map((c) => (
                  <Row key={c.id} c={c} canSelect={canSelect} />
                ))}
              </tbody>
            </table>
          )}
        </Card>
        {inspected && <Inspector c={inspected} />}
      </div>
    </div>
  );
}

function Row({ c, canSelect }: { c: Candidate; canSelect: boolean }) {
  const selected = useSweepStore((s) => s.selected.has(c.id));
  const inspectedId = useSweepStore((s) => s.inspectedId);
  const toggleSelected = useSweepStore((s) => s.toggleSelected);
  const inspect = useSweepStore((s) => s.inspect);
  return (
    <tr
      className={`cursor-pointer hover:bg-surface-2 ${inspectedId === c.id ? "bg-surface-2" : ""}`}
      onClick={() => inspect(inspectedId === c.id ? null : c.id)}
    >
      <td className="px-3 py-2" onClick={(e) => e.stopPropagation()}>
        <input
          type="checkbox"
          aria-label={`Select ${c.path}`}
          title={c.blockedReason ?? (c.bulkSelectable ? undefined : "Caution: select individually")}
          disabled={!canSelect || !isSelectable(c)}
          checked={selected}
          onChange={() => toggleSelected(c.id)}
        />
      </td>
      <td className="max-w-0 py-2 pr-3">
        <div className="flex flex-wrap items-center gap-2 font-medium">
          {c.displayName}
          {c.blockedReason && (
            <span className="rounded bg-surface-2 px-1.5 py-0.5 text-xs font-normal text-muted">Not cleanable</span>
          )}
        </div>
        <div className="truncate font-mono text-xs text-muted" title={c.path}>
          {c.path}
        </div>
      </td>
      <td className="py-2">
        <RiskBadge risk={c.risk} />
      </td>
      <td className="whitespace-nowrap py-2 text-right text-muted">{formatAge(c.lastModified)}</td>
      <td
        className="whitespace-nowrap px-3 py-2 text-right tabular-nums"
        title={c.sizeComplete ? undefined : "Some entries could not be read; size is a lower bound"}
      >
        {c.sizeComplete ? "" : "≥ "}
        {formatBytes(c.sizeBytes)}
      </td>
    </tr>
  );
}

function Inspector({ c }: { c: Candidate }) {
  const rules = useSweepStore((s) => s.rules);
  const inspect = useSweepStore((s) => s.inspect);
  const addExclusions = useSweepStore((s) => s.addExclusions);
  const rule = rules.find((r) => r.id === c.ruleId);
  return (
    <Card className="w-80 shrink-0 overflow-auto">
      <div className="mb-3 flex items-start justify-between gap-2">
        <h2 className="font-semibold">{c.displayName}</h2>
        <button className="text-muted hover:text-text" onClick={() => inspect(null)} aria-label="Close details">
          ✕
        </button>
      </div>
      <dl className="space-y-2 text-sm">
        <Field label="Path">
          <span className="break-all font-mono text-xs">{c.path}</span>
        </Field>
        <Field label="Project">
          <span className="break-all font-mono text-xs">{c.projectDir}</span>
        </Field>
        <Field label="Risk">
          <RiskBadge risk={c.risk} />
        </Field>
        <Field label="Estimated size (logical, at scan time)">
          {c.sizeComplete ? "" : "≥ "}
          {formatBytes(c.sizeBytes)} in {formatCount(c.fileCount)} files
        </Field>
        <Field label="Last modified">{formatAge(c.lastModified)}</Field>
        <Field label="Rule">
          <span className="font-mono text-xs">{c.ruleId}</span>
        </Field>
      </dl>
      {rule && (
        <div className="mt-4 space-y-2 text-sm">
          <p>{rule.description}</p>
          <p className="text-xs text-muted">Matched because: {rule.context_hint}</p>
        </div>
      )}
      {c.blockedReason && (
        <div className="mt-3">
          <Notice tone="warn">Cannot be cleaned: {c.blockedReason}.</Notice>
        </div>
      )}
      {c.unreadable > 0 && (
        <div className="mt-3">
          <Notice tone="warn">
            {c.unreadable} entries could not be read; the size is a lower bound and cleanup may partly fail.
          </Notice>
        </div>
      )}
      {c.risk === "caution" && (
        <div className="mt-3">
          <Notice tone="warn">
            This folder name is ambiguous or may contain unique data. Open it and check first. Caution items are never
            included by "select all" and must be confirmed individually at review.
          </Notice>
        </div>
      )}
      <Button className="mt-4 w-full" onClick={() => void addExclusions([c.path])}>
        Always exclude this folder
      </Button>
    </Card>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-xs text-muted">{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function ScanStatus() {
  const scanning = useSweepStore((s) => s.scanning);
  const progress = useSweepStore((s) => s.progress);
  const summary = useSweepStore((s) => s.summary);
  const error = useSweepStore((s) => s.scanError);
  const roots = useSweepStore((s) => s.roots);
  const stale = useSweepStore((s) => s.summaryStale);

  if (stale && !scanning)
    return (
      <div className="mb-3">
        <Notice tone="warn">
          Exclusions changed after this scan. These results can no longer be cleaned — scan again.
        </Notice>
      </div>
    );
  if (error) return <div className="mb-3"><Notice tone="error">Scan failed: {error}</Notice></div>;
  if (!scanning && roots.length === 0)
    return (
      <div className="mb-3">
        <Notice>Add a folder on the Dashboard to start scanning.</Notice>
      </div>
    );
  if (scanning)
    return (
      <Card className="mb-3">
        <div className="mb-2 h-1 overflow-hidden rounded bg-surface-2">
          <div className="h-full w-1/3 animate-pulse rounded bg-accent" />
        </div>
        <div className="flex flex-wrap gap-x-6 gap-y-1 text-sm">
          <span>{formatCount(progress?.dirsVisited ?? 0)} folders</span>
          <span>{formatCount(progress?.candidatesFound ?? 0)} matches</span>
          <span>{formatBytes(progress?.bytesFound ?? 0)} found</span>
          {(progress?.errors ?? 0) > 0 && <span className="text-review">{progress?.errors} unreadable</span>}
        </div>
        <div className="mt-1 truncate font-mono text-xs text-muted">{progress?.currentPath}</div>
      </Card>
    );
  if (summary)
    return (
      <div className="mb-3">
        <Notice tone={summary.incomplete ? "warn" : "info"}>
          {summary.cancelled ? "Scan cancelled — results are partial. " : ""}
          Scanned {formatCount(summary.dirsVisited)} folders in {formatDuration(summary.elapsedMs)}; found{" "}
          {summary.candidates.length} items, about {formatBytes(summary.totalBytes)} (logical size at scan time).
          {summary.errors > 0 && ` ${summary.errors} folders could not be read, so items may be missing.`}
          {!summary.cancelled && summary.incomplete && summary.errors === 0 && " Some sizes are lower bounds."}
        </Notice>
      </div>
    );
  return null;
}
