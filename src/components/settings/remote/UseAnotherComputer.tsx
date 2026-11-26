import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Laptop, Pencil, RotateCw, Trash2, X } from "lucide-react";
import { commands, type RemoteServer, type RemoteStatus } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { goTo } from "@/stores/navigationStore";
import { notify } from "@/stores/noticeStore";
import { useRemoteStore } from "@/stores/remoteStore";
import { formatRelativeTime } from "@/utils/dateFormat";
import { Alert } from "../../ui/Alert";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Input } from "../../ui/Input";
import { Radio } from "../../ui/Radio";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { TextLink } from "../../ui/TextLink";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { AddComputerDialog } from "./AddComputerDialog";
import { EngineSwitch } from "./EngineSwitch";
import { linkDot, linkText, reachDot, reachText } from "./linkStatus";

/** How often the paired computers are looked for while the page is up. */
const PROBE_EVERY_MS = 20_000;

const ServerRow: React.FC<{
  server: RemoteServer;
  status: RemoteStatus;
  onUnpair: (server: RemoteServer) => void;
  onPairAgain: () => void;
}> = ({ server, status, onUnpair, onPairAgain }) => {
  const { t, i18n } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(server.name);
  const [details, setDetails] = useState(false);
  const selected = status.in_use && status.selected === server.id;

  const select = async () => {
    if (selected) return;
    const result = await commands.selectRemoteComputer(server.id);
    if (result.status === "error") {
      notify({
        title:
          result.error === "busy"
            ? t("settings.remote.engine.busy")
            : t("settings.remote.row.selectFailed"),
        level: "error",
      });
    }
  };

  const save = async () => {
    setEditing(false);
    const trimmed = name.trim();
    if (trimmed === server.name) return;
    const result = await commands.renameRemoteComputer(server.id, trimmed);
    if (result.status === "error") {
      setName(server.name);
      notify({
        title: t("settings.remote.row.renameFailed"),
        detail: result.error,
        level: "error",
      });
    }
  };

  // The one in use says how its link is; the others, whether they'd answer.
  const stateLine = selected
    ? linkText(status, t)
    : (reachText(server, t) ??
      t("settings.remote.row.pairedAt", {
        time: formatRelativeTime(String(server.paired_at), i18n.language),
      }));
  const dot = selected ? linkDot(status) : reachDot(server);

  return (
    <div className="px-4 py-2">
      <div className="flex items-center gap-3">
        <button
          type="button"
          role="radio"
          aria-checked={selected}
          onClick={() => void select()}
          aria-label={t("settings.remote.row.use", { name: server.name })}
          className="cursor-pointer rounded-full focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary"
        >
          <Radio checked={selected} />
        </button>
        <Laptop
          className={`h-5 w-5 shrink-0 ${selected && status.link === "connected" ? "text-logo-primary" : "text-ink-3"}`}
        />
        <div className="min-w-0 flex-1">
          {editing ? (
            <Input
              autoFocus
              variant="compact"
              value={name}
              maxLength={40}
              placeholder={server.host_name}
              aria-label={t("settings.remote.row.nameLabel")}
              onChange={(event) => setName(event.target.value)}
              onBlur={() => void save()}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
                if (event.key === "Escape") {
                  setName(server.name);
                  setEditing(false);
                }
              }}
              className="w-full max-w-56"
            />
          ) : (
            <p className="truncate text-sm font-medium">{server.name}</p>
          )}
          <p className="flex items-center gap-1.5 truncate text-xs text-ink-3">
            {dot && (
              <span
                aria-hidden="true"
                className={`inline-block h-2 w-2 shrink-0 rounded-full ${dot}`}
              />
            )}
            <span className="truncate">{stateLine}</span>
          </p>
        </div>
        {selected &&
          (status.link === "offline" || status.link === "incompatible") && (
            <button
              type="button"
              onClick={() => void commands.reconnectRemoteComputer()}
              title={t("settings.remote.row.reconnect")}
              aria-label={t("settings.remote.row.reconnect")}
              className="cursor-pointer rounded-md p-1.5 text-ink-3 transition-colors hover:text-logo-primary"
            >
              <RotateCw className="h-4 w-4" />
            </button>
          )}
        {selected && status.link === "rejected" && (
          <Button variant="secondary" size="sm" onClick={onPairAgain}>
            {t("settings.remote.row.pairAgain")}
          </Button>
        )}
        <button
          type="button"
          onClick={() => setDetails((d) => !d)}
          aria-expanded={details}
          className="cursor-pointer rounded-md px-1.5 py-1 text-xs text-ink-3 transition-colors hover:text-text"
        >
          {t("settings.remote.row.details")}
        </button>
        <button
          type="button"
          onClick={() => setEditing(true)}
          title={t("settings.remote.row.rename")}
          aria-label={t("settings.remote.row.rename")}
          className="cursor-pointer rounded-md p-1.5 text-ink-3 transition-colors hover:text-logo-primary"
        >
          <Pencil className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => onUnpair(server)}
          title={t("settings.remote.row.unpair")}
          aria-label={t("settings.remote.row.unpair")}
          className="cursor-pointer rounded-md p-1.5 text-ink-3 transition-colors hover:text-error"
        >
          <Trash2 className="h-4 w-4" />
        </button>
      </div>
      {details && (
        <dl className="ms-[58px] mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs text-ink-3">
          <dt>{t("settings.remote.row.hostName")}</dt>
          <dd className="truncate">{server.host_name}</dd>
          <dt>{t("settings.remote.row.address")}</dt>
          <dd className="truncate font-mono">
            {server.addresses.map((a) => `${a}:${server.port}`).join(", ")}
          </dd>
          <dt>{t("settings.remote.row.fingerprint")}</dt>
          <dd>
            <span className="font-mono select-text">{server.fingerprint}</span>
            <span className="ms-2">
              {t("settings.remote.row.fingerprintHint")}
            </span>
          </dd>
          {selected && status.version && (
            <>
              <dt>{t("settings.remote.row.version")}</dt>
              <dd>{status.version}</dd>
            </>
          )}
          {selected && status.detail && status.link !== "connected" && (
            <>
              <dt>{t("settings.remote.row.lastError")}</dt>
              <dd className="break-words">{status.detail}</dd>
            </>
          )}
        </dl>
      )}
    </div>
  );
};

