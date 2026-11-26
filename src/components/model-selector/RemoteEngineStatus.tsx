import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { remoteServerName, useRemoteStore } from "@/stores/remoteStore";
import { useModelsLocked } from "@/stores/transcriptionJobsStore";
import { commands } from "@/bindings";
import { notify } from "@/stores/noticeStore";
import { getTranslatedModelName } from "../../lib/utils/modelTranslation";
import { goTo } from "@/stores/navigationStore";
import { linkDot, linkText } from "../settings/remote/linkStatus";

/**
 * The footer's engine status while another computer transcribes: which
 * one, whether it's reachable, and its model. The dropdown switches back to
 * this computer or to another paired one.
 */
const RemoteEngineStatus: React.FC = () => {
  const { t } = useTranslation();
  const status = useRemoteStore((s) => s.status);
  const { updateSetting } = useSettings();
  const locked = useModelsLocked();
  const localModel = useModelStore((s) =>
    s.models.find((m) => m.id === s.currentModel && m.is_downloaded),
  );
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, []);

  if (!status) return null;
  const name = remoteServerName(status) ?? "";
  const text =
    status.link === "connected" && status.model && !status.busy
      ? `${name} · ${status.model.name}`
      : `${name} · ${linkText(status, t)}`;

  const select = async (id: string | null) => {
    setOpen(false);
    if (id === null) {
      await updateSetting("transcription_engine", "local");
      return;
    }
    if (id === status.selected) return;
    const result = await commands.selectRemoteComputer(id);
    if (result.status === "error") {
      notify({ title: t("settings.remote.engine.busy"), level: "error" });
    }
  };

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2 transition-colors hover:text-text"
        title={text}
      >
        <div
          className={`h-2 w-2 rounded-full ${
            status.busy && status.link === "connected"
              ? "bg-moss animate-pulse motion-reduce:animate-none"
              : linkDot(status)
          }`}
        />
        <span className="max-w-48 truncate">{text}</span>
        <svg
          className={`h-3 w-3 transition-transform ${open ? "rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {open && (
        <div className="absolute bottom-full start-0 z-50 mb-2 w-64 rounded-lg border border-line-strong bg-background py-1 shadow-lg">
          <p className="px-3 py-1 text-xs font-medium uppercase tracking-wide text-ink-3">
            {t("settings.remote.engine.title")}
          </p>
          <button
            type="button"
            disabled={locked}
            onClick={() => void select(null)}
            className="flex w-full flex-col px-3 py-1.5 text-start text-sm hover:bg-logo-primary/10 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <span>{t("settings.remote.engine.thisComputer")}</span>
            <span className="text-xs text-ink-3">
              {localModel
                ? getTranslatedModelName(localModel, t)
                : t("modelSelector.noModelDownloadRequired")}
            </span>
          </button>
          {status.servers.map((server) => {
            const active = status.selected === server.id;
            return (
              <button
                key={server.id}
                type="button"
                disabled={locked}
                onClick={() => void select(server.id)}
                className={`flex w-full items-center justify-between px-3 py-1.5 text-start text-sm hover:bg-logo-primary/10 disabled:cursor-not-allowed disabled:opacity-50 ${
                  active ? "font-semibold text-logo-primary" : ""
                }`}
              >
                <span className="truncate">{server.name}</span>
              </button>
            );
          })}
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              goTo("remote");
            }}
            className="mt-1 w-full border-t border-line px-3 pt-2 pb-1.5 text-start text-sm text-ink-2 hover:bg-logo-primary/10 hover:text-text"
          >
            {t("footer.remote.manage")}
          </button>
        </div>
      )}
    </div>
  );
};

export default RemoteEngineStatus;
