import { useState } from "react";
import { inspectAiData, inspectAiProject } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { AiDataItem } from "../types";
import { Button, Card, Empty, Notice } from "./ui";

export function AiDataInspector() {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ path?: string; items: AiDataItem[] }>();
  const [error, setError] = useState("");

  async function inspect(project: boolean) {
    setBusy(true);
    setError("");
    try {
      const next = project ? await inspectAiProject() : { items: await inspectAiData() };
      if (next) setResult(next);
    } catch (e) {
      setResult(undefined);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className="space-y-3 p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="font-semibold">AI tool data</h2>
          <p className="mt-1 text-sm text-muted">Check known locations for Claude, ChatGPT, Codex and Cursor.</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy} onClick={() => void inspect(false)}>Inspect local data</Button>
          <Button disabled={busy} onClick={() => void inspect(true)}>Choose project</Button>
        </div>
      </div>
      {busy && <p role="status" className="text-sm text-muted">Checking AI tool data…</p>}
      {error && <Notice tone="error">{error}</Notice>}
      {result && (
        <div aria-live="polite">
          <h3 className="break-all text-sm font-medium">{result.path ?? "Current user"}</h3>
          {result.items.length === 0 ? (
            <Empty>No recognized AI tool data found in this scope.</Empty>
          ) : (
            <>
              <p className="mt-1 text-xs text-muted">Read-only inspection. Sizes are logical estimates. Sessions, settings and workspace state can contain your work.</p>
              <ul className="mt-2 divide-y divide-border text-sm">
                {result.items.map((item) => (
                  <li key={item.path} className="flex flex-wrap items-start justify-between gap-2 py-2">
                    <div className="min-w-0">
                      <div>{item.tool} · {item.kind}</div>
                      <div className="break-all text-xs text-muted">{item.path}</div>
                    </div>
                    <span className="text-muted">
                      {formatBytes(item.bytes)}{item.incomplete && " · incomplete"}
                    </span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </Card>
  );
}
