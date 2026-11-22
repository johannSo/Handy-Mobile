import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  cancel as cancelScan,
  checkPermissions,
  Format,
  requestPermissions,
  scan,
} from "@tauri-apps/plugin-barcode-scanner";
import { ArrowLeft, Keyboard, ScanLine } from "lucide-react";
import { deviceName, pair, type PairError } from "../lib/link";

type Problem = PairError | "camera_denied" | "scan_failed";

const PAIR_ERRORS: PairError[] = [
  "invalid_code",
  "expired_code",
  "incompatible",
  "rate_limited",
  "unreachable",
  "untrusted",
  "failed",
];

const asPairError = (error: unknown): PairError =>
  PAIR_ERRORS.includes(error as PairError) ? (error as PairError) : "failed";

interface PairScreenProps {
  onPaired: () => void;
  /** Shown when there is a pairing to go back to. */
  onBack?: () => void;
}

export const PairScreen: React.FC<PairScreenProps> = ({ onPaired, onBack }) => {
  const { t } = useTranslation();
  const [scanning, setScanning] = useState(false);
  const [pairing, setPairing] = useState(false);
  const [manual, setManual] = useState(false);
  const [code, setCode] = useState("");
  const [problem, setProblem] = useState<Problem | null>(null);
  const scanCancelled = useRef(false);

  // The camera draws behind the page, so the page has to be see-through
  // for exactly as long as it's scanning.
  useEffect(() => {
    document.documentElement.classList.toggle("scanning", scanning);
    return () => document.documentElement.classList.remove("scanning");
  }, [scanning]);

  const submit = async (value: string) => {
    setProblem(null);
    setPairing(true);
    try {
      await pair(value.trim(), deviceName());
      onPaired();
    } catch (error) {
      setProblem(asPairError(error));
    } finally {
      setPairing(false);
    }
  };

  const startScan = async () => {
    setProblem(null);
    try {
      let permission = await checkPermissions();
      if (permission !== "granted") {
        permission = await requestPermissions();
      }
      if (permission !== "granted") {
        setProblem("camera_denied");
        return;
      }
    } catch {
      setProblem("scan_failed");
      return;
    }

    scanCancelled.current = false;
    setScanning(true);
    try {
      const result = await scan({ windowed: true, formats: [Format.QRCode] });
      setScanning(false);
      await submit(result.content);
    } catch {
      // Cancelling ends the scan with an error too; that isn't a problem.
      if (!scanCancelled.current) setProblem("scan_failed");
      setScanning(false);
    }
  };

  const stopScan = () => {
    scanCancelled.current = true;
    setScanning(false);
    void cancelScan().catch(() => undefined);
  };

  if (scanning) {
    return (
      <div className="safe-top safe-bottom flex h-full flex-col items-center justify-between">
        <p className="mt-10 rounded-full bg-black/60 px-4 py-2 text-sm text-white">
          {t("pair.scanHint")}
        </p>
        <div className="h-64 w-64 rounded-3xl border-4 border-white/90 shadow-[0_0_0_100vmax_rgba(0,0,0,0.45)]" />
        <button
          type="button"
          onClick={stopScan}
          className="mb-10 rounded-full bg-black/60 px-8 py-3 text-base font-medium text-white active:bg-black/80"
        >
          {t("pair.cancelScan")}
        </button>
      </div>
    );
  }

  return (
    // The insets pad the frame and the content scrolls inside it, so text
    // never slides under the status bar when the keyboard pushes it up.
    <div className="safe-top safe-bottom flex h-full flex-col">
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-6">
        <div className="flex h-12 shrink-0 items-center">
          {onBack && (
            <button
              type="button"
              onClick={onBack}
              aria-label={t("settings.close")}
              className="-ms-2 rounded-full p-2 text-ink-2 active:bg-wash"
            >
              <ArrowLeft className="h-6 w-6" />
            </button>
          )}
        </div>

        <div className="flex flex-1 flex-col justify-center gap-6 pb-8">
          <div>
            <p className="text-sm font-semibold text-background-ui">
              {t("app.name")}
            </p>
            <h1 className="mt-1 text-2xl font-bold">{t("pair.title")}</h1>
            <p className="mt-2 text-base text-ink-2">{t("pair.intro")}</p>
          </div>

          <ol className="space-y-3">
            {(["step1", "step2", "step3"] as const).map((step, index) => (
              <li key={step} className="flex gap-3">
                <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-logo-primary/40 text-sm font-semibold">
                  {index + 1}
                </span>
                <span className="text-base text-ink-2">
                  {t(`pair.${step}`)}
                </span>
              </li>
            ))}
          </ol>

          {problem && (
            <p
              role="alert"
              className="rounded-xl bg-error-wash px-4 py-3 text-sm text-error"
            >
              {t(`pair.errors.${problem}`)}
            </p>
          )}

          <div className="space-y-3">
            <button
              type="button"
              onClick={() => void startScan()}
              disabled={pairing}
              className="flex w-full items-center justify-center gap-2 rounded-2xl bg-background-ui px-4 py-4 text-base font-semibold text-white active:opacity-80 disabled:opacity-50"
            >
              <ScanLine className="h-5 w-5" />
              {pairing ? t("pair.pairing") : t("pair.scan")}
            </button>

            {manual ? (
              <form
                className="space-y-2"
                onSubmit={(event) => {
                  event.preventDefault();
                  void submit(code);
                }}
              >
                <textarea
                  value={code}
                  onChange={(event) => setCode(event.target.value)}
                  placeholder={t("pair.manualPlaceholder")}
                  // Once the keyboard has taken its space, bring the field and
                  // the button under it back into view.
                  onFocus={(event) => {
                    const form = event.currentTarget.form;
                    window.setTimeout(
                      () => form?.scrollIntoView({ block: "end" }),
                      350,
                    );
                  }}
                  rows={3}
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck={false}
                  className="selectable w-full resize-none rounded-2xl border border-line-strong bg-wash px-4 py-3 text-sm outline-none focus:border-background-ui"
                />
                <button
                  type="submit"
                  disabled={pairing || code.trim() === ""}
                  className="w-full rounded-2xl border border-line-strong px-4 py-3 text-base font-medium active:bg-wash disabled:opacity-50"
                >
                  {pairing ? t("pair.pairing") : t("pair.manualSubmit")}
                </button>
              </form>
            ) : (
              <button
                type="button"
                onClick={() => setManual(true)}
                className="flex w-full items-center justify-center gap-2 rounded-2xl px-4 py-3 text-base font-medium text-ink-2 active:bg-wash"
              >
                <Keyboard className="h-5 w-5" />
                {t("pair.manual")}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
