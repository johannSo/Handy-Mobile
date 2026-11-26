import React from "react";
import { useTranslation } from "react-i18next";
import { MonitorSmartphone } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useMobileStore } from "@/stores/mobileStore";
import { goTo } from "@/stores/navigationStore";

/**
 * Says that this computer is listening for remote connections, wherever in
 * the app the user is: who is connected, and who is dictating. Nothing shows
 * while they are off. Opens Handy Remote.
 */
const RemoteConnectionsStatus: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const status = useMobileStore((s) => s.status);
  const enabled = getSetting("mobile_enabled") ?? false;
  if (!enabled || !status) return null;

  const dictating = status.devices.find((device) => device.active);
  const connected = status.devices.filter((device) => device.connected).length;
  const failed = !!status.error;
  const text = failed
    ? t("footer.remote.failed")
    : dictating
      ? t("footer.remote.dictating", { name: dictating.name })
      : connected > 0
        ? t("footer.remote.connected", { count: connected })
        : t("footer.remote.on");

  return (
    <button
      type="button"
      onClick={() => goTo("remote")}
      title={t("settings.remote.open")}
      className={`flex min-w-0 cursor-pointer items-center gap-1.5 transition-colors hover:text-text focus:outline-none focus-visible:ring-1 focus-visible:ring-logo-primary ${failed ? "text-error" : ""}`}
    >
      <MonitorSmartphone
        className={`h-3.5 w-3.5 shrink-0 ${dictating ? "animate-pulse text-moss motion-reduce:animate-none" : connected > 0 && !failed ? "text-moss" : ""}`}
        aria-hidden="true"
      />
      <span className="max-w-40 truncate">{text}</span>
    </button>
  );
};

export default RemoteConnectionsStatus;
