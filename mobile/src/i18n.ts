import i18n from "i18next";
import { initReactI18next } from "react-i18next";

// Every file in locales/ is a language, named by its code. Adding one is
// adding a file. English is the source and what a missing string falls
// back to.
const files = import.meta.glob<{ default: Record<string, unknown> }>(
  "./locales/*.json",
  { eager: true },
);
const resources = Object.fromEntries(
  Object.entries(files).map(([path, file]) => [
    path.slice("./locales/".length, -".json".length),
    { translation: file.default },
  ]),
);

const RTL = ["ar", "he"];

/** The phone's language if there's a file for it, else the nearest one. */
const pick = (): string => {
  for (const wanted of navigator.languages ?? [navigator.language]) {
    if (wanted in resources) return wanted;
    const [base, ...rest] = wanted.split("-");
    // Traditional Chinese goes by region or by script.
    if (
      base === "zh" &&
      rest.some((part) => ["TW", "HK", "MO", "Hant"].includes(part)) &&
      "zh-TW" in resources
    ) {
      return "zh-TW";
    }
    if (base in resources) return base;
  }
  return "en";
};

const language = pick();
document.documentElement.lang = language;
document.documentElement.dir = RTL.includes(language) ? "rtl" : "ltr";

void i18n.use(initReactI18next).init({
  resources,
  lng: language,
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
