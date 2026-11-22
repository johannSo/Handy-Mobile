import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";

// English only for now. Every string goes through here, so adding a
// language is adding a file in locales/ and one line below.
void i18n.use(initReactI18next).init({
  resources: { en: { translation: en } },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
