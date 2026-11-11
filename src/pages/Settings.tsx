import { useState } from "react";
import { Button, Card, Empty, Notice, PageHeader, RiskBadge } from "../components/ui";
import { inTauri, pickFolders } from "../lib/api";
import { useSweepStore, type Theme } from "../stores/useSweepStore";

export function Settings() {
  const exclusions = useSweepStore((s) => s.exclusions);
  const addExclusions = useSweepStore((s) => s.addExclusions);
  const removeExclusion = useSweepStore((s) => s.removeExclusion);
  const theme = useSweepStore((s) => s.theme);
  const setTheme = useSweepStore((s) => s.setTheme);
  const rules = useSweepStore((s) => s.rules);
  const history = useSweepStore((s) => s.history);
  const clearHistory = useSweepStore((s) => s.clearHistory);
  const [error, setError] = useState<string | null>(null);

  async function addExclusion() {
    try {
      const picked = await pickFolders();
      if (picked.length) addExclusions(picked);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="space-y-4">
      <PageHeader title="Settings" subtitle="Preferences are stored only on this device." />

      <Card>
        <h2 className="mb-1 font-semibold">Excluded folders</h2>
        <p className="mb-3 text-sm text-muted">Scans never enter these folders or anything below them.</p>
        {exclusions.length === 0 ? (
          <p className="mb-3 text-sm text-muted">No exclusions.</p>
        ) : (
          <ul className="mb-3 divide-y divide-border rounded-md border border-border">
            {exclusions.map((e) => (
              <li key={e} className="flex items-center gap-2 px-3 py-2">
                <span className="min-w-0 flex-1 truncate font-mono text-xs" title={e}>
                  {e}
                </span>
                <Button variant="ghost" onClick={() => removeExclusion(e)}>
                  Remove
                </Button>
              </li>
            ))}
          </ul>
        )}
        <Button onClick={addExclusion} disabled={!inTauri()}>
          + Exclude folder
        </Button>
        {error && (
          <div className="mt-3">
            <Notice tone="error">{error}</Notice>
          </div>
        )}
      </Card>

      <Card>
        <h2 className="mb-3 font-semibold">Appearance</h2>
        <div className="flex gap-2" role="radiogroup" aria-label="Theme">
          {(["system", "light", "dark"] as Theme[]).map((t) => (
            <Button
              key={t}
              role="radio"
              aria-checked={theme === t}
              variant={theme === t ? "primary" : "secondary"}
              onClick={() => setTheme(t)}
            >
              {t[0].toUpperCase() + t.slice(1)}
            </Button>
          ))}
        </div>
      </Card>

      <Card>
        <h2 className="mb-1 font-semibold">Detection rules</h2>
        <p className="mb-3 text-sm text-muted">
          Built-in rules. A folder name alone is never enough — each rule also checks project context. Nothing is
          selected automatically. Protected: source code, <code>.git</code>, <code>.ipynb</code> notebooks,{" "}
          <code>.env</code> and secrets, IDE settings, system and home folders.
        </p>
        {rules.length === 0 ? (
          <Empty>Rules are loaded from the desktop app.</Empty>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-muted">
              <tr className="border-b border-border">
                <th className="py-2 font-medium">Rule</th>
                <th className="py-2 font-medium">Matches</th>
                <th className="py-2 font-medium">Context check</th>
                <th className="py-2 font-medium">Risk</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {rules.map((r) => (
                <tr key={r.id}>
                  <td className="py-2 pr-3">
                    <div className="font-medium">{r.display_name}</div>
                    <div className="font-mono text-xs text-muted">{r.id}</div>
                  </td>
                  <td className="py-2 pr-3 font-mono text-xs">{r.match_names.join(", ")}</td>
                  <td className="py-2 pr-3 text-xs text-muted">{r.context_hint}</td>
                  <td className="py-2">
                    <RiskBadge risk={r.risk} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <Card>
        <h2 className="mb-1 font-semibold">Local history</h2>
        <p className="mb-3 text-sm text-muted">
          {history.length} entries. Only dates, item counts and sizes are kept — never paths or file names.
        </p>
        <Button onClick={clearHistory} disabled={history.length === 0}>
          Clear history
        </Button>
      </Card>
    </div>
  );
}
