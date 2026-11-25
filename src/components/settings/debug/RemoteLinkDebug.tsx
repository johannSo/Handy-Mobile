import React from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { remoteServerName, useRemoteStore } from "@/stores/remoteStore";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/** The link to the computer this one transcribes with, and a way to test
 *  what happens without it. */
export const RemoteLinkDebug: React.FC = () => {
  const { t } = useTranslation();
  const status = useRemoteStore((s) => s.status);
  if (!status || status.servers.length === 0) return null;
  const name = remoteServerName(status) ?? "—";
  return (
    <>
      <SettingContainer
        title={t("settings.debug.remoteLink.title")}
        description={t("settings.debug.remoteLink.description")}
        descriptionMode="tooltip"
        grouped
      >
        <span className="font-mono text-xs text-ink-2">
          {`${name}: ${status.link}${status.busy ? " (busy)" : ""}${
            status.detail && status.link !== "connected"
              ? ` — ${status.detail}`
              : ""
          }`}
        </span>
      </SettingContainer>
      <ToggleSwitch
        checked={status.simulate_offline}
        onChange={(value) => void commands.setRemoteSimulateOffline(value)}
        label={t("settings.debug.remoteLink.simulateOffline")}
        description={t("settings.debug.remoteLink.simulateOfflineDescription")}
        descriptionMode="tooltip"
        grouped
      />
    </>
  );
};
