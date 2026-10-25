import { useTranslation } from "react-i18next";
import { useLocation } from "react-router-dom";
import { Globe, LogOut, Moon, PanelRight, Sun } from "lucide-react";
import { api } from "../api/client";
import { useAuthStore } from "../store/auth";
import { useThemeStore } from "../store/theme";
import { NAV } from "./Sidebar";

function initials(name?: string) {
  const parts = (name ?? "").trim().split(/\s+/).filter(Boolean);
  return parts.length ? parts.slice(0, 2).map((p) => p.charAt(0)).join("").toUpperCase() : "?";
}

export default function Header({ onToggleSidebar }: { onToggleSidebar: () => void }) {
  const { t, i18n } = useTranslation();
  const session = useAuthStore((s) => s.session);
  const logout = useAuthStore((s) => s.logout);
  const { theme, toggle } = useThemeStore();
  const location = useLocation();
  const isRtl = i18n.dir() === "rtl";
  const isAr = i18n.language.startsWith("ar");

  const segment = "/" + (location.pathname.split("/").filter(Boolean)[0] ?? "");
  const item = NAV.find((n) => n.path === segment);
  const flip = isRtl ? undefined : { transform: "scaleX(-1)" };

  const signOut = async () => {
    // Revokes the token server-side on every device; sign out locally even if offline.
    try {
      await api.post("/auth/logout", {});
    } catch {
      /* already invalid or offline */
    }
    logout();
  };

  return (
    <header className="header">
      <div className="header-left">
        <button type="button" className="sidebar-toggle" onClick={onToggleSidebar} aria-label={t("common.toggleMenu")} title={t("common.toggleMenu")}>
          <PanelRight size={17} strokeWidth={1.75} style={flip} aria-hidden />
        </button>
        <h1 className="page-title">{item ? t(`nav.${item.key}`) : t("app.tagline")}</h1>
        {item && <span className="page-subtitle">{t("app.name")}</span>}
      </div>

      <div className="header-right">
        <button type="button" className="theme-toggle" onClick={toggle} aria-label={t("common.toggleTheme")} title={t("common.toggleTheme")}>
          {theme === "dark" ? <Sun size={15} aria-hidden /> : <Moon size={15} aria-hidden />}
        </button>

        <button type="button" className="lang-toggle" onClick={() => i18n.changeLanguage(isAr ? "en" : "ar")}>
          <Globe size={15} aria-hidden />
          <span lang={isAr ? "en" : "ar"}>{isAr ? "EN" : "عربي"}</span>
        </button>

        <span className="user-avatar" aria-hidden>
          {initials(session?.fullName)}
        </span>
        <div className="user-info">
          <span className="user-name">{session?.fullName}</span>
          <span className="user-role">{t(`roles.${session?.role ?? "viewer"}`)}</span>
        </div>

        <button type="button" className="logout-btn" onClick={signOut}>
          <LogOut size={15} style={isRtl ? { transform: "scaleX(-1)" } : undefined} aria-hidden />
          <span>{t("auth.logout")}</span>
        </button>
      </div>
    </header>
  );
}
