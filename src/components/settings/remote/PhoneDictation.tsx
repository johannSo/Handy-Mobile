import React from "react";
import { useTranslation } from "react-i18next";
import type { MobileOutputMode } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { goTo } from "@/stores/navigationStore";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { TextLink } from "../../ui/TextLink";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

/** What happens to a paired phone's dictation. Computers aren't affected. */
export const PhoneDictation: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const outputMode = (getSetting("mobile_output_mode") ??
    "desktop") as MobileOutputMode;
  const postProcessAvailable = getSetting("post_process_enabled") ?? false;
  const postProcess = getSetting("mobile_post_process") ?? false;

  return (
    <SettingsGroup title={t("settings.mobile.groups.dictation")}>
      <SettingContainer
        title={t("settings.mobile.outputMode.title")}
        description={t("settings.mobile.outputMode.description")}
        descriptionMode="inline"
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
      <div>
        <ToggleSwitch
          checked={postProcessAvailable && postProcess}
          onChange={(value) => updateSetting("mobile_post_process", value)}
          isUpdating={isUpdating("mobile_post_process")}
          disabled={!postProcessAvailable}
          label={t("settings.mobile.postProcess.label")}
          description={t("settings.mobile.postProcess.description")}
          descriptionMode="tooltip"
          grouped
        />
        {!postProcessAvailable && (
          // Shown rather than hidden: the switch exists, and this is why it
          // can't be used yet.
          <p className="-mt-1 px-4 pb-2 text-xs text-ink-3">
            {t("settings.mobile.postProcess.unavailable")}{" "}
            <TextLink onClick={() => goTo("advanced")}>
              {t("settings.mobile.postProcess.openAdvanced")}
            </TextLink>
          </p>
        )}
      </div>
    </SettingsGroup>
  );
};
