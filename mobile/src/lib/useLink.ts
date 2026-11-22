import { useEffect, useState } from "react";
import { getSnapshot, onSnapshot, reconnect, type Snapshot } from "./link";

const INITIAL: Snapshot = {
  link: "connecting",
  desktop_name: null,
  desktop_version: null,
  default_output: null,
  session: "idle",
  detail: null,
};

/** The core's view of the link, kept current. `null` until first read. */
export const useLink = (): Snapshot | null => {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unlisten = onSnapshot((next) => {
      if (!cancelled) setSnapshot(next);
    });
    getSnapshot()
      .then((first) => {
        // An event that beat this read is newer than it.
        if (!cancelled) setSnapshot((current) => current ?? first);
      })
      .catch(() => {
        if (!cancelled) setSnapshot((current) => current ?? INITIAL);
      });

    // Coming back to the app is the moment to find the desktop again.
    const onVisible = () => {
      if (document.visibilityState === "visible") void reconnect();
    };
    document.addEventListener("visibilitychange", onVisible);

    return () => {
      cancelled = true;
      void unlisten.then((stop) => stop());
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, []);

  return snapshot;
};
