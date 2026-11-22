import React, { useEffect, useState } from "react";
import type { Snapshot } from "./lib/link";
import {
  keyboardAvailable,
  markSetupSeen,
  setupSeen,
  setupStatus,
  takeSetupRequest,
} from "./lib/setup";
import { useLink } from "./lib/useLink";
import { KeyboardSetup } from "./screens/KeyboardSetup";
import { MainScreen } from "./screens/MainScreen";
import { PairScreen } from "./screens/PairScreen";

export const App: React.FC = () => {
  const snapshot = useLink();
  // "Pair Again" from a phone the desktop no longer knows.
  const [repairing, setRepairing] = useState(false);

  if (!snapshot) return null;

  return (
    <>
      {/* Opaque behind the status bar, so nothing scrolls through the clock. */}
      <div className="status-bar-cover" />
      <Screen
        snapshot={snapshot}
        repairing={repairing}
        setRepairing={setRepairing}
      />
    </>
  );
};

const Screen: React.FC<{
  snapshot: Snapshot;
  repairing: boolean;
  setRepairing: (repairing: boolean) => void;
}> = ({ snapshot, repairing, setRepairing }) => {
  // The keyboard setup: offered once after pairing, opened from settings, or
  // asked for by the keyboard itself when it's missing something.
  const [setup, setSetup] = useState<"offered" | "opened" | null>(null);
  useEffect(() => {
    const check = () => {
      if (document.visibilityState === "visible" && takeSetupRequest()) {
        setSetup("opened");
      }
    };
    check();
    document.addEventListener("visibilitychange", check);
    return () => document.removeEventListener("visibilitychange", check);
  }, []);

  const paired = snapshot.link !== "unpaired" && !repairing;
  useEffect(() => {
    if (!paired || setupSeen() || !keyboardAvailable()) return;
    markSetupSeen();
    if (!setupStatus()?.keyboardSelected) setSetup("offered");
  }, [paired]);

  if (setup && paired) {
    return (
      <KeyboardSetup
        fromSettings={setup === "opened"}
        onDone={() => setSetup(null)}
      />
    );
  }

  if (snapshot.link === "unpaired" || repairing) {
    return (
      <PairScreen
        onPaired={() => setRepairing(false)}
        onBack={
          repairing && snapshot.link !== "unpaired"
            ? () => setRepairing(false)
            : undefined
        }
      />
    );
  }

  return (
    <MainScreen
      snapshot={snapshot}
      onPairAgain={() => setRepairing(true)}
      onKeyboardSetup={() => setSetup("opened")}
    />
  );
};
