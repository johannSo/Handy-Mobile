import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useMobileStore } from "@/stores/mobileStore";
import { goTo } from "@/stores/navigationStore";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Input } from "../../ui/Input";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { TextLink } from "../../ui/TextLink";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

const MIN_PORT = 1024;
const MAX_PORT = 65535;

/**
 * The port devices connect to. Changing it strands every paired device,
 * which looks for this computer on the old one, so that is asked first.
 */
const PortSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const refresh = useMobileStore((s) => s.refresh);
  const paired = useMobileStore((s) => (s.status?.devices.length ?? 0) > 0);
  const port = getSetting("mobile_port") ?? 47823;
  const [draft, setDraft] = useState(String(port));
  const [asking, setAsking] = useState<number | null>(null);

  useEffect(() => setDraft(String(port)), [port]);

  const apply = async (value: number) => {
    await updateSetting("mobile_port", value);
    void refresh();
  };

  const commit = () => {
    const value = parseInt(draft, 10);
    if (isNaN(value) || value < MIN_PORT || value > MAX_PORT) {
      setDraft(String(port));
      return;
    }
    if (value === port) return;
    if (paired) setAsking(value);
    else void apply(value);
  };

  const cancel = () => {
    setAsking(null);
    setDraft(String(port));
  };

  return (
    <>
      <SettingContainer
        title={t("settings.mobile.port.title")}
        description={t("settings.mobile.port.description")}
        descriptionMode="tooltip"
        grouped
      >
        <Input
          type="number"
          min={MIN_PORT}
          max={MAX_PORT}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
          disabled={isUpdating("mobile_port")}
          className="w-24"
        />
      </SettingContainer>
      <Dialog
        open={asking !== null}
        title={t("settings.mobile.port.confirmTitle", { port: asking ?? "" })}
        onOpenChange={(open) => {
          if (!open) cancel();
        }}
        closeLabel={t("common.close")}
        contentFades={false}
        className="max-w-sm"
        footer={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={cancel}
              className="inline-flex items-center gap-1.5"
            >
              <X className="h-3.5 w-3.5" />
              {t("settings.mobile.devices.cancel")}
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={() => {
                const value = asking;
                setAsking(null);
                if (value !== null) void apply(value);
              }}
            >
              {t("settings.mobile.port.change")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-ink-2">
          {t("settings.mobile.port.confirm")}
        </p>
      </Dialog>
    </>
  );
};

/**
 * Letting phones and other computers dictate with this computer's model:
 * the switch, where it listens, and what it transcribes with.
 */
export const ShareThisComputer: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const status = useMobileStore((s) => s.status);
  const refresh = useMobileStore((s) => s.refresh);
  const enabled = getSetting("mobile_enabled") ?? false;

  const failed = enabled && !!status?.error;
  const listening =
    enabled && !failed && !!status?.running && status.addresses.length > 0;
  const statusText = !enabled
    ? t("settings.mobile.status.off")
    : status?.error
      ? t("settings.mobile.status.failed", { error: status.error })
      : !status?.running
        ? t("settings.mobile.status.starting")
        : status.addresses.length === 0
          ? t("settings.mobile.status.noNetwork")
          : t("settings.mobile.status.listening", {
              address: `${status.addresses[0]}:${status.port}`,
            });
  // A computer with a VPN or a second network card has more than one.
  const otherAddresses = listening ? status.addresses.slice(1) : [];

  return (
    <SettingsGroup
      title={t("settings.mobile.groups.connection")}
      description={t("settings.mobile.intro")}
    >
      <ToggleSwitch
        checked={enabled}
        onChange={async (value) => {
          await updateSetting("mobile_enabled", value);
          void refresh();
        }}
        isUpdating={isUpdating("mobile_enabled")}
        label={t("settings.mobile.enabled.label")}
        description={t("settings.mobile.enabled.description")}
        descriptionMode="inline"
        grouped
      />
      <SettingContainer
        title={t("settings.mobile.status.title")}
        description={t("settings.mobile.status.description")}
        descriptionMode="tooltip"
        grouped
      >
        <div className="text-end">
          <p className={`text-sm ${failed ? "text-error" : "text-ink-2"}`}>
            {statusText}
          </p>
          {otherAddresses.length > 0 && (
            <p className="text-xs text-ink-3">
              {t("settings.mobile.status.alsoAt", {
                addresses: otherAddresses.join(", "),
              })}
            </p>
          )}
        </div>
      </SettingContainer>
      <SettingContainer
        title={t("settings.mobile.model.title")}
        description={t("settings.mobile.model.description")}
        descriptionMode="tooltip"
        grouped
      >
        {status && !status.model_name ? (
          <span className="flex items-center gap-2 text-sm text-warning">
            {t("settings.mobile.model.none")}
            <TextLink onClick={() => goTo("models")}>
              {t("settings.mobile.model.download")}
            </TextLink>
          </span>
        ) : (
          <TextLink onClick={() => goTo("models")} className="!text-sm">
            {status?.model_name ?? ""}
          </TextLink>
        )}
      </SettingContainer>
      {status?.fingerprint && (
        <SettingContainer
          title={t("settings.mobile.certificate.title")}
          description={t("settings.mobile.certificate.description")}
          descriptionMode="tooltip"
          grouped
        >
          <span className="font-mono text-xs text-ink-2 select-text">
            {status.fingerprint}
          </span>
        </SettingContainer>
      )}
      <PortSetting />
    </SettingsGroup>
  );
};
