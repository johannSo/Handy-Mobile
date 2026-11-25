import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import { commands } from "@/bindings";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Textarea } from "../../ui/Textarea";

/** The codes `pair_remote_computer` fails with. */
const PAIR_ERRORS = [
  "invalid_code",
  "expired_code",
  "incompatible",
  "rate_limited",
  "unreachable",
  "untrusted",
  "self",
  "failed",
] as const;

const pairErrorKey = (code: string) =>
  `settings.remote.add.errors.${
    (PAIR_ERRORS as readonly string[]).includes(code) ? code : "failed"
  }`;

/** The steps and the code field, shared by the dialog and onboarding. */
export const AddComputerForm: React.FC<{
  onPaired: (name: string) => void;
  /** Laid out for a dialog, or for an onboarding screen. */
  compact?: boolean;
}> = ({ onPaired, compact = true }) => {
  const { t } = useTranslation();
  const [code, setCode] = useState("");
  const [pairing, setPairing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pair = async () => {
    const trimmed = code.trim();
    if (!trimmed || pairing) return;
    setPairing(true);
    setError(null);
    const result = await commands.pairRemoteComputer(trimmed);
    setPairing(false);
    if (result.status === "ok") {
      setCode("");
      onPaired(result.data.name);
    } else {
      setError(result.error);
    }
  };

  return (
    <div className={`flex flex-col ${compact ? "gap-3" : "gap-4"}`}>
      <ol className="list-decimal space-y-1 ps-5 text-sm text-ink-2">
        <li>{t("settings.remote.add.steps.open")}</li>
        <li>{t("settings.remote.add.steps.share")}</li>
        <li>{t("settings.remote.add.steps.paste")}</li>
      </ol>
      <Textarea
        variant="compact"
        value={code}
        onChange={(event) => {
          setCode(event.target.value);
          setError(null);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            void pair();
          }
        }}
        placeholder={t("settings.remote.add.placeholder")}
        aria-label={t("settings.remote.add.codeLabel")}
        spellCheck={false}
        className="w-full font-mono text-xs"
      />
      {error && (
        <p role="alert" className="text-sm text-error">
          {t(pairErrorKey(error))}
        </p>
      )}
      <div className="flex justify-end">
        <Button
          variant="primary"
          size="md"
          onClick={() => void pair()}
          disabled={!code.trim() || pairing}
        >
          {pairing
            ? t("settings.remote.add.pairing")
            : t("settings.remote.add.pair")}
        </Button>
      </div>
    </div>
  );
};

export const AddComputerDialog: React.FC<{
  open: boolean;
  onClose: () => void;
}> = ({ open, onClose }) => {
  const { t } = useTranslation();
  const [pairedName, setPairedName] = useState<string | null>(null);

  useEffect(() => {
    if (open) setPairedName(null);
  }, [open]);

  return (
    <Dialog
      open={open}
      title={t("settings.remote.add.title")}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      closeLabel={t("common.close")}
      contentFades={false}
      className="max-w-md"
    >
      {pairedName !== null ? (
        <div className="flex flex-col items-center gap-3 py-6 text-center">
          <div className="rounded-full bg-logo-primary/20 p-3">
            <Check className="h-6 w-6" />
          </div>
          <p className="text-sm font-medium">
            {t("settings.remote.add.done", { name: pairedName })}
          </p>
          <Button variant="secondary" size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        </div>
      ) : (
        <AddComputerForm onPaired={setPairedName} />
      )}
    </Dialog>
  );
};
