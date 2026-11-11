import { useEffect } from "react";
import { onScanProgress } from "../lib/api";
import { useSweepStore } from "../stores/useSweepStore";

/** Subscribe once to backend scan progress events. */
export function useScanProgress() {
  const handle = useSweepStore((s) => s.handleProgress);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    onScanProgress(handle).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [handle]);
}
