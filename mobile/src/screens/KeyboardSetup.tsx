import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Check } from "lucide-react";
import {
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
}> = ({ number, done, title, description, action, onAction, waiting }) => (
  <li className="flex gap-3 rounded-2xl border border-line px-4 py-3">
    <span
      className={`mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-sm font-semibold ${
        done ? "bg-moss text-background" : "bg-wash text-ink-2"
      }`}
    >
      {done ? <Check className="h-4 w-4" /> : number}
    </span>
    <div className="min-w-0 flex-1">
      <p className="text-base font-medium">{title}</p>
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
    </div>
  </li>
);

interface KeyboardSetupProps {
  onDone: () => void;
  /** Opened from settings, so there is somewhere to go back to. */
  fromSettings: boolean;
}

export const KeyboardSetup: React.FC<KeyboardSetupProps> = ({
  onDone,
  fromSettings,
}) => {
  const { t } = useTranslation();
  const status = useSetupStatus();
  if (!status) return null;

  const steps = [
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
            />
          ))}
        </ol>

        {status.keyboardEnabled && (
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
            <p className="mt-2 text-sm text-ink-3">
              {t("keyboard.switchHint")}
            </p>
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
