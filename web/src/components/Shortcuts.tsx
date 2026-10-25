import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import Modal from "./Modal";
import { NAV } from "./Sidebar";
import { useAuthStore } from "../store/auth";
import { useThemeStore } from "../store/theme";

/** Second key after `g` → route. Letters follow the English page names. */
export const GO_KEYS: Record<string, string> = {
  d: "/",
  j: "/journal",
  a: "/accounts",
  f: "/funds",
  o: "/donors",
  n: "/donations",
  b: "/beneficiaries",
  s: "/sponsorships",
  v: "/volunteers",
  r: "/reports",
  e: "/archive",
  l: "/letters",
  z: "/zakat",
  i: "/invoices",
  x: "/audit",
  u: "/users",
  t: "/settings",
};

/** How long `g` waits for its second key. */
const SEQUENCE_MS = 1200;

/** Shown in the help dialog and in Settings; `keys` are display labels. */
export const SHORTCUTS: { keys: string[]; action: string }[] = [
  { keys: ["?"], action: "shortcuts.help" },
  { keys: ["/"], action: "shortcuts.search" },
  { keys: ["N"], action: "shortcuts.new" },
  { keys: ["Ctrl", "S"], action: "shortcuts.save" },
  { keys: ["Ctrl", "P"], action: "shortcuts.print" },
  { keys: ["Alt", "1…9"], action: "shortcuts.navNumber" },
  { keys: ["G", "…"], action: "shortcuts.go" },
  { keys: ["Shift", "T"], action: "shortcuts.theme" },
  { keys: ["Shift", "L"], action: "shortcuts.language" },
  { keys: ["Esc"], action: "shortcuts.close" },
];

function typing(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));
}

/** Clicks the visible element marked `data-shortcut=<name>`, preferring one inside an open dialog. */
function clickMarked(name: string): boolean {
  const scope = document.querySelector(".modal") ?? document;
  const el = scope.querySelector<HTMLButtonElement>(`[data-shortcut="${name}"]:not([disabled])`) ?? document.querySelector<HTMLButtonElement>(`[data-shortcut="${name}"]:not([disabled])`);
  if (!el || el.offsetParent === null) return false;
  el.click();
  return true;
}

export function Kbd({ keys }: { keys: string[] }) {
  return (
    <span className="kbd-group" dir="ltr">
      {keys.map((k) => (
        <kbd key={k}>{k}</kbd>
      ))}
    </span>
  );
}

export function ShortcutList() {
  const { t } = useTranslation();
  return (
    <div className="shortcut-list">
      <dl>
        {SHORTCUTS.map((s) => (
          <div key={s.action}>
            <dt>
              <Kbd keys={s.keys} />
            </dt>
            <dd>{t(s.action)}</dd>
          </div>
        ))}
      </dl>
      <h4>{t("shortcuts.goTitle")}</h4>
      <dl className="go-grid">
        {Object.entries(GO_KEYS).map(([k, path]) => {
          const item = NAV.find((n) => n.path === path);
          return item ? (
            <div key={k}>
              <dt>
                <Kbd keys={["G", k.toUpperCase()]} />
              </dt>
              <dd>{t(`nav.${item.key}`)}</dd>
            </div>
          ) : null;
        })}
      </dl>
    </div>
  );
}

/** Global keyboard shortcuts; mounted once in the layout. */
export default function ShortcutsLayer() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const can = useAuthStore((s) => s.can);
  const toggleTheme = useThemeStore((s) => s.toggle);
  const [help, setHelp] = useState(false);
  const pendingG = useRef(0);

  useEffect(() => {
    const allowed = NAV.filter((n) => n.permission === null || can(n.permission));
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && !e.altKey && (e.key === "s" || e.key === "S")) {
        if (clickMarked("save")) e.preventDefault();
        return;
      }
      if (mod && !e.altKey && (e.key === "p" || e.key === "P")) {
        // Print the document being previewed, not the application chrome.
        if (clickMarked("print")) e.preventDefault();
        return;
      }
      if (e.altKey && !mod && /^Digit[1-9]$/.test(e.code)) {
        const item = allowed[Number(e.code.slice(5)) - 1];
        if (item) {
          e.preventDefault();
          navigate(item.path);
        }
        return;
      }
      if (mod || e.altKey || typing(e.target) || document.querySelector(".modal")) return;
      const key = e.key.toLowerCase();
      if (Date.now() - pendingG.current < SEQUENCE_MS) {
        pendingG.current = 0;
        const path = GO_KEYS[key];
        if (path && allowed.some((n) => n.path === path)) {
          e.preventDefault();
          navigate(path);
        }
        return;
      }
      if (e.key === "?") {
        e.preventDefault();
        setHelp(true);
      } else if (e.key === "/") {
        const search = document.querySelector<HTMLInputElement>(".search-box input, input[type=search]");
        if (search) {
          e.preventDefault();
          search.focus();
        }
      } else if (key === "g" && !e.shiftKey) {
        pendingG.current = Date.now();
      } else if (key === "n" && !e.shiftKey) {
        if (clickMarked("new")) e.preventDefault();
      } else if (e.shiftKey && key === "t") {
        toggleTheme();
      } else if (e.shiftKey && key === "l") {
        i18n.changeLanguage(i18n.language.startsWith("ar") ? "en" : "ar");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [can, navigate, toggleTheme, i18n]);

  return help ? (
    <Modal isOpen onClose={() => setHelp(false)} title={t("shortcuts.title")}>
      <ShortcutList />
    </Modal>
  ) : null;
}
