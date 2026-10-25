import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import ar from "./ar.json";
import en from "./en.json";

const RTL_LANGUAGES = ["ar", "he", "fa", "ur"];

function isRtl(lng: string) {
  return RTL_LANGUAGES.includes(lng);
}

function applyLanguageToDocument(lng: string) {
  const dir = isRtl(lng) ? "rtl" : "ltr";
  const html = document.documentElement;

  html.setAttribute("dir", dir);
  html.setAttribute("lang", lng);

  document.body?.setAttribute("dir", dir);
  document.getElementById("root")?.setAttribute("dir", dir);

  try {
    localStorage.setItem("dir", dir);
    localStorage.setItem("lang", lng);
  } catch {
    /* private or sandboxed mode */
  }
}

function getInitialLang(): string {
  try {
    return localStorage.getItem("lang") || "ar";
  } catch {
    return "ar";
  }
}

const storedLang = getInitialLang();

i18n.use(initReactI18next).init({
  resources: {
    ar: { translation: ar },
    en: { translation: en },
  },
  lng: storedLang,
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

// Apply initial direction and lang on page load
applyLanguageToDocument(storedLang);

// Update direction and lang whenever language changes
i18n.on("languageChanged", (lng) => {
  applyLanguageToDocument(lng);
});

export default i18n;
