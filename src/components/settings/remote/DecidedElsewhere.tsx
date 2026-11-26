import React from "react";
import { useTranslation } from "react-i18next";
import { useRemoteInUse } from "@/stores/remoteStore";

/**
 * A setting the other computer decides while it transcribes this one's
 * dictation: shown, but greyed out and out of reach, with why written under
 * it.
 */
export const DecidedElsewhere: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const { t } = useTranslation();
  const remote = useRemoteInUse();
  if (!remote) return <>{children}</>;
  return (
    <div>
      <div
        aria-disabled="true"
        // Out of keyboard reach too.
        ref={(element) => element?.setAttribute("inert", "")}
        className="pointer-events-none opacity-50"
      >
        {children}
      </div>
      <p className="-mt-1 px-4 pb-2 text-xs text-ink-3">
        {t("settings.remote.decidedElsewhere")}
      </p>
    </div>
  );
};

/**
 * A setting for this computer's model, which only transcribes as the
 * fallback while another computer is in use. It still works, and says when
 * it matters.
 */
export const LocalOnly: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const { t } = useTranslation();
  const remote = useRemoteInUse();
  if (!remote) return <>{children}</>;
  return (
    <div>
      {children}
      <p className="-mt-1 px-4 pb-2 text-xs text-ink-3">
        {t("settings.remote.localOnly")}
      </p>
    </div>
  );
};
