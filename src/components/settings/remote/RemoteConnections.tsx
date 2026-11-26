import React, { useEffect, useState } from "react";
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
  type DeviceKind,
  type MobileDevice,
  type MobilePairing,
} from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useMobileStore } from "@/stores/mobileStore";
import { notify } from "@/stores/noticeStore";
import { formatRelativeTime } from "@/utils/dateFormat";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Switch } from "../../ui/Switch";
import { copyToClipboard } from "../history/clipboard";

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
          {/* How the code gets to the other computer, and what it's worth. */}
          <p className="text-xs text-ink-3">
            {t("settings.mobile.pair.computerCarry")}
          </p>
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
      <label className="flex cursor-pointer items-center gap-2">
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
      </label>
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

/**
 * The phones and computers paired with this computer, and pairing another.
 */
export const RemoteConnections: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const status = useMobileStore((s) => s.status);
  const refresh = useMobileStore((s) => s.refresh);
  const pairedId = useMobileStore((s) => s.pairedDevice);
  const clearPaired = useMobileStore((s) => s.clearPaired);
  const [pairing, setPairing] = useState<MobilePairing | null>(null);
  const [pairingKind, setPairingKind] = useState<DeviceKind>("phone");
  const [removing, setRemoving] = useState<MobileDevice | null>(null);

  const enabled = getSetting("mobile_enabled") ?? false;
  const devices = status?.devices ?? [];
  // Only a pairing this dialog started is its to announce.
  // (And only once the list has it, so its name is there to show.)
  const pairedName =
    pairing !== null && pairedId !== null
      ? (devices.find((device) => device.id === pairedId)?.name ?? null)
      : null;

  const startPairing = async (kind: DeviceKind) => {
    clearPaired();
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
    clearPaired();
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

  return (
    <>
      <SettingsGroup title={t("settings.mobile.groups.devices")}>
        {devices.length === 0 ? (
          <div className="px-4 py-3 text-sm text-ink-3">
            <p>{t("settings.mobile.devices.empty")}</p>
            <p className="mt-1 text-xs">
              {t("settings.mobile.devices.emptyPhone")}
            </p>
          </div>
        ) : (
          devices.map((device) => (
            <DeviceRow key={device.id} device={device} onRemove={setRemoving} />
          ))
        )}
        {devices.length > 0 && (
          // What the switch on every row means, where it can be read.
          <p className="px-4 py-2 text-xs text-ink-3">
            {t("settings.mobile.devices.canTypeNote")}
          </p>
        )}
        {enabled ? (
          <div className="flex items-center justify-between gap-3 px-4 py-2">
            <p className="text-xs text-ink-3">
              {t("settings.mobile.pair.hint")}
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
        ) : (
          // Nothing can pair while this computer isn't listening, so the
          // way forward is offered instead of two dead buttons.
          <div className="flex items-center justify-between gap-3 px-4 py-2">
            <p className="text-xs text-ink-3">
              {t("settings.mobile.pair.turnOnFirst")}
            </p>
            <Button
              variant="primary"
              size="md"
              disabled={isUpdating("mobile_enabled")}
              onClick={async () => {
                await updateSetting("mobile_enabled", true);
                void refresh();
              }}
              className="shrink-0"
            >
              {t("settings.mobile.pair.turnOn")}
            </Button>
          </div>
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
    </>
  );
};
