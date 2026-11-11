import { useSweepStore } from "../stores/useSweepStore";
import type { Page } from "../types";
import { formatBytes } from "../lib/format";
import { useDiskStore } from "../stores/useDiskStore";

const items: { page: Page; label: string; icon: string }[] = [
  { page: "dashboard", label: "Dashboard", icon: "M3 13h8V3H3zm0 8h8v-6H3zm10 0h8V11h-8zm0-18v6h8V3z" },
  {
    page: "scanner",
    label: "Scanner",
    icon: "M10 2a8 8 0 0 1 6.32 12.9l5.39 5.4-1.42 1.4-5.39-5.38A8 8 0 1 1 10 2m0 2a6 6 0 1 0 0 12 6 6 0 0 0 0-12",
  },
  { page: "review", label: "Cleanup review", icon: "M9 3h6l1 2h4v2H4V5h4zm-3 6h12l-1 12H7z" },
  {
    page: "disk",
    label: "Disk Analyzer",
    icon: "M12 2a10 10 0 1 0 10 10h-10zm2-0v8h8A10 10 0 0 0 14 2",
  },
  {
    page: "settings",
    label: "Settings",
    icon: "M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8m8.5 4-.1-1.3 2-1.6-2-3.4-2.4 1-2.2-1.3L15.5 3h-4l-.4 2.4L9 6.7l-2.4-1-2 3.4 2 1.6L6.5 12l.1 1.3-2 1.6 2 3.4 2.4-1 2.2 1.3.4 2.4h4l.4-2.4 2.2-1.3 2.4 1 2-3.4-2-1.6z",
  },
];

export function Sidebar() {
  const page = useSweepStore((s) => s.page);
  const setPage = useSweepStore((s) => s.setPage);
  const selectedCount = useSweepStore((s) => s.selected.size);
  const scanning = useSweepStore((s) => s.scanning);
  const bytesFound = useSweepStore((s) => s.progress?.bytesFound ?? 0);
  const analyzing = useDiskStore((s) => s.analyzing);

  return (
    <nav className="flex w-56 shrink-0 flex-col border-r border-border bg-surface p-3" aria-label="Main">
      <div className="mb-6 flex items-center gap-2 px-2 pt-1">
        <img src="/icon.svg" alt="" className="h-7 w-7" />
        <div>
          <div className="text-base font-semibold leading-tight">DevSweep</div>
          <div className="text-xs text-muted">Reclaim your space</div>
        </div>
      </div>
      <ul className="space-y-1">
        {items.map((it) => (
          <li key={it.page}>
            <button
              onClick={() => setPage(it.page)}
              aria-current={page === it.page ? "page" : undefined}
              className={`flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left text-sm transition ${
                page === it.page
                  ? "bg-accent-soft font-medium text-text"
                  : "text-muted hover:bg-surface-2 hover:text-text"
              }`}
            >
              <svg viewBox="0 0 24 24" className="h-4 w-4 fill-current" aria-hidden>
                <path d={it.icon} />
              </svg>
              <span className="flex-1">{it.label}</span>
              {it.page === "review" && selectedCount > 0 && (
                <span className="rounded-full bg-accent px-1.5 text-xs tabular-nums text-accent-fg">
                  {selectedCount}
                </span>
              )}
              {it.page === "scanner" && scanning && (
                <span className="h-2 w-2 animate-pulse rounded-full bg-accent" aria-label="Scan running" />
              )}
              {it.page === "disk" && analyzing && (
                <span className="h-2 w-2 animate-pulse rounded-full bg-accent" aria-label="Analysis running" />
              )}
            </button>
          </li>
        ))}
      </ul>
      <div className="mt-auto px-2 text-xs text-muted">
        {scanning ? <>Scanning… {formatBytes(bytesFound)} found</> : <>Local only · nothing is uploaded</>}
      </div>
    </nav>
  );
}
