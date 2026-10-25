import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Archive,
  BadgeCheck,
  BookOpenCheck,
  Calculator,
  Check,
  ExternalLink,
  FileSignature,
  Globe,
  HandCoins,
  HandHeart,
  Heart,
  HeartHandshake,
  Keyboard,
  Layers,
  Link2,
  Mail,
  MessageCircle,
  Receipt,
  ShieldCheck,
  Star,
  Users,
  Wallet,
} from "lucide-react";
import Modal from "./Modal";
import { ShortcutList } from "./Shortcuts";

export const APP_VERSION = "0.2.0";

/** Public ZeidexSoft channels (zeidexsoft.com and the developer's ZeidexLink page). */
const DEVELOPER = {
  site: "https://zeidexsoft.com",
  profile: "https://zeidexlink.zeidexsoft.com/zeidex",
  store: "https://zeidexsoft.com/shop",
  email: "info@zeidexsoft.com",
  whatsapp: "https://wa.me/qr/2EZCWPVVO3DUE1?s=v",
  tiktok: "https://www.tiktok.com/@zeidexsoft",
  github: "https://github.com/zeiad6",
};

const FEATURES = [
  { icon: BookOpenCheck, key: "funds" },
  { icon: HandCoins, key: "donations" },
  { icon: HandHeart, key: "aid" },
  { icon: HeartHandshake, key: "sponsorships" },
  { icon: Users, key: "volunteers" },
  { icon: Archive, key: "archive" },
  { icon: FileSignature, key: "letters" },
  { icon: Receipt, key: "zatca" },
  { icon: Calculator, key: "zakat" },
  { icon: Wallet, key: "payments" },
  { icon: ShieldCheck, key: "audit" },
  { icon: Globe, key: "bilingual" },
];

function Out({ href, children, className = "dev-link" }: { href: string; children: React.ReactNode; className?: string }) {
  return (
    <a className={className} href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  );
}

export function ShortcutsSection() {
  const { t } = useTranslation();
  return (
    <section className="settings-section">
      <div className="settings-section-head">
        <h3>
          <Keyboard size={17} /> {t("shortcuts.title")}
        </h3>
        <small className="settings-hint">{t("shortcuts.hint")}</small>
      </div>
      <ShortcutList />
    </section>
  );
}

export function AboutSection() {
  const { t } = useTranslation();
  const [donating, setDonating] = useState(false);
  return (
    <>
      <section className="settings-section about-panel">
        <div className="about-hero">
          <img src="/logo-mark.svg" alt="" width={56} height={56} />
          <div>
            <h3>
              {t("app.name")} <span className="version-pill">v{APP_VERSION}</span>
            </h3>
            <p>{t("about.tagline")}</p>
          </div>
        </div>
        <ul className="feature-grid">
          {FEATURES.map((f) => (
            <li key={f.key}>
              <f.icon size={18} />
              <div>
                <strong>{t(`about.features.${f.key}.title`)}</strong>
                <span>{t(`about.features.${f.key}.text`)}</span>
              </div>
            </li>
          ))}
        </ul>
        <p className="settings-hint about-stack">{t("about.stack")}</p>
      </section>

      <section className="settings-section developer-card">
        <div className="developer-head">
          <span className="dev-mark" aria-hidden>
            Z
          </span>
          <div>
            <h3>
              ZeidexSoft <BadgeCheck size={16} className="verified" />
            </h3>
            <p>{t("developer.about")}</p>
          </div>
        </div>
        <div className="dev-links">
          <Out href={DEVELOPER.profile}>
            <Link2 size={15} /> {t("developer.profile")}
          </Out>
          <Out href={DEVELOPER.site}>
            <Globe size={15} /> zeidexsoft.com
          </Out>
          <Out href={`mailto:${DEVELOPER.email}`}>
            <Mail size={15} /> {DEVELOPER.email}
          </Out>
          <Out href={DEVELOPER.whatsapp}>
            <MessageCircle size={15} /> {t("developer.whatsapp")}
          </Out>
          <Out href={DEVELOPER.tiktok}>
            <ExternalLink size={15} /> TikTok @zeidexsoft
          </Out>
          <Out href={DEVELOPER.github}>
            <ExternalLink size={15} /> GitHub
          </Out>
        </div>
        <div className="support-row">
          <p>{t("donate.pitch")}</p>
          <button type="button" className="btn btn-primary support-btn" onClick={() => setDonating(true)}>
            <Heart size={16} /> {t("donate.button")}
          </button>
        </div>
        <p className="settings-note">© {new Date().getFullYear()} ZeidexSoft — {t("developer.rights")}</p>
      </section>

      {donating && (
        <Modal isOpen onClose={() => setDonating(false)} title={t("donate.title")}>
          <p className="settings-hint">{t("donate.intro")}</p>
          <div className="support-options">
            <Out href={DEVELOPER.store} className="support-option">
              <span className="dm-icon stars">
                <Star size={20} />
              </span>
              <span>
                <strong>{t("donate.stars")}</strong>
                <small>{t("donate.starsHint")}</small>
              </span>
              <ExternalLink size={15} />
            </Out>
            <Out href={DEVELOPER.whatsapp} className="support-option">
              <span className="dm-icon kuraimi">
                <Wallet size={20} />
              </span>
              <span>
                <strong>{t("donate.kuraimi")}</strong>
                <small>{t("donate.kuraimiHint")}</small>
              </span>
              <ExternalLink size={15} />
            </Out>
            <Out href={DEVELOPER.site} className="support-option">
              <span className="dm-icon site">
                <Globe size={20} />
              </span>
              <span>
                <strong>{t("donate.site")}</strong>
                <small>zeidexsoft.com</small>
              </span>
              <ExternalLink size={15} />
            </Out>
          </div>
          <p className="settings-note">{t("donate.note")}</p>
        </Modal>
      )}
    </>
  );
}

const LAYERS = [
  { key: "domain", tech: "Rust · yusr-domain" },
  { key: "infra", tech: "Rust · sqlx · PostgreSQL 18" },
  { key: "server", tech: "Rust · Axum · Tokio" },
  { key: "web", tech: "React · TypeScript · Vite" },
  { key: "desktop", tech: "Tauri 2 · WebView2 · MSI" },
  { key: "advisor", tech: "llama.cpp · Qwen 3.5 · OpenAI API" },
] as const;

/** How Yusr is built: the layers, the rules that hold them together, and how it is checked. */
export function BuildSection() {
  const { t } = useTranslation();
  return (
    <section className="settings-section build-panel">
      <div className="settings-section-head">
        <h3>
          <Layers size={17} /> {t("build.title")}
        </h3>
        <small className="settings-hint">{t("build.intro")}</small>
      </div>
      <ol className="layer-stack">
        {LAYERS.map((l, i) => (
          <li key={l.key}>
            <span className="layer-no">{i + 1}</span>
            <div>
              <strong>{t(`build.layers.${l.key}.title`)}</strong>
              <span>{t(`build.layers.${l.key}.text`)}</span>
              <code dir="ltr">{l.tech}</code>
            </div>
          </li>
        ))}
      </ol>
      <h4 className="build-sub">{t("build.principlesTitle")}</h4>
      <ul className="build-principles">
        {(["ledger", "audit", "security", "money", "offline", "tests"] as const).map((k) => (
          <li key={k}>
            <Check size={15} /> {t(`build.principles.${k}`)}
          </li>
        ))}
      </ul>
      <p className="settings-note">{t("build.license")}</p>
    </section>
  );
}
