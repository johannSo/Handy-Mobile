import React from "react";
import { useTranslation } from "react-i18next";
import { useRemoteInUse } from "@/stores/remoteStore";

/**
 * A setting the other computer decides while it transcribes this one's
 * dictation: shown, but greyed out and out of reach, with why.
 */
export const DecidedElsewhere: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const { t } = useTranslation();
  const remote = useRemoteInUse();
  if (!remote) return <>{children}</>;
  return (
    <div title={t("settings.remote.decidedElsewhere")}>
      <div
        aria-disabled="true"
        // Out of keyboard reach too.
        ref={(element) => element?.setAttribute("inert", "")}
        className="pointer-events-none opacity-50"
      >
        {children}
      </div>
    </div>
  );
};
