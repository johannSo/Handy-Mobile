import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronRight,
  Copy,
  Keyboard,
  Laptop,
  Loader2,
  Mic,
  Settings,
  Share2,
  Smartphone,
  Square,
  Trash2,
  X,
} from "lucide-react";
import { reconnect, unpair, type OutputMode, type Snapshot } from "../lib/link";
import { keyboardAvailable, setupStatus } from "../lib/setup";
import { useDictation, type Transcript } from "../lib/useDictation";

const OUTPUT_KEY = "handy-mobile.output";

const storedOutput = (): OutputMode | null => {
  const value = localStorage.getItem(OUTPUT_KEY);
  return value === "desktop" || value === "phone" ? value : null;
};

const copyText = async (text: string): Promise<boolean> => {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
};

const formatElapsed = (ms: number) => {
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
};

/** Ticks while a recording runs. */
const useElapsed = (startedAt: number | null) => {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (startedAt === null) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [startedAt]);
  return startedAt === null ? null : Math.max(0, now - startedAt);
};

const TranscriptCard: React.FC<{ transcript: Transcript }> = ({
  transcript,
}) => {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    if (await copyText(transcript.text)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    }
  };

  return (
    <li className="rounded-2xl border border-line bg-wash px-4 py-3">
      <p className="selectable whitespace-pre-wrap text-base leading-relaxed">
        {transcript.text}
      </p>
      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="flex items-center gap-1 text-xs text-ink-3">
          {transcript.pasted && (
            <>
              <Laptop className="h-3.5 w-3.5" />
              {t("transcripts.pasted")}
            </>
          )}
        </span>
        <div className="flex gap-1">
          {typeof navigator.share === "function" && (
            <button
              type="button"
              onClick={() =>
                void navigator.share({ text: transcript.text }).catch(() => {})
              }
              aria-label={t("transcripts.share")}
              className="rounded-full p-2 text-ink-2 active:bg-line"
            >
              <Share2 className="h-4 w-4" />
            </button>
          )}
          <button
            type="button"
            onClick={() => void copy()}
            className="flex items-center gap-1.5 rounded-full px-3 py-2 text-sm font-medium text-ink-2 active:bg-line"
          >
            {copied ? (
              <Check className="h-4 w-4 text-moss" />
            ) : (
              <Copy className="h-4 w-4" />
            )}
            {copied ? t("transcripts.copied") : t("transcripts.copy")}
          </button>
        </div>
      </div>
    </li>
  );
};

