import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Check } from "lucide-react";
import {
  openAppSettings,
  openBubbleSettings,
  openKeyboardSettings,
  requestPermission,
  setupStatus,
  showKeyboardPicker,
  type SetupStatus,
} from "../lib/setup";

/** Where setup stands, re-read while the screen is up. */
const useSetupStatus = (): SetupStatus | null => {
  const [status, setStatus] = useState<SetupStatus | null>(setupStatus);
  useEffect(() => {
    // Polled: nothing announces that a keyboard was switched on or chosen,
    // and the chooser doesn't even take the app off screen.
    const timer = window.setInterval(() => setStatus(setupStatus()), 700);
    return () => window.clearInterval(timer);
  }, []);
  return status;
};

const Step: React.FC<{
  number: number;
  done: boolean;
  title: string;
  description: string;
  action: string;
  onAction: () => void;
  /** An earlier step has to be done first. */
  waiting?: boolean;
  /** Marks a step added since the user last saw the setup. */
  badge?: string;
  /** What to do when the step's own button gets nowhere. */
  help?: { title: string; text: string; action: string; onAction: () => void };
}> = ({
  number,
  done,
  title,
  description,
  action,
  onAction,
  waiting,
  badge,
  help,
}) => (
  <li className="flex gap-3 rounded-2xl border border-line px-4 py-3">
    <span
      className={`mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-sm font-semibold ${
        done ? "bg-moss text-background" : "bg-wash text-ink-2"
      }`}
    >
      {done ? <Check className="h-4 w-4" /> : number}
    </span>
    <div className="min-w-0 flex-1">
      <p className="text-base font-medium">
        {title}
        {badge && !done && (
          <span className="ms-2 rounded-full bg-background-ui px-2 py-0.5 align-middle text-xs font-semibold text-white">
            {badge}
          </span>
        )}
      </p>
      <p className="mt-0.5 text-sm text-ink-2">{description}</p>
      {!done && (
        <button
          type="button"
          onClick={onAction}
          disabled={waiting}
          className="mt-3 rounded-full bg-background-ui px-4 py-2 text-sm font-semibold text-white active:opacity-80 disabled:opacity-40"
        >
          {action}
        </button>
      )}
      {!done && help && (
        <details className="mt-3 text-sm text-ink-2">
          <summary className="font-medium">{help.title}</summary>
          <p className="mt-1">{help.text}</p>
          <button
            type="button"
            onClick={help.onAction}
            className="mt-2 rounded-full border border-line-strong px-4 py-2 text-sm font-semibold active:bg-wash"
          >
            {help.action}
          </button>
        </details>
      )}
    </div>
  </li>
);

interface KeyboardSetupProps {
  onDone: () => void;
  /** Opened from settings, so there is somewhere to go back to. */
  fromSettings: boolean;
  /** The user set the keyboard up before the bubble existed. */
  highlightBubble?: boolean;
}

export const KeyboardSetup: React.FC<KeyboardSetupProps> = ({
  onDone,
  fromSettings,
  highlightBubble,
}) => {
  const { t } = useTranslation();
  const status = useSetupStatus();
  if (!status) return null;

  const steps: {
    key: string;
    done: boolean;
    onAction: () => void;
    waiting?: boolean;
    badge?: string;
    help?: React.ComponentProps<typeof Step>["help"];
  }[] = [
    {
      key: "microphone",
      done: status.microphone,
      onAction: () => requestPermission("microphone"),
    },
    ...(status.localNetworkNeeded
      ? [
          {
            key: "localNetwork",
            done: status.localNetwork,
            onAction: () => requestPermission("localNetwork"),
          },
        ]
      : []),
    {
      key: "bubble",
      done: status.bubbleEnabled,
      onAction: () => openBubbleSettings(),
      badge: highlightBubble ? t("keyboard.new") : undefined,
      help: {
        title: t("keyboard.steps.bubble.helpTitle"),
        text: t("keyboard.steps.bubble.help"),
        action: t("keyboard.steps.bubble.helpAction"),
        onAction: () => openAppSettings(),
      },
    },
    {
      key: "enable",
      done: status.keyboardEnabled,
      onAction: () => openKeyboardSettings(),
    },
    {
      key: "select",
      done: status.keyboardSelected,
      onAction: () => showKeyboardPicker(),
      waiting: !status.keyboardEnabled,
    },
  ];
  const allDone = steps.every((step) => step.done);

  return (
    <div className="safe-top safe-bottom flex h-full flex-col">
      <header className="flex h-14 shrink-0 items-center px-2">
        {fromSettings && (
          <button
            type="button"
            onClick={onDone}
            aria-label={t("keyboard.back")}
            className="rounded-full p-2 text-ink-2 active:bg-wash"
          >
            <ArrowLeft className="h-6 w-6" />
          </button>
        )}
      </header>

      <main className="min-h-0 flex-1 overflow-y-auto px-6 pb-4">
        <h1 className="text-2xl font-semibold">{t("keyboard.title")}</h1>
        <p className="mt-2 text-base text-ink-2">{t("keyboard.intro")}</p>

        <ol className="mt-6 space-y-3">
          {steps.map((step, index) => (
            <Step
              key={step.key}
              number={index + 1}
              done={step.done}
              title={t(`keyboard.steps.${step.key}.title`)}
              description={t(`keyboard.steps.${step.key}.description`)}
              action={t(`keyboard.steps.${step.key}.action`)}
              onAction={step.onAction}
              waiting={step.waiting}
              badge={step.badge}
              help={step.help}
            />
          ))}
        </ol>

        {(status.keyboardEnabled || status.bubbleEnabled) && (
          <div className="mt-6">
            <label htmlFor="keyboard-try" className="text-sm font-medium">
              {t("keyboard.tryLabel")}
            </label>
            <textarea
              id="keyboard-try"
              rows={3}
              placeholder={t("keyboard.tryPlaceholder")}
              className="selectable mt-2 w-full resize-none rounded-2xl border border-line-strong bg-transparent px-4 py-3 text-base outline-none focus:border-background-ui"
            />
            {status.bubbleEnabled && (
              <p className="mt-2 text-sm text-ink-3">
                {t("keyboard.bubbleHint")}
              </p>
            )}
            {status.keyboardEnabled && (
              <p className="mt-2 text-sm text-ink-3">
                {t("keyboard.switchHint")}
              </p>
            )}
          </div>
        )}
      </main>

      <footer className="shrink-0 px-6 pb-4 pt-2">
        <button
          type="button"
          onClick={onDone}
          className={`w-full rounded-2xl px-4 py-3 text-base font-semibold active:opacity-80 ${
            allDone
              ? "bg-background-ui text-white"
              : "border border-line-strong text-text"
          }`}
        >
          {allDone ? t("keyboard.done") : t("keyboard.later")}
        </button>
      </footer>
    </div>
  );
};
