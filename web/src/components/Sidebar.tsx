import { NavLink } from "react-router-dom";
import { useTranslation } from "react-i18next";
import {
  BookOpen,
  Building2,
  Calculator,
  ChartColumn,
  FolderArchive,
  HeartHandshake,
  UsersRound,
  Mail,
  FileText,
  HandCoins,
  HandHeart,
  LayoutGrid,
  Receipt,
  Settings,
  ShieldCheck,
  UserCog,
  Users,
  type LucideIcon,
} from "lucide-react";
import { useAuthStore } from "../store/auth";
import type { Permission } from "../store/permissions";

export interface NavItem {
  path: string;
  key: string;
  icon: LucideIcon;
  permission: Permission | null;
}

export const NAV: NavItem[] = [
  { path: "/", key: "dashboard", icon: LayoutGrid, permission: "reports.read" },
  { path: "/journal", key: "journal", icon: FileText, permission: "ledger.read" },
  { path: "/accounts", key: "accounts", icon: BookOpen, permission: "ledger.read" },
  { path: "/funds", key: "funds", icon: Building2, permission: "reports.read" },
  { path: "/donors", key: "donors", icon: Users, permission: "donors.read" },
  { path: "/donations", key: "donations", icon: HandCoins, permission: "donations.read" },
  { path: "/beneficiaries", key: "beneficiaries", icon: HandHeart, permission: "cases.read" },
  { path: "/sponsorships", key: "sponsorships", icon: HeartHandshake, permission: "sponsorships.read" },
  { path: "/volunteers", key: "volunteers", icon: UsersRound, permission: "volunteers.read" },
  { path: "/reports", key: "reports", icon: ChartColumn, permission: "reports.read" },
  { path: "/archive", key: "archive", icon: FolderArchive, permission: "archive.read" },
  { path: "/letters", key: "letters", icon: Mail, permission: "letters.read" },
  { path: "/zakat", key: "zakat", icon: Calculator, permission: "zakat.calculate" },
  { path: "/invoices", key: "invoices", icon: Receipt, permission: "invoices.read" },
  { path: "/audit", key: "audit", icon: ShieldCheck, permission: "audit.read" },
  { path: "/users", key: "users", icon: UserCog, permission: "users.manage" },
  { path: "/settings", key: "settings", icon: Settings, permission: null },
];

/** `onNavigate` lets the layout fold the sidebar after a pick on phones. */
export default function Sidebar({ onNavigate }: { onNavigate?: () => void }) {
  const { t } = useTranslation();
  const can = useAuthStore((s) => s.can);

  return (
    <aside className="sidebar" aria-label={t("app.name")}>
      <div className="sidebar-brand">
        <span className="brand-icon">
          <img src="/logo-mark-light.svg" alt="" width={18} height={18} aria-hidden style={{ display: "block" }} />
        </span>
        <span className="brand-text">{t("app.short")}</span>
      </div>

      <nav className="sidebar-nav">
        {NAV.filter((item) => item.permission === null || can(item.permission)).map((item) => (
          <NavLink
            key={item.path}
            aria-label={t(`nav.${item.key}`)}
            to={item.path}
            end={item.path === "/"}
            className={({ isActive }) => `nav-item${isActive ? " active" : ""}`}
            title={t(`nav.${item.key}`)}
            onClick={onNavigate}
          >
            <span className="nav-icon">
              <item.icon size={18} strokeWidth={1.75} aria-hidden />
            </span>
            <span className="nav-label">{t(`nav.${item.key}`)}</span>
          </NavLink>
        ))}
      </nav>

    </aside>
  );
}
