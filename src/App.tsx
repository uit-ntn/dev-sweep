import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { useScanProgress } from "./hooks/useScanProgress";
import { CleanupReview } from "./pages/CleanupReview";
import { Dashboard } from "./pages/Dashboard";
import { DiskAnalyzer } from "./pages/DiskAnalyzer";
import { Scanner } from "./pages/Scanner";
import { Settings } from "./pages/Settings";
import { useSweepStore } from "./stores/useSweepStore";

export default function App() {
  const page = useSweepStore((s) => s.page);
  const theme = useSweepStore((s) => s.theme);
  const init = useSweepStore((s) => s.init);

  useScanProgress();

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="flex h-full">
      <Sidebar />
      <main className="min-w-0 flex-1 overflow-auto p-6">
        {page === "dashboard" && <Dashboard />}
        {page === "scanner" && <Scanner />}
        {page === "review" && <CleanupReview />}
        {page === "disk" && <DiskAnalyzer />}
        {page === "settings" && <Settings />}
      </main>
    </div>
  );
}
