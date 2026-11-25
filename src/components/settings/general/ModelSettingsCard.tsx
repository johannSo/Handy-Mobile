import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { LanguageSelector } from "../LanguageSelector";
import { TranslateToEnglish } from "../TranslateToEnglish";
import { useModelStore } from "../../../stores/modelStore";
import { remoteServerName, useRemoteStore } from "../../../stores/remoteStore";
import type { ModelInfo } from "@/bindings";

/** The language and translation of the model another computer uses. */
const RemoteModelSettingsCard: React.FC = () => {
  const { t } = useTranslation();
  const status = useRemoteStore((s) => s.status);
  const model = status?.model;
  if (!status || !model) return null;
  const showLanguageSelector = model.supports_language_selection;
  if (!showLanguageSelector && !model.supports_translation) return null;
  return (
    <SettingsGroup
      title={t("settings.modelSettings.remoteTitle", {
        model: model.name,
        computer: remoteServerName(status) ?? "",
      })}
    >
      {showLanguageSelector && (
        <LanguageSelector
          descriptionMode="tooltip"
          grouped={true}
          modelId={model.id}
          supportedLanguages={model.languages}
          remoteAuto={model.supports_language_detection}
        />
      )}
      {model.supports_translation && (
        <TranslateToEnglish descriptionMode="tooltip" grouped={true} />
      )}
    </SettingsGroup>
  );
};

export const ModelSettingsCard: React.FC = () => {
  const { t } = useTranslation();
  const { currentModel, models } = useModelStore();
  const remote = useRemoteStore((s) => s.status?.in_use ?? false);

  const currentModelInfo = models.find((m: ModelInfo) => m.id === currentModel);

  const showLanguageSelector =
    currentModelInfo?.supports_language_selection ?? false;
  const supportsTranslation = currentModelInfo?.supports_translation ?? false;
  const hasAnySettings = showLanguageSelector || supportsTranslation;

  if (remote) return <RemoteModelSettingsCard />;

  // Don't render anything if no model is selected or no settings available
  if (!currentModel || !currentModelInfo || !hasAnySettings) {
    return null;
  }

  return (
    <SettingsGroup
      title={t("settings.modelSettings.title", {
        model: currentModelInfo.name,
      })}
    >
      {showLanguageSelector && (
        <LanguageSelector
          descriptionMode="tooltip"
          grouped={true}
          modelId={currentModelInfo.id}
          supportedLanguages={currentModelInfo.supported_languages}
        />
      )}
      {supportsTranslation && (
        <TranslateToEnglish descriptionMode="tooltip" grouped={true} />
      )}
    </SettingsGroup>
  );
};
