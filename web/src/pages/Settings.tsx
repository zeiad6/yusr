import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api/client";
import { ORG_TYPES, type Organization } from "../api/types";
import { AboutSection, BuildSection, ShortcutsSection } from "../components/About";
import { AdvisorSection, BackupSection } from "../components/SystemSections";
import { toastSuccess } from "../components/Toast";
import { Field, PageHeader, useSubmit } from "../components/ui";
import { useAuthStore } from "../store/auth";
import { useRefs } from "../store/refs";
import { today } from "../utils/money";
import { ImagePlus, Trash2 } from "lucide-react";
import { toastError } from "../components/Toast";
import { MIN_PASSWORD_LEN } from "./Users";

export default function Settings() {
  const { t } = useTranslation();
  const can = useAuthStore((s) => s.can);
  return (
    <div className="page">
      <PageHeader title={t("settings.title")} />
      <div className="settings-stack">
        {can("reports.read") && <OrganizationSection editable={can("settings.manage")} />}
        {can("fx.manage") && <FxSection />}
        {can("backup.create") && <BackupSection />}
        {can("advisor.manage") && <AdvisorSection />}
        <PasswordSection />
        <ShortcutsSection />
        <AboutSection />
        <BuildSection />
      </div>
    </div>
  );
}

function OrganizationSection({ editable }: { editable: boolean }) {
  const { t } = useTranslation();
  const [org, setOrg] = useState<Organization | null>(null);
  const { busy, run } = useSubmit();

  useEffect(() => {
    api.get<Organization>("/settings/organization").then(setOrg).catch(() => undefined);
  }, []);

  if (!org) return null;
  // Optional fields store null rather than "" so the server keeps them absent.
  const OPTIONAL: (keyof Organization)[] = ["vat_number", "registration_no", "phone", "email", "website", "org_type"];
  const set = (k: keyof Organization) => (e: { target: { value: string } }) =>
    setOrg({ ...org, [k]: OPTIONAL.includes(k) ? e.target.value.trim() || null : e.target.value });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.put("/settings/organization", org);
      toastSuccess(t("common.saved"));
    });
  };

  return (
    <form className="settings-section" onSubmit={submit}>
      <div className="settings-section-head">
        <h3>{t("settings.organization")}</h3>
        <small className="settings-hint">{t("settings.organizationHint")}</small>
      </div>
      <fieldset disabled={!editable}>
        <div className="form-row">
          <Field label={t("common.nameAr")}>
            <input required dir="rtl" value={org.name_ar} onChange={set("name_ar")} />
          </Field>
          <Field label={t("common.nameEn")}>
            <input required dir="ltr" value={org.name_en} onChange={set("name_en")} />
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("settings.license")}>
            <input value={org.license_no} onChange={set("license_no")} />
          </Field>
          <Field label={t("settings.vatNumber")} hint={t("settings.vatHint")}>
            <input dir="ltr" maxLength={15} value={org.vat_number ?? ""} onChange={set("vat_number")} />
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("settings.orgType")}>
            <select value={org.org_type ?? ""} onChange={set("org_type")}>
              <option value="">—</option>
              {ORG_TYPES.map((o) => (
                <option key={o} value={o}>
                  {t(`orgTypes.${o}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("settings.registrationNo")} hint={t("settings.registrationHint")}>
            <input dir="ltr" value={org.registration_no ?? ""} onChange={set("registration_no")} />
          </Field>
        </div>
        <Field label={t("settings.address")}>
          <input value={org.address} onChange={set("address")} />
        </Field>
        <div className="form-row three">
          <Field label={t("common.phone")}>
            <input type="tel" dir="ltr" value={org.phone ?? ""} onChange={set("phone")} />
          </Field>
          <Field label={t("common.email")}>
            <input type="email" dir="ltr" value={org.email ?? ""} onChange={set("email")} />
          </Field>
          <Field label={t("settings.website")}>
            <input type="url" dir="ltr" placeholder="https://" value={org.website ?? ""} onChange={set("website")} />
          </Field>
        </div>
        <Field label={t("settings.fiscalYearStart")} hint={t("settings.fiscalYearHint")}>
          <select value={org.fiscal_year_start ?? 1} onChange={(e) => setOrg({ ...org, fiscal_year_start: Number(e.target.value) })}>
            {Array.from({ length: 12 }, (_, i) => i + 1).map((m) => (
              <option key={m} value={m}>
                {t(`months.${m}`)}
              </option>
            ))}
          </select>
        </Field>
        <div className="image-settings">
          <ImageSetting
            label={t("settings.logo")}
            hint={t("settings.logoHint")}
            value={org.logo_data_url}
            fallback="/logo-mark.svg"
            editable={editable}
            onChange={(logo_data_url) => setOrg({ ...org, logo_data_url })}
          />
          <ImageSetting
            label={t("settings.stamp")}
            hint={t("settings.stampHint")}
            value={org.stamp_data_url}
            editable={editable}
            onChange={(stamp_data_url) => setOrg({ ...org, stamp_data_url })}
          />
        </div>
        <div className="form-row">
          <Field label={t("settings.taxStatementAr")}>
            <textarea rows={2} dir="rtl" value={org.tax_statement_ar} onChange={set("tax_statement_ar")} />
          </Field>
          <Field label={t("settings.taxStatementEn")}>
            <textarea rows={2} dir="ltr" value={org.tax_statement_en} onChange={set("tax_statement_en")} />
          </Field>
        </div>
        {editable && (
          <div className="form-actions">
            <button type="submit" className="btn btn-primary" disabled={busy}>
              {t("common.save")}
            </button>
          </div>
        )}
      </fieldset>
    </form>
  );
}

function FxSection() {
  const { t } = useTranslation();
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [form, setForm] = useState({ currency: "USD", date: today(), rate: "" });
  const [current, setCurrent] = useState<string | null>(null);
  const { busy, run } = useSubmit();

  const lookup = () =>
    run(async () => {
      const r = await api.get<{ rate: string }>(`/fx-rates/${form.currency}`, { date: form.date });
      setCurrent(r.rate);
    });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.put("/fx-rates", { ...form, source: "manual" });
      toastSuccess(t("common.saved"));
      setCurrent(form.rate);
    });
  };

  return (
    <form className="settings-section" onSubmit={submit}>
      <div className="settings-section-head">
        <h3>{t("settings.fx")}</h3>
        <small className="settings-hint">{t("settings.fxHint", { base: baseCurrency })}</small>
      </div>
      <div className="form-row three">
        <Field label={t("common.currency")}>
          <input required maxLength={3} pattern="[A-Z]{3}" title="ISO 4217 (SAR, USD…)" dir="ltr" value={form.currency} onChange={(e) => (setForm({ ...form, currency: e.target.value.toUpperCase() }), setCurrent(null))} />
        </Field>
        <Field label={t("common.date")}>
          <input type="date" required value={form.date} onChange={(e) => (setForm({ ...form, date: e.target.value }), setCurrent(null))} />
        </Field>
        <Field label={t("settings.rate", { base: baseCurrency })}>
          <input required inputMode="decimal" dir="ltr" value={form.rate} onChange={(e) => setForm({ ...form, rate: e.target.value })} />
        </Field>
      </div>
      {current !== null && (
        <p className="settings-note">
          1 {form.currency} = {current} {baseCurrency}
        </p>
      )}
      <div className="form-actions">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {t("common.save")}
        </button>
        <button type="button" className="btn btn-secondary" disabled={busy || form.currency.length !== 3} onClick={lookup}>
          {t("settings.lookupRate")}
        </button>
      </div>
    </form>
  );
}

function PasswordSection() {
  const { t } = useTranslation();
  const logout = useAuthStore((s) => s.logout);
  const [form, setForm] = useState({ current_password: "", new_password: "", confirm: "" });
  const { busy, run } = useSubmit();
  const valid = form.new_password.length >= MIN_PASSWORD_LEN && form.new_password === form.confirm;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.post("/auth/password", { current_password: form.current_password, new_password: form.new_password });
      toastSuccess(t("settings.passwordChanged"));
      // The server revoked every token, this one included.
      logout();
    });
  };

  return (
    <form className="settings-section" onSubmit={submit}>
      <div className="settings-section-head">
        <h3>{t("settings.password")}</h3>
        <small className="settings-hint">{t("settings.passwordHint")}</small>
      </div>
      <div className="form-row three">
        <Field label={t("settings.currentPassword")}>
          <input type="password" required autoComplete="current-password" value={form.current_password} onChange={(e) => setForm({ ...form, current_password: e.target.value })} />
        </Field>
        <Field label={t("settings.newPassword")} hint={t("users.passwordHint", { min: MIN_PASSWORD_LEN })}>
          <input type="password" required autoComplete="new-password" value={form.new_password} onChange={(e) => setForm({ ...form, new_password: e.target.value })} />
        </Field>
        <Field label={t("settings.confirmPassword")}>
          <input type="password" required autoComplete="new-password" value={form.confirm} onChange={(e) => setForm({ ...form, confirm: e.target.value })} />
        </Field>
      </div>
      <div className="form-actions">
        <button type="submit" className="btn btn-primary" disabled={busy || !valid}>
          {t("settings.changePassword")}
        </button>
      </div>
    </form>
  );
}

/** Server limit: crates/yusr-domain/src/receipt.rs MAX_LOGO_CHARS. */
const MAX_LOGO_CHARS = 400_000;
/** Letterheads print the logo at about 22 mm; 480 px is sharp at 300 dpi and stays small. */
const LOGO_PX = 480;

/** Scales the image down and re-encodes it as PNG, or WebP when the PNG is too large. */
async function shrinkLogo(file: File): Promise<string> {
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, LOGO_PX / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(bitmap.width * scale));
  canvas.height = Math.max(1, Math.round(bitmap.height * scale));
  canvas.getContext("2d")!.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  for (const [type, quality] of [["image/png", undefined], ["image/webp", 0.9], ["image/webp", 0.75], ["image/jpeg", 0.8]] as const) {
    const url = canvas.toDataURL(type, quality);
    if (url.startsWith(`data:${type};base64,`) && url.length <= MAX_LOGO_CHARS) return url;
  }
  throw new Error("logo too large");
}

/** Logo or stamp picker: shrinks the image in the browser so it fits the server limit. */
function ImageSetting(props: { label: string; hint: string; value?: string | null; fallback?: string; editable: boolean; onChange: (v: string | null) => void }) {
  const { t } = useTranslation();
  return (
    <div className="logo-editor">
      <div className="logo-preview">
        {props.value ? <img src={props.value} alt={props.label} /> : props.fallback ? <img src={props.fallback} alt="" /> : <span className="logo-empty">—</span>}
      </div>
      <div className="logo-actions">
        <strong>{props.label}</strong>
        <small className="settings-hint">{props.hint}</small>
        {props.editable && (
          <div className="action-btns">
            <label className="btn btn-secondary btn-sm">
              <ImagePlus size={14} /> {t("settings.chooseLogo")}
              <input
                type="file"
                accept="image/png,image/jpeg,image/webp"
                hidden
                onChange={async (e) => {
                  const f = e.target.files?.[0];
                  e.target.value = "";
                  if (!f) return;
                  try {
                    props.onChange(await shrinkLogo(f));
                  } catch {
                    toastError(t("settings.logoInvalid"));
                  }
                }}
              />
            </label>
            {props.value && (
              <button type="button" className="btn btn-sm btn-ghost" onClick={() => props.onChange(null)}>
                <Trash2 size={14} /> {t("common.delete")}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
