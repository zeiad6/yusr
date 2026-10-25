import { useEffect, useState } from "react";
import { Outlet } from "react-router-dom";
import Sidebar from "./Sidebar";
import Header from "./Header";
import ToastContainer from "./Toast";
import ShortcutsLayer from "./Shortcuts";
import AdvisorWidget from "./Advisor";
import { useTranslation } from "react-i18next";
import { WifiOff } from "lucide-react";
import { useAuthStore } from "../store/auth";

/** setTimeout clamps delays above 2^31-1 ms (~24.8 days) to 0; tokens never live that long anyway. */
const MAX_TIMER_MS = 2 ** 31 - 1;

const COLLAPSED_KEY = "sidebar-collapsed";
// Matches the phone breakpoint in globals.css; the expanded sidebar would take half the screen.
const PHONE = "(max-width: 768px)";
const isPhone = () => window.matchMedia?.(PHONE).matches ?? false;

function readCollapsed(): boolean {
  try {
    const stored = localStorage.getItem(COLLAPSED_KEY);
    if (stored !== null) return stored === "1";
  } catch {
    // storage unavailable (private mode): fall through to the screen-size default
  }
  return isPhone();
}

export default function Layout() {
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = useState(readCollapsed);
  const expiresAt = useAuthStore((s) => s.session?.expiresAt);
  const logout = useAuthStore((s) => s.logout);
  const [online, setOnline] = useState(() => navigator.onLine);

  // Sign out when the token expires instead of failing on the next click.
  useEffect(() => {
    if (!expiresAt) return;
    const id = setTimeout(logout, Math.min(MAX_TIMER_MS, Math.max(0, expiresAt * 1000 - Date.now())));
    return () => clearTimeout(id);
  }, [expiresAt, logout]);

  useEffect(() => {
    const up = () => setOnline(true);
    const down = () => setOnline(false);
    window.addEventListener("online", up);
    window.addEventListener("offline", down);
    return () => {
      window.removeEventListener("online", up);
      window.removeEventListener("offline", down);
    };
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(COLLAPSED_KEY, collapsed ? "1" : "0");
    } catch {
      // storage unavailable (private mode): state stays in memory only
    }
  }, [collapsed]);

  return (
    <div className={`layout${collapsed ? " collapsed" : ""}`}>
      <Sidebar onNavigate={() => isPhone() && setCollapsed(true)} />
      <div className="main-content">
        <Header onToggleSidebar={() => setCollapsed((v) => !v)} />
        <main className="page-content">
          {!online && (
            <div className="offline-banner" role="status">
              <WifiOff size={16} /> {t("common.offline")}
            </div>
          )}
          <Outlet />
        </main>
      </div>
      <ToastContainer />
      <ShortcutsLayer />
      <AdvisorWidget />
    </div>
  );
}
