import { useState } from "react";
import { inTauri, pickFolders } from "../lib/api";
import { useSweepStore } from "../stores/useSweepStore";
import { Button, Notice } from "./ui";

/** Manage the user-selected scan roots. Roots are validated by the backend. */
export function RootsEditor() {
  const roots = useSweepStore((s) => s.roots);
  const addRoots = useSweepStore((s) => s.addRoots);
  const removeRoot = useSweepStore((s) => s.removeRoot);
  const scanning = useSweepStore((s) => s.scanning);
  const [errors, setErrors] = useState<string[]>([]);

  async function add() {
    try {
      const picked = await pickFolders();
      if (picked.length) setErrors(await addRoots(picked));
    } catch (e) {
      setErrors([String(e)]);
    }
  }

  return (
    <div className="space-y-3">
      {roots.length === 0 ? (
        <p className="text-sm text-muted">
          No folders selected. DevSweep only looks inside folders you choose — pick the directory that holds your
          projects (for example <code className="font-mono">D:\src</code>).
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-md border border-border">
          {roots.map((r) => (
            <li key={r} className="flex items-center gap-2 px-3 py-2">
              <span className="min-w-0 flex-1 truncate font-mono text-xs" title={r}>
                {r}
              </span>
              <Button variant="ghost" onClick={() => removeRoot(r)} disabled={scanning} aria-label={`Remove ${r}`}>
                Remove
              </Button>
            </li>
          ))}
        </ul>
      )}
      <Button onClick={add} disabled={scanning || !inTauri()}>
        + Add folder
      </Button>
      {!inTauri() && (
        <Notice tone="warn">Running in a browser preview. Open the desktop app to choose folders and scan.</Notice>
      )}
      {errors.length > 0 && (
        <Notice tone="error">
          {errors.map((e) => (
            <div key={e}>{e}</div>
          ))}
        </Notice>
      )}
    </div>
  );
}
