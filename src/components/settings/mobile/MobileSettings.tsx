import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  Copy,
  Laptop,
  Pencil,
  Smartphone,
  Trash2,
  X,
} from "lucide-react";
import {
  commands,
  events,
  type DeviceKind,
  type MobileDevice,
  type MobileOutputMode,
  type MobilePairing,
  type MobileStatus,
} from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { notify } from "@/stores/noticeStore";
import { formatRelativeTime } from "@/utils/dateFormat";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Dropdown } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Switch } from "../../ui/Switch";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { copyToClipboard } from "../history/clipboard";

const MIN_PORT = 1024;
const MAX_PORT = 65535;

/** The Mobile tab's view of the backend, refreshed whenever it changes. */
const useMobileStatus = (onPaired: (deviceId: string) => void) => {
  const [status, setStatus] = useState<MobileStatus | null>(null);
  const onPairedRef = useRef(onPaired);
  onPairedRef.current = onPaired;

  const refresh = useCallback(async () => {
    setStatus(await commands.getMobileStatus());
  }, []);

  useEffect(() => {
    let cancelled = false;
    void refresh();
    const unlisten = events.mobileLinkChanged.listen((event) => {
      if (cancelled) return;
      void refresh();
      if (event.payload.paired_device) {
        onPairedRef.current(event.payload.paired_device);
      }
    });
    return () => {
      cancelled = true;
      void unlisten.then((stop) => stop());
    };
  }, [refresh]);

  return { status, refresh };
};

const PortSetting: React.FC<{ onChanged: () => void }> = ({ onChanged }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const port = getSetting("mobile_port") ?? 47823;
  const [draft, setDraft] = useState(String(port));

  useEffect(() => setDraft(String(port)), [port]);

  const commit = async () => {
    const value = parseInt(draft, 10);
    if (isNaN(value) || value < MIN_PORT || value > MAX_PORT) {
      setDraft(String(port));
      return;
    }
    if (value !== port) {
      await updateSetting("mobile_port", value);
      onChanged();
    }
  };

  return (
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
        onBlur={() => void commit()}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
        disabled={isUpdating("mobile_port")}
        className="w-24"
      />
    </SettingContainer>
  );
};

