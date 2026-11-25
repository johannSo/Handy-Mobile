import React from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../ui/Button";
import { OnboardingShell, ScreenHeader, ScreenMain } from "../OnboardingShell";
import { AddComputerForm } from "../../settings/remote/AddComputerDialog";
import { linkText } from "../../settings/remote/RemoteEnginePanel";
import { remoteServerName, useRemoteStore } from "@/stores/remoteStore";

interface ConnectProps {
  progress?: { index: number; count: number };
  /** A preview never pairs. */
  preview?: boolean;
  onBack: () => void;
  onContinue: () => void;
}

/**
 * Instead of a model here: pair with another computer that shares its own,
 * and transcribe with that one.
 */
export const Connect: React.FC<ConnectProps> = ({
  progress,
  preview = false,
  onBack,
  onContinue,
}) => {
  const { t } = useTranslation();
  const status = useRemoteStore((s) => s.status);
  const paired = preview || (status?.in_use ?? false);
  const name = status ? remoteServerName(status) : null;

  const hint =
    paired && status && name
      ? `${name} · ${linkText(status, t)}`
      : t("onboarding.connect.hint");

  return (
    <OnboardingShell
      progress={progress}
      onBack={onBack}
      hint={hint}
      primary={
        <Button
          variant="primary"
          size="lg"
          onClick={onContinue}
          disabled={!paired}
        >
          {t("onboarding.shell.continue")}
        </Button>
      }
    >
      <ScreenMain
        align="stretch"
        header={
          <ScreenHeader
            title={t("onboarding.connect.title")}
            sub={t("onboarding.connect.subtitle")}
          />
        }
      >
        <div className="mx-auto w-full max-w-[460px] text-start">
          {paired && name ? (
            <p className="rounded-card border border-line bg-wash px-4 py-3 text-small">
              {t("onboarding.connect.paired", { name })}
            </p>
          ) : (
            <AddComputerForm compact={false} onPaired={() => {}} />
          )}
        </div>
      </ScreenMain>
    </OnboardingShell>
  );
};