/**
 * Transcribing with another computer: the switch, the computers this one is
 * paired with, and what happens when the one in use can't be reached.
 */
export const UseAnotherComputer: React.FC = () => {
  const { t } = useTranslation();
  const status = useRemoteStore((s) => s.status);
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const hasLocalModel = useModelStore((s) =>
    s.models.some((m) => m.id === s.currentModel && m.is_downloaded),
  );
  const [addOpen, setAddOpen] = useState(false);
  const [unpairing, setUnpairing] = useState<RemoteServer | null>(null);

  const servers = status?.servers ?? [];
  const paired = servers.length > 0;
  const fallback = getSetting("remote_fallback") ?? true;
  const selected = servers.find((s) => s.id === status?.selected);

  // Whether each paired computer would answer, kept fresh while it's shown.
  useEffect(() => {
    if (!paired) return;
    const probe = () => void commands.probeRemoteComputers().catch(() => {});
    probe();
    const timer = window.setInterval(probe, PROBE_EVERY_MS);
    return () => window.clearInterval(timer);
  }, [paired]);

  const confirmUnpair = async () => {
    if (!unpairing) return;
    const result = await commands.forgetRemoteComputer(unpairing.id);
    if (result.status === "error") {
      notify({
        title:
          result.error === "busy"
            ? t("settings.remote.engine.busy")
            : t("settings.remote.row.unpairFailed"),
        detail: result.error === "busy" ? undefined : result.error,
        level: "error",
      });
    }
    setUnpairing(null);
  };

  return (
    <div className="space-y-2">
      {status?.in_use && status.link === "incompatible" && selected && (
        <Alert variant="warning">
          {t("settings.remote.incompatible", { name: selected.name })}
        </Alert>
      )}

      <SettingsGroup
        title={t("settings.remote.computers.title")}
        description={t("settings.remote.computers.description")}
      >
        <EngineSwitch onAddComputer={() => setAddOpen(true)} />
        {status !== null && paired && (
          <div role="radiogroup">
            {servers.map((server) => (
              <ServerRow
                key={server.id}
                server={server}
                status={status}
                onUnpair={setUnpairing}
                onPairAgain={() => setAddOpen(true)}
              />
            ))}
          </div>
        )}
        <div className="flex items-center justify-between gap-3 px-4 py-2">
          <p className="text-xs text-ink-3">
            {paired
              ? t("settings.remote.computers.hint")
              : t("settings.remote.computers.empty")}
          </p>
          <Button
            variant={paired ? "secondary" : "primary"}
            size="md"
            onClick={() => setAddOpen(true)}
            className="shrink-0"
          >
            {t("settings.remote.computers.add")}
          </Button>
        </div>
        {paired && (
          <div>
            <ToggleSwitch
              checked={fallback}
              onChange={(value) => updateSetting("remote_fallback", value)}
              isUpdating={isUpdating("remote_fallback")}
              label={t("settings.remote.fallback.label")}
              description={t("settings.remote.fallback.description")}
              descriptionMode="inline"
              grouped
            />
            {fallback && !hasLocalModel && (
              <p className="-mt-1 px-4 pb-2 text-xs text-warning">
                {t("settings.remote.fallback.noModel")}{" "}
                <TextLink onClick={() => goTo("models")}>
                  {t("settings.remote.fallback.getModel")}
                </TextLink>
              </p>
            )}
          </div>
        )}
      </SettingsGroup>

      {paired && (
        <p className="px-4 text-xs text-ink-3">{t("settings.remote.stays")}</p>
      )}

      <AddComputerDialog open={addOpen} onClose={() => setAddOpen(false)} />

      <Dialog
        open={unpairing !== null}
        title={t("settings.remote.row.unpairConfirmTitle", {
          name: unpairing?.name ?? "",
        })}
        onOpenChange={(open) => {
          if (!open) setUnpairing(null);
        }}
        closeLabel={t("common.close")}
        contentFades={false}
        className="max-w-sm"
        footer={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setUnpairing(null)}
              className="inline-flex items-center gap-1.5"
            >
              <X className="h-3.5 w-3.5" />
              {t("settings.remote.row.cancel")}
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={() => void confirmUnpair()}
            >
              {t("settings.remote.row.unpair")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-ink-2">
          {t("settings.remote.row.unpairConfirm")}
        </p>
      </Dialog>
    </div>
  );
};