const PairingDialog: React.FC<{
  pairing: MobilePairing | null;
  /** What the code is for: a phone scans it, a computer pastes it. */
  kind: DeviceKind;
  pairedName: string | null;
  onNewCode: () => void;
  onClose: () => void;
}> = ({ pairing, kind, pairedName, onNewCode, onClose }) => {
  const { t } = useTranslation();
  const [secondsLeft, setSecondsLeft] = useState(0);

  useEffect(() => {
    if (!pairing) return;
    const expiresAt = Date.now() + pairing.expires_in_secs * 1000;
    const tick = () =>
      setSecondsLeft(Math.max(0, Math.ceil((expiresAt - Date.now()) / 1000)));
    tick();
    const timer = window.setInterval(tick, 1000);
    return () => window.clearInterval(timer);
  }, [pairing]);

  const expired = secondsLeft === 0;
  const minutes = Math.floor(secondsLeft / 60);
  const seconds = String(secondsLeft % 60).padStart(2, "0");

  const copyCode = async () => {
    if (!pairing) return;
    const copied = await copyToClipboard(pairing.code);
    notify({
      title: copied
        ? t("settings.mobile.pair.copied")
        : t("settings.mobile.pair.copyFailed"),
      level: copied ? undefined : "error",
    });
  };

  return (
    <Dialog
      open={pairing !== null}
      title={
        kind === "computer"
          ? t("settings.mobile.pair.computerTitle")
          : t("settings.mobile.pair.title")
      }
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      closeLabel={t("common.close")}
      contentFades={false}
      className="max-w-sm"
    >
      {pairedName !== null ? (
        <div className="flex flex-col items-center gap-3 py-6 text-center">
          <div className="rounded-full bg-logo-primary/20 p-3">
            <Check className="h-6 w-6" />
          </div>
          <p className="text-sm font-medium">
            {t("settings.mobile.pair.done", { name: pairedName })}
          </p>
          <Button variant="secondary" size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        </div>
      ) : pairing && kind === "computer" ? (
        <div className="flex flex-col gap-3">
          <p className="text-sm text-ink-2">
            {t("settings.mobile.pair.computerDescription")}
          </p>
          <code
            className={`block max-h-24 overflow-y-auto break-all rounded-md border border-line bg-wash px-2 py-1.5 font-mono text-xs select-all ${expired ? "opacity-30" : ""}`}
          >
            {pairing.code}
          </code>
          <div className="flex items-center justify-between gap-3">
            <p className="text-xs text-ink-3">
              {expired
                ? t("settings.mobile.pair.expired")
                : t("settings.mobile.pair.expires", {
                    time: `${minutes}:${seconds}`,
                  })}
            </p>
            {expired ? (
              <Button variant="primary" size="sm" onClick={onNewCode}>
                {t("settings.mobile.pair.newCode")}
              </Button>
            ) : (
              <Button
                variant="primary"
                size="sm"
                onClick={() => void copyCode()}
                className="inline-flex items-center gap-1.5"
              >
                <Copy className="h-3.5 w-3.5" />
                {t("settings.mobile.pair.copy")}
              </Button>
            )}
          </div>
          <p className="text-xs text-ink-3">
            {t("settings.mobile.pair.computerNote")}
          </p>
        </div>
      ) : (
        pairing && (
          <div className="flex flex-col items-center gap-3 text-center">
            <p className="text-sm text-ink-2">
              {t("settings.mobile.pair.description")}
            </p>
            <div className="relative rounded-lg bg-white p-2">
              <img
                src={`data:image/svg+xml;utf8,${encodeURIComponent(pairing.qr_svg)}`}
                alt={t("settings.mobile.pair.qrAlt")}
                className={`h-56 w-56 ${expired ? "opacity-10" : ""}`}
              />
              {expired && (
                <div className="absolute inset-0 flex items-center justify-center">
                  <Button variant="primary" size="sm" onClick={onNewCode}>
                    {t("settings.mobile.pair.newCode")}
                  </Button>
                </div>
              )}
            </div>
            <p className="text-xs text-ink-3">
              {expired
                ? t("settings.mobile.pair.expired")
                : t("settings.mobile.pair.expires", {
                    time: `${minutes}:${seconds}`,
                  })}
            </p>
            <div className="flex flex-col items-center gap-1.5 border-t border-line pt-3 w-full">
              <p className="text-xs text-ink-3">
                {t("settings.mobile.pair.manual")}
              </p>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void copyCode()}
                disabled={expired}
                className="inline-flex items-center gap-1.5"
              >
                <Copy className="h-3.5 w-3.5" />
                {t("settings.mobile.pair.copy")}
              </Button>
            </div>
          </div>
        )
      )}
    </Dialog>
  );
};

