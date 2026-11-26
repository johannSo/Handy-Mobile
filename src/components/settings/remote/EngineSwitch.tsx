import React from "react";
import { useTranslation } from "react-i18next";
import { Laptop, Monitor } from "lucide-react";
import type { TranscriptionEngine } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useModelsLocked } from "@/stores/transcriptionJobsStore";
import { useRemoteStore } from "@/stores/remoteStore";

/**
 * This computer's model, or another computer's. Picking another computer
 * before one is paired opens the pairing dialog instead.
 */
export const EngineSwitch: React.FC<{ onAddComputer: () => void }> = ({
  onAddComputer,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const engine = (getSetting("transcription_engine") ??
    "local") as TranscriptionEngine;
  const paired = useRemoteStore((s) => (s.status?.servers.length ?? 0) > 0);
  const locked = useModelsLocked();
  const updating = isUpdating("transcription_engine");

  const choose = (next: TranscriptionEngine) => {
    if (next === engine || updating) return;
    if (next === "remote" && !paired) {
      onAddComputer();
      return;
    }
    void updateSetting("transcription_engine", next);
  };

  const options: {
    value: TranscriptionEngine;
    label: string;
    icon: React.ElementType;
  }[] = [
    {
      value: "local",
      label: t("settings.remote.engine.thisComputer"),
      icon: Monitor,
    },
    {
      value: "remote",
      label: t("settings.remote.engine.anotherComputer"),
      icon: Laptop,
    },
  ];

  return (
    <div className="px-4 py-2">
      <div className="flex min-h-8 items-center justify-between gap-3">
        <span className="text-sm font-medium">
          {t("settings.remote.engine.title")}
        </span>
        <div
          role="radiogroup"
          aria-label={t("settings.remote.engine.title")}
          className="inline-flex rounded-lg border border-line bg-wash p-0.5"
        >
          {options.map(({ value, label, icon: Icon }) => {
            const active = engine === value;
            return (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={active}
                disabled={locked || updating}
                onClick={() => choose(value)}
                className={`inline-flex cursor-pointer items-center gap-1.5 rounded-md px-3 py-1 text-sm transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary disabled:cursor-not-allowed disabled:opacity-60 ${
                  active
                    ? "bg-background font-medium text-text shadow-sm"
                    : "text-ink-2 hover:text-text"
                }`}
              >
                <Icon className="h-3.5 w-3.5" aria-hidden="true" />
                {label}
              </button>
            );
          })}
        </div>
      </div>
      {/* Why the switch doesn't answer, where it can be read. */}
      {locked && (
        <p className="mt-1 text-end text-xs text-ink-3">
          {t("settings.remote.engine.busy")}
        </p>
      )}
    </div>
  );
};
