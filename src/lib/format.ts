const UNITS = ["B", "KiB", "MiB", "GiB", "TiB"];

/** Sizes are integer bytes internally; format only for display. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${UNITS[i]}`;
}

export function formatCount(n: number): string {
  return n.toLocaleString();
}

export function formatAge(epochSecs: number | null): string {
  if (epochSecs == null) return "—";
  const days = Math.floor((Date.now() / 1000 - epochSecs) / 86400);
  if (days < 1) return "today";
  if (days < 30) return `${days}d ago`;
  if (days < 365) return `${Math.floor(days / 30)}mo ago`;
  return `${(days / 365).toFixed(1)}y ago`;
}

export function formatDate(ms: number): string {
  return new Date(ms).toLocaleString();
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(1)} s`;
}