const DeviceRow: React.FC<{
  device: MobileDevice;
  onRemove: (device: MobileDevice) => void;
}> = ({ device, onRemove }) => {
  const { t, i18n } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(device.name);

  const save = async () => {
    const trimmed = name.trim();
    setEditing(false);
    if (!trimmed || trimmed === device.name) {
      setName(device.name);
      return;
    }
    const result = await commands.renameMobileDevice(device.id, trimmed);
    if (result.status === "error") {
      setName(device.name);
      notify({
        title: t("settings.mobile.devices.renameFailed"),
        detail: result.error,
        level: "error",
      });
    }
  };

  const setCanType = async (canType: boolean) => {
    const result = await commands.setMobileDeviceCanType(device.id, canType);
    if (result.status === "error") {
      notify({
        title: t("settings.mobile.devices.canTypeFailed"),
        detail: result.error,
        level: "error",
      });
    }
  };

  const DeviceIcon = device.kind === "computer" ? Laptop : Smartphone;
  const state = device.active
    ? device.kind === "computer"
      ? t("settings.mobile.devices.transcribing")
      : t("settings.mobile.devices.dictating")
    : device.connected
      ? t("settings.mobile.devices.connected")
      : device.last_seen !== null
        ? t("settings.mobile.devices.lastSeen", {
            time: formatRelativeTime(String(device.last_seen), i18n.language),
          })
        : t("settings.mobile.devices.never");

  return (
    <div className="flex items-center gap-3 px-4 py-2">
      <DeviceIcon
        className={`h-5 w-5 shrink-0 ${device.connected ? "text-logo-primary" : "text-ink-3"}`}
      />
      <div className="min-w-0 flex-1">
        {editing ? (
          <Input
            autoFocus
            variant="compact"
            value={name}
            maxLength={40}
            aria-label={t("settings.mobile.devices.nameLabel")}
            onChange={(event) => setName(event.target.value)}
            onBlur={() => void save()}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
              if (event.key === "Escape") {
                setName(device.name);
                setEditing(false);
              }
            }}
            className="w-full max-w-56"
          />
        ) : (
          <p className="truncate text-sm font-medium">{device.name}</p>
        )}
        <p className="truncate text-xs text-ink-3">{state}</p>
      </div>
      <div
        className="flex items-center gap-2"
        title={t("settings.mobile.devices.canTypeDescription")}
      >
        <span className="text-xs text-ink-3">
          {t("settings.mobile.devices.canType")}
        </span>
        <Switch
          checked={device.can_type}
          onChange={(value) => void setCanType(value)}
          ariaLabel={t("settings.mobile.devices.canTypeLabel", {
            name: device.name,
          })}
        />
      </div>
      <button
        type="button"
        onClick={() => setEditing(true)}
        title={t("settings.mobile.devices.rename")}
        aria-label={t("settings.mobile.devices.rename")}
        className="cursor-pointer rounded-md p-1.5 text-ink-3 transition-colors hover:text-logo-primary"
      >
        <Pencil className="h-4 w-4" />
      </button>
      <button
        type="button"
        onClick={() => onRemove(device)}
        title={t("settings.mobile.devices.remove")}
        aria-label={t("settings.mobile.devices.remove")}
        className="cursor-pointer rounded-md p-1.5 text-ink-3 transition-colors hover:text-error"
      >
        <Trash2 className="h-4 w-4" />
      </button>
    </div>
  );
};

