import { useEffect, useRef, useState } from "react";
import { AiDataInspector } from "../components/AiDataInspector";
import { Button, Card, Empty, Notice, PageHeader } from "../components/ui";
import { inTauri, inspectAppStorage, listInstalledApps, measureAppCache } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { AiDataItem, AppCacheSize, AppInventory, InstalledApp, SupportedBrowser } from "../types";

function browserFor(app: InstalledApp): SupportedBrowser | undefined {
  if (app.name === "Google Chrome" && app.publisher.startsWith("Google ")) return "chrome";
  if (app.name === "Microsoft Edge" && app.publisher === "Microsoft Corporation") return "edge";
}

export function InstalledApps() {
  const [inventory, setInventory] = useState<AppInventory>();
  const [refresh, setRefresh] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [cache, setCache] = useState<{ browser: SupportedBrowser; results: AppCacheSize[] }>();
  const [measuring, setMeasuring] = useState<SupportedBrowser>();
  const [cacheError, setCacheError] = useState("");
  const cachePanel = useRef<HTMLDivElement>(null);
  const [storage, setStorage] = useState<{ name: string; items: AiDataItem[] }>();
  const [storageBusy, setStorageBusy] = useState(false);

  async function inspectStorage(app: InstalledApp) {
    setStorageBusy(true);
    setCacheError("");
    setStorage(undefined);
    try {
      const items = await inspectAppStorage(app.name.toLowerCase() === "postman" ? "postman" : "capcut");
      setStorage({ name: app.name, items });
    } catch (e) {
      setCacheError(String(e));
    } finally {
      setStorageBusy(false);
    }
  }

  useEffect(() => {
    if (!inTauri()) return;
    let active = true;
    listInstalledApps()
      .then((data) => { if (active) setInventory(data); })
      .catch((e: unknown) => { if (active) setError(String(e)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [refresh]);

  useEffect(() => {
    if (cache || cacheError || storage) cachePanel.current?.scrollIntoView({ block: "nearest" });
  }, [cache, cacheError, storage]);

  async function checkCache(browser: SupportedBrowser) {
    setMeasuring(browser);
    setCache(undefined);
    setCacheError("");
    try {
      setCache({ browser, results: await measureAppCache(browser) });
    } catch (e) {
      setCacheError(String(e));
    } finally {
      setMeasuring(undefined);
    }
  }

  const search = query.trim().toLowerCase();
  const apps = inventory?.apps.filter((app) =>
    [app.name, app.publisher, app.location].some((value) => value.toLowerCase().includes(search)),
  ) ?? [];

  return (
    <div className="space-y-4">
      <PageHeader
        title="Installed apps"
        subtitle="Inspect registered Windows apps, browser caches and AI tool data."
        actions={inTauri() && (
          <Button disabled={loading} onClick={() => {
            setError("");
            setLoading(true);
            setRefresh((current) => current + 1);
          }}>Refresh</Button>
        )}
      />
      {!inTauri() && <Notice>Open DevSweep on Windows to inspect installed apps.</Notice>}
      {inTauri() && <AiDataInspector />}
      {inTauri() && loading && <p role="status" className="text-sm text-muted">Reading installed apps…</p>}
      {error && <Notice tone="error">{error}</Notice>}
      {inventory?.warnings.map((warning) => <Notice key={warning} tone="warn">{warning}</Notice>)}
      {(cache || cacheError || storage) && (
        <div ref={cachePanel} aria-live="polite">
          {storage && (
            <Card className="space-y-3 p-4">
              <h2 className="font-semibold">{storage.name} storage</h2>
              <p className="text-sm text-muted">Known default locations. Review cache candidates in the application before clearing. Custom locations may require Disk Analyzer.</p>
              {storage.items.length === 0 && <Empty>No recognized folders found at the default locations.</Empty>}
              {storage.items.map((item) => (
                <div key={item.path} className="border-t border-border py-2 text-sm">
                  <div>{item.kind} · {formatBytes(item.bytes)}{item.incomplete && " · incomplete"}</div>
                  <div className="break-all text-xs text-muted">{item.path}</div>
                </div>
              ))}
            </Card>
          )}
          {cacheError && <Notice tone="error">{cacheError}</Notice>}
          {cache && <Card className="space-y-3 p-4">
            <h2 className="font-semibold">{cache.browser === "chrome" ? "Google Chrome" : "Microsoft Edge"} cache</h2>
            <p className="text-sm text-muted">Estimated logical size in standard browser profiles for this Windows user.</p>
            {cache.results.length === 0 && <Empty>No supported cache folders found.</Empty>}
            {cache.results.length > 0 && (
              <>
                <p className="text-sm font-medium">{formatBytes(cache.results.reduce((total, item) => total + item.bytes, 0))} measured</p>
                <ul className="divide-y divide-border text-sm">
                  {cache.results.map((item) => (
                    <li key={item.path} className="flex flex-wrap justify-between gap-2 py-2">
                      <span className="break-all">{item.path}</span>
                      <span className="text-muted">{formatBytes(item.bytes)}{item.unreadable > 0 && " · incomplete"}</span>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </Card>}        </div>
      )}
      {inventory && (
        <Card className="overflow-hidden">
          <div className="flex flex-wrap items-center gap-3 border-b border-border p-3">
            <input
              type="search"
              aria-label="Search installed apps"
              placeholder="Search apps, publishers or paths"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="min-w-56 flex-1 rounded-md border border-border bg-surface-2 px-3 py-2 text-sm"
            />
            <span className="text-sm text-muted">{apps.length} of {inventory.apps.length} apps</span>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full table-fixed text-left text-sm">
              <thead className="bg-surface-2 text-muted">
                <tr>
                  <th scope="col" className="w-[30%] p-3">App</th>
                  <th scope="col" className="w-[35%] p-3">Install location</th>
                  <th scope="col" className="w-[20%] p-3">Source evidence</th>
                  <th scope="col" className="w-[15%] p-3">Cache</th>
                </tr>
              </thead>
              <tbody>
                {apps.map((app) => {
                  const browser = browserFor(app);
                  return (
                    <tr key={JSON.stringify(app)} className="border-t border-border align-top">
                      <td className="break-words p-3">
                        {["postman", "capcut"].includes(app.name.toLowerCase()) ? (
                          <button disabled={storageBusy} className="font-medium text-accent underline" onClick={() => void inspectStorage(app)}>
                            {app.name} · Inspect storage
                          </button>
                        ) : <div className="font-medium">{app.name}</div>}
                        <div className="text-xs text-muted">
                          {[app.publisher, app.version].filter(Boolean).join(" · ")}
                        </div>
                      </td>
                      <td className="break-all p-3 text-xs">{app.location || "Unavailable"}</td>
                      <td className="p-3">
                        <div>{app.source === "storeSigned" ? "Microsoft Store signature" : "Unknown"}</div>
                        <div className="text-xs text-muted">{app.registration === "msix" ? "Packaged app" : "Desktop app"}</div>
                      </td>
                      <td className="p-3">
                        {browser ? (
                          <Button disabled={Boolean(measuring)} onClick={() => void checkCache(browser)}>
                            {measuring === browser ? "Measuring…" : "Measure"}
                          </Button>
                        ) : <span className="text-muted">—</span>}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {apps.length === 0 && <Empty>{search ? "No apps match your search." : "No registered apps found."}</Empty>}
        </Card>
      )}
    </div>
  );
}