const SettingsSheet: React.FC<{
  snapshot: Snapshot;
  onClose: () => void;
  onKeyboardSetup: () => void;
}> = ({ snapshot, onClose, onKeyboardSetup }) => {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  const name = snapshot.desktop_name || t("link.yourComputer");

  return (
    <div
      className="fixed inset-0 z-20 flex items-end bg-black/50"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("settings.title")}
        className="safe-bottom w-full rounded-t-3xl bg-background px-6 pt-4"
      >
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">{t("settings.title")}</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("settings.close")}
            className="-me-2 rounded-full p-2 text-ink-2 active:bg-wash"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <div className="mt-4 flex items-center gap-3 rounded-2xl border border-line px-4 py-3">
          <Laptop className="h-6 w-6 shrink-0 text-ink-2" />
          <div className="min-w-0">
            <p className="text-xs text-ink-3">{t("settings.computer")}</p>
            <p className="truncate text-base font-medium">{name}</p>
            {snapshot.desktop_version && (
              <p className="text-xs text-ink-3">
                {t("settings.version", { version: snapshot.desktop_version })}
              </p>
            )}
          </div>
        </div>

        {keyboardAvailable() && (
          <button
            type="button"
            onClick={onKeyboardSetup}
            className="mt-3 flex w-full items-center gap-3 rounded-2xl border border-line px-4 py-3 text-start active:bg-wash"
          >
            <Keyboard className="h-6 w-6 shrink-0 text-ink-2" />
            <div className="min-w-0 flex-1">
              <p className="text-base font-medium">
                {t("keyboard.settingsRow")}
              </p>
              <p className="text-xs text-ink-3">
                {setupStatus()?.keyboardEnabled
                  ? t("keyboard.settingsOn")
                  : t("keyboard.settingsOff")}
              </p>
            </div>
            <ChevronRight className="h-5 w-5 shrink-0 text-ink-3" />
          </button>
        )}

        <div className="py-5">
          {confirming ? (
            <div className="space-y-3">
              <p className="text-sm text-ink-2">
                {t("settings.unpairConfirm", { name })}
              </p>
              <p className="text-xs text-ink-3">{t("settings.unpairNote")}</p>
              <div className="flex gap-3">
                <button
                  type="button"
                  onClick={() => setConfirming(false)}
                  className="flex-1 rounded-2xl border border-line-strong px-4 py-3 text-base font-medium active:bg-wash"
                >
                  {t("settings.cancel")}
                </button>
                <button
                  type="button"
                  onClick={() => void unpair()}
                  className="flex-1 rounded-2xl bg-error px-4 py-3 text-base font-semibold text-background active:opacity-80"
                >
                  {t("settings.confirm")}
                </button>
              </div>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => setConfirming(true)}
              className="w-full rounded-2xl border border-line-strong px-4 py-3 text-base font-medium text-error active:bg-error-wash"
            >
              {t("settings.unpair")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
};

interface MainScreenProps {
  snapshot: Snapshot;
  onPairAgain: () => void;
  onKeyboardSetup: () => void;
}

export const MainScreen: React.FC<MainScreenProps> = ({
  snapshot,
  onPairAgain,
  onKeyboardSetup,
}) => {
  const { t } = useTranslation();
  const [chosenOutput, setChosenOutput] = useState<OutputMode | null>(
    storedOutput,
  );
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Until the user picks here, follow what the desktop is set to.
  const output: OutputMode =
    chosenOutput ?? snapshot.default_output ?? "desktop";
  const dictation = useDictation(snapshot, output);
  const elapsed = useElapsed(dictation.startedAt);

  const name = snapshot.desktop_name || t("link.yourComputer");
  const connected = snapshot.link === "connected";

  // What the big button is doing, from the microphone here and the session
  // on the desktop together.
  const phase =
    dictation.mic !== "idle"
      ? dictation.mic
      : snapshot.session === "transcribing"
        ? "transcribing"
        : snapshot.session === "recording"
          ? "finishing"
          : "idle";

  const chooseOutput = (mode: OutputMode) => {
    setChosenOutput(mode);
    localStorage.setItem(OUTPUT_KEY, mode);
  };

  // Text that stayed on the phone is most useful on its clipboard.
  const newest = dictation.transcripts[0];
  const lastCopied = useRef<number | null>(newest?.id ?? null);
  useEffect(() => {
    if (!newest || newest.id === lastCopied.current) return;
    lastCopied.current = newest.id;
    if (!newest.pasted) void copyText(newest.text);
  }, [newest]);

  const onButton = () => {
    if (phase === "idle") void dictation.start();
    else if (phase === "recording") void dictation.finish();
  };

  const statusLine =
    phase === "idle"
      ? t("record.tapToStart")
      : phase === "starting"
        ? t("record.starting")
        : phase === "recording"
          ? t("record.listening")
          : phase === "finishing"
            ? t("record.finishing")
            : t("record.transcribing");

  const linkBanner =
    snapshot.link === "offline" ? (
      <div className="mx-4 rounded-2xl bg-error-wash px-4 py-3">
        <p className="text-sm font-medium text-error">{t("link.offline")}</p>
        <p className="mt-1 text-sm text-ink-2">
          {t("link.offlineHint", { name })}
        </p>
        <button
          type="button"
          onClick={() => void reconnect()}
          className="mt-2 rounded-full border border-line-strong px-4 py-1.5 text-sm font-medium active:bg-wash"
        >
          {t("link.retry")}
        </button>
      </div>
    ) : snapshot.link === "rejected" || snapshot.link === "incompatible" ? (
      <div className="mx-4 rounded-2xl bg-error-wash px-4 py-3">
        <p className="text-sm text-ink-2">
          {t(`link.${snapshot.link}`, { name })}
        </p>
        {snapshot.link === "rejected" && (
          <button
            type="button"
            onClick={onPairAgain}
            className="mt-2 rounded-full border border-line-strong px-4 py-1.5 text-sm font-medium active:bg-wash"
          >
            {t("link.pairAgain")}
          </button>
        )}
      </div>
    ) : null;

  const recording = phase === "recording";
  const busy =
    phase === "starting" || phase === "finishing" || phase === "transcribing";

  return (
    <div className="safe-top safe-bottom flex h-full flex-col">
      <header className="flex h-14 shrink-0 items-center justify-between px-4">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={`h-2.5 w-2.5 shrink-0 rounded-full ${
              connected
                ? "bg-moss"
                : snapshot.link === "connecting"
                  ? "bg-warning"
                  : "bg-error"
            }`}
          />
          <div className="min-w-0">
            <p className="truncate text-base font-semibold">{name}</p>
            <p className="text-xs text-ink-3">
              {connected
                ? t("link.connected")
                : snapshot.link === "connecting"
                  ? t("link.connecting")
                  : snapshot.link === "rejected"
                    ? t("link.notPaired")
                    : snapshot.link === "incompatible"
                      ? t("link.updateNeeded")
                      : t("link.offline")}
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={() => setSettingsOpen(true)}
          aria-label={t("settings.title")}
          className="-me-2 rounded-full p-2 text-ink-2 active:bg-wash"
        >
          <Settings className="h-6 w-6" />
        </button>
      </header>

      {linkBanner}

      <main className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {dictation.transcripts.length === 0 ? (
          <p className="flex h-full items-center justify-center text-center text-base text-ink-3">
            {t("transcripts.empty")}
          </p>
        ) : (
          <>
            <ul className="space-y-3">
              {dictation.transcripts.map((transcript) => (
                <TranscriptCard key={transcript.id} transcript={transcript} />
              ))}
            </ul>
            <button
              type="button"
              onClick={dictation.clearTranscripts}
              className="mx-auto mt-4 flex items-center gap-1.5 rounded-full px-4 py-2 text-sm text-ink-3 active:bg-wash"
            >
              <Trash2 className="h-4 w-4" />
              {t("transcripts.clear")}
            </button>
          </>
        )}
      </main>

      <footer className="shrink-0 border-t border-line px-4 pb-4 pt-3">
        {dictation.notice && (
          <button
            type="button"
            onClick={dictation.dismissNotice}
            role="status"
            className="mb-3 flex w-full items-start justify-between gap-3 rounded-2xl bg-wash px-4 py-3 text-start text-sm text-ink-2"
          >
            <span>{t(`notices.${dictation.notice}`)}</span>
            <X className="mt-0.5 h-4 w-4 shrink-0 text-ink-3" />
          </button>
        )}

        <div
          role="radiogroup"
          aria-label={t("output.label")}
          className="mx-auto flex max-w-xs rounded-full bg-wash p-1"
        >
          {(
            [
              ["desktop", Laptop, t("output.desktop")],
              ["phone", Smartphone, t("output.phone")],
            ] as const
          ).map(([mode, Icon, label]) => (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={output === mode}
              disabled={phase !== "idle"}
              onClick={() => chooseOutput(mode)}
              className={`flex flex-1 items-center justify-center gap-1.5 rounded-full px-3 py-2 text-sm font-medium transition-colors ${
                output === mode
                  ? "bg-background text-text shadow-sm"
                  : "text-ink-3"
              }`}
            >
              <Icon className="h-4 w-4" />
              {label}
            </button>
          ))}
        </div>
        <p className="mt-1.5 text-center text-xs text-ink-3">
          {output === "desktop"
            ? t("output.desktopHint", { name })
            : t("output.phoneHint")}
        </p>

        <div className="mt-4 flex flex-col items-center">
          <div className="relative flex h-28 w-28 items-center justify-center">
            {recording && (
              <span className="pulse-ring absolute inset-2 rounded-full bg-logo-primary" />
            )}
            {recording && (
              <span
                className="absolute inset-2 rounded-full bg-logo-primary/40 transition-transform duration-100"
                style={{
                  transform: `scale(${1 + Math.min(1, dictation.level * 3) * 0.25})`,
                }}
              />
            )}
            <button
              type="button"
              onClick={onButton}
              disabled={!connected || busy}
              aria-label={recording ? t("record.stop") : t("record.start")}
              className={`relative flex h-24 w-24 items-center justify-center rounded-full text-white shadow-lg transition-transform active:scale-95 disabled:opacity-40 ${
                recording ? "bg-error" : "bg-background-ui"
              }`}
            >
              {recording ? (
                <Square className="h-9 w-9 fill-current" />
              ) : busy ? (
                <Loader2 className="h-10 w-10 animate-spin" />
              ) : (
                <Mic className="h-10 w-10" />
              )}
            </button>
          </div>

          <p className="mt-2 text-sm font-medium text-ink-2" aria-live="polite">
            {statusLine}
            {recording && elapsed !== null && (
              <span className="ms-2 tabular-nums text-ink-3">
                {formatElapsed(elapsed)}
              </span>
            )}
          </p>

          <button
            type="button"
            onClick={() => void dictation.cancel()}
            className={`mt-1 rounded-full px-5 py-2 text-sm font-medium text-ink-2 active:bg-wash ${
              phase === "idle" ? "invisible" : ""
            }`}
          >
            {t("record.cancel")}
          </button>
        </div>
      </footer>

      {settingsOpen && (
        <SettingsSheet
          snapshot={snapshot}
          onClose={() => setSettingsOpen(false)}
          onKeyboardSetup={() => {
            setSettingsOpen(false);
            onKeyboardSetup();
          }}
        />
      )}
    </div>
  );
};