export const MobileSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [pairing, setPairing] = useState<MobilePairing | null>(null);
  const [pairingKind, setPairingKind] = useState<DeviceKind>("phone");
  const [pairedId, setPairedId] = useState<string | null>(null);
  const [removing, setRemoving] = useState<MobileDevice | null>(null);

  const { status, refresh } = useMobileStatus(setPairedId);

  const enabled = getSetting("mobile_enabled") ?? false;
  const outputMode = (getSetting("mobile_output_mode") ??
    "desktop") as MobileOutputMode;
  const postProcessAvailable = getSetting("post_process_enabled") ?? false;
  const postProcess = getSetting("mobile_post_process") ?? false;

  const devices = status?.devices ?? [];
  const pairedName =
    pairedId !== null
      ? (devices.find((device) => device.id === pairedId)?.name ?? "")
      : null;

  const startPairing = async (kind: DeviceKind) => {
    setPairedId(null);
    setPairingKind(kind);
    const result = await commands.startMobilePairing(kind);
    if (result.status === "ok") {
      setPairing(result.data);
    } else {
      notify({
        title: t("settings.mobile.pair.failed"),
        detail: result.error,
        level: "error",
      });
    }
  };

  const closePairing = () => {
    setPairing(null);
    setPairedId(null);
    void commands.cancelMobilePairing();
  };

  const confirmRemove = async () => {
    if (!removing) return;
    const result = await commands.removeMobileDevice(removing.id);
    if (result.status === "error") {
      notify({
        title: t("settings.mobile.devices.removeFailed"),
        detail: result.error,
        level: "error",
      });
    }
    setRemoving(null);
  };

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

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
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
          descriptionMode="tooltip"
          grouped
          tooltipPosition="bottom"
        />
        <SettingContainer
          title={t("settings.mobile.status.title")}
          description={t("settings.mobile.status.description")}
          descriptionMode="tooltip"
          grouped
        >
          <span
            className={`text-sm ${enabled && status?.error ? "text-error" : "text-ink-2"}`}
          >
            {statusText}
          </span>
        </SettingContainer>
        <PortSetting onChanged={() => void refresh()} />
        <SettingContainer
          title={t("settings.mobile.model.title")}
          description={t("settings.mobile.model.description")}
          descriptionMode="tooltip"
          grouped
        >
          <span
            className={`text-sm ${status && !status.model_name ? "text-warning" : "text-ink-2"}`}
          >
            {status?.model_name ?? t("settings.mobile.model.none")}
          </span>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t("settings.mobile.groups.devices")}>
        {devices.length === 0 ? (
          <p className="px-4 py-3 text-sm text-ink-3">
            {t("settings.mobile.devices.empty")}
          </p>
        ) : (
          devices.map((device) => (
            <DeviceRow key={device.id} device={device} onRemove={setRemoving} />
          ))
        )}
        <div className="flex items-center justify-between gap-3 px-4 py-2">
          <p className="text-xs text-ink-3">
            {status?.running
              ? t("settings.mobile.pair.hint")
              : t("settings.mobile.pair.turnOnFirst")}
          </p>
          <div className="flex shrink-0 gap-2">
            <Button
              variant="secondary"
              size="md"
              onClick={() => void startPairing("computer")}
              disabled={!status?.running}
            >
              {t("settings.mobile.pair.computerButton")}
            </Button>
            <Button
              variant="primary"
              size="md"
              onClick={() => void startPairing("phone")}
              disabled={!status?.running}
            >
              {t("settings.mobile.pair.button")}
            </Button>
          </div>
        </div>
      </SettingsGroup>

      <SettingsGroup title={t("settings.mobile.groups.dictation")}>
        <SettingContainer
          title={t("settings.mobile.outputMode.title")}
          description={t("settings.mobile.outputMode.description")}
          descriptionMode="tooltip"
          grouped
        >
          <Dropdown
            options={[
              {
                value: "desktop",
                label: t("settings.mobile.outputMode.options.desktop"),
              },
              {
                value: "phone",
                label: t("settings.mobile.outputMode.options.phone"),
              },
            ]}
            selectedValue={outputMode}
            onSelect={(value) =>
              updateSetting("mobile_output_mode", value as MobileOutputMode)
            }
            disabled={isUpdating("mobile_output_mode")}
          />
        </SettingContainer>
        {postProcessAvailable && (
          <ToggleSwitch
            checked={postProcess}
            onChange={(value) => updateSetting("mobile_post_process", value)}
            isUpdating={isUpdating("mobile_post_process")}
            label={t("settings.mobile.postProcess.label")}
            description={t("settings.mobile.postProcess.description")}
            descriptionMode="tooltip"
            grouped
          />
        )}
      </SettingsGroup>

      <PairingDialog
        pairing={pairing}
        kind={pairingKind}
        pairedName={pairedName}
        onNewCode={() => void startPairing(pairingKind)}
        onClose={closePairing}
      />

      <Dialog
        open={removing !== null}
        title={t("settings.mobile.devices.removeConfirmTitle", {
          name: removing?.name ?? "",
        })}
        onOpenChange={(open) => {
          if (!open) setRemoving(null);
        }}
        closeLabel={t("common.close")}
        contentFades={false}
        className="max-w-sm"
        footer={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setRemoving(null)}
              className="inline-flex items-center gap-1.5"
            >
              <X className="h-3.5 w-3.5" />
              {t("settings.mobile.devices.cancel")}
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={() => void confirmRemove()}
            >
              {t("settings.mobile.devices.remove")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-ink-2">
          {t("settings.mobile.devices.removeConfirm")}
        </p>
      </Dialog>
    </div>
  );
};
