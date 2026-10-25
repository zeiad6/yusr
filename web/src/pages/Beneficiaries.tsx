import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { HandHeart, Plus, Ticket } from "lucide-react";
import { api, newRequestKey } from "../api/client";
import {
  AID_KINDS,
  ASNAF,
  HOUSING,
  type AidKind,
  type Beneficiary,
  type CaseProfile,
  type Distribution,
  type NewBeneficiary,
  type NewDistribution,
  type PostedEntry,
  type Priority,
} from "../api/types";
import Modal from "../components/Modal";
import { toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, useLoad, useSubmit } from "../components/ui";
import { FormActions, FundSelect } from "../components/forms";
import EditRecord, { EditButton } from "../components/EditRecord";
import { useAuthStore } from "../store/auth";
import { localName, useRefs } from "../store/refs";
import { formatMoney, isPositive, today } from "../utils/money";

const PRIORITY_TONE: Record<Priority, "danger" | "warning" | "info" | "secondary"> = {
  critical: "danger",
  high: "warning",
  medium: "info",
  low: "secondary",
};

const emptyProfile = (): CaseProfile => ({
  household_size: 1,
  monthly_income: "0",
  poverty_line_per_person: "1000",
  dependents_under_18: 0,
  members_with_disability: 0,
  members_with_chronic_illness: 0,
  outstanding_debt: "0",
  housing: "rented",
  monthly_rent: "0",
  head_of_household_employed: false,
  female_or_orphan_headed: false,
});

export default function Beneficiaries() {
  const { t } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const [creating, setCreating] = useState(false);
  const [redeeming, setRedeeming] = useState(false);
  const [viewing, setViewing] = useState<Beneficiary | null>(null);
  const [showAll, setShowAll] = useState(false);
  const { data, error, reload } = useLoad(() => api.get<Beneficiary[]>("/beneficiaries", { limit: 500, all: showAll || undefined }), [showAll]);

  return (
    <div className="page">
      <PageHeader title={t("cases.title")}>
        <label className="checkbox-label">
          <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />
          {t("cases.showClosed")}
        </label>
        {can("vouchers.redeem") && (
          <button type="button" className="btn btn-secondary" onClick={() => setRedeeming(true)}>
            <Ticket size={16} /> {t("cases.redeemVoucher")}
          </button>
        )}
        {can("cases.manage") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("cases.new")}
          </button>
        )}
      </PageHeader>

      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={data ?? []}
        rowKey={(b) => b.id}
        exportName="beneficiaries"
        onRowClick={setViewing}
        columns={[
          { header: t("cases.fileNo"), cell: (b) => String(b.file_no) },
          { header: t("common.name"), cell: (b) => b.full_name },
          { header: t("cases.category"), cell: (b) => t(`asnaf.${b.category}`) },
          { header: t("cases.city"), cell: (b) => b.city ?? "" },
          { header: t("cases.score"), cell: (b) => String(b.score), num: true },
          {
            header: t("cases.priority"),
            text: (b) => t(`priority.${b.priority}`),
            cell: (b) => <Badge tone={PRIORITY_TONE[b.priority]}>{t(`priority.${b.priority}`)}</Badge>,
          },
          { header: t("common.status"), cell: (b) => t(`caseStatus.${b.status}`, { defaultValue: b.status }) },
        ]}
      />

      {creating && <BeneficiaryForm onClose={() => setCreating(false)} onSaved={reload} />}
      {redeeming && <RedeemForm onClose={() => setRedeeming(false)} />}
      {viewing && (
        <CaseModal
          beneficiary={viewing}
          onClose={() => setViewing(null)}
          onChanged={(b) => {
            setViewing(b);
            reload();
          }}
        />
      )}
    </div>
  );
}

function ProfileFields({ value, onChange }: { value: CaseProfile; onChange: (p: CaseProfile) => void }) {
  const { t } = useTranslation();
  const int = (k: keyof CaseProfile) => (
    <Field label={t(`profile.${k}`)}>
      <input type="number" min={k === "household_size" ? 1 : 0} required value={value[k] as number} onChange={(e) => onChange({ ...value, [k]: Number(e.target.value) })} />
    </Field>
  );
  const dec = (k: keyof CaseProfile) => (
    <Field label={t(`profile.${k}`)}>
      <input inputMode="decimal" required value={value[k] as string} onChange={(e) => onChange({ ...value, [k]: e.target.value })} />
    </Field>
  );
  return (
    <fieldset className="settings-section">
      <legend>{t("cases.profile")}</legend>
      <div className="form-row three">
        {int("household_size")}
        {int("dependents_under_18")}
        {int("members_with_disability")}
      </div>
      <div className="form-row three">
        {int("members_with_chronic_illness")}
        {dec("monthly_income")}
        {dec("poverty_line_per_person")}
      </div>
      <div className="form-row three">
        {dec("outstanding_debt")}
        <Field label={t("profile.housing")}>
          <select value={value.housing} onChange={(e) => onChange({ ...value, housing: e.target.value as CaseProfile["housing"] })}>
            {HOUSING.map((h) => (
              <option key={h} value={h}>
                {t(`housing.${h}`)}
              </option>
            ))}
          </select>
        </Field>
        {value.housing === "rented" && dec("monthly_rent")}
      </div>
      <label className="checkbox-label">
        <input type="checkbox" checked={value.head_of_household_employed} onChange={(e) => onChange({ ...value, head_of_household_employed: e.target.checked })} />
        {t("profile.head_of_household_employed")}
      </label>
      <label className="checkbox-label">
        <input type="checkbox" checked={value.female_or_orphan_headed} onChange={(e) => onChange({ ...value, female_or_orphan_headed: e.target.checked })} />
        {t("profile.female_or_orphan_headed")}
      </label>
    </fieldset>
  );
}

function BeneficiaryForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState<NewBeneficiary>({ full_name: "", national_id: null, phone: null, city: null, category: "fuqara", profile: emptyProfile() });
  const { busy, run } = useSubmit();
  const opt = (v: string) => v.trim() || null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const b = await api.post<Beneficiary>("/beneficiaries", form);
      toastSuccess(t("cases.created", { no: b.file_no, score: b.score }));
      onSaved();
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} size="lg" title={t("cases.new")}>
      <form onSubmit={submit}>
        <div className="form-row">
          <Field label={t("common.name")}>
            <input required value={form.full_name} onChange={(e) => setForm({ ...form, full_name: e.target.value })} />
          </Field>
          <Field label={t("cases.category")}>
            <select value={form.category} onChange={(e) => setForm({ ...form, category: e.target.value as NewBeneficiary["category"] })}>
              {ASNAF.map((a) => (
                <option key={a} value={a}>
                  {t(`asnaf.${a}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="form-row three">
          <Field label={t("donors.nationalId")}>
            <input value={form.national_id ?? ""} onChange={(e) => setForm({ ...form, national_id: opt(e.target.value) })} />
          </Field>
          <Field label={t("common.phone")}>
            <input type="tel" dir="ltr" value={form.phone ?? ""} onChange={(e) => setForm({ ...form, phone: opt(e.target.value) })} />
          </Field>
          <Field label={t("cases.city")}>
            <input value={form.city ?? ""} onChange={(e) => setForm({ ...form, city: opt(e.target.value) })} />
          </Field>
        </div>
        <ProfileFields value={form.profile} onChange={(profile) => setForm({ ...form, profile })} />
        <FormActions busy={busy} onClose={onClose} />
      </form>
    </Modal>
  );
}

function CaseModal({ beneficiary: b, onClose, onChanged }: { beneficiary: Beneficiary; onClose: () => void; onChanged: (b: Beneficiary) => void }) {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [mode, setMode] = useState<"view" | "reassess" | "aid" | "edit">("view");
  const [profile, setProfile] = useState<CaseProfile>(b.profile);
  const [note, setNote] = useState("");
  const { busy, run } = useSubmit();

  const reassess = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const updated = await api.post<Beneficiary>(`/beneficiaries/${b.id}/reassess`, { profile, note });
      toastSuccess(t("cases.reassessed", { score: updated.score }));
      onChanged(updated);
      setMode("view");
    });
  };

  return (
    <Modal isOpen onClose={onClose} size="lg" title={`${b.full_name} · #${b.file_no}`}>
      <div className="detail-grid">
        <div><span>{t("cases.category")}</span><strong>{t(`asnaf.${b.category}`)}</strong></div>
        <div><span>{t("cases.score")}</span><strong>{b.score} / 100</strong></div>
        <div><span>{t("cases.priority")}</span><Badge tone={PRIORITY_TONE[b.priority]}>{t(`priority.${b.priority}`)}</Badge></div>
        <div><span>{t("common.status")}</span><strong>{t(`caseStatus.${b.status}`, { defaultValue: b.status })}</strong></div>
        <div><span>{t("profile.household_size")}</span><strong>{b.profile.household_size}</strong></div>
        <div><span>{t("profile.monthly_income")}</span><strong>{formatMoney(b.profile.monthly_income, baseCurrency, i18n.language)}</strong></div>
        <div><span>{t("profile.housing")}</span><strong>{t(`housing.${b.profile.housing}`)}</strong></div>
        <div><span>{t("profile.outstanding_debt")}</span><strong>{formatMoney(b.profile.outstanding_debt, baseCurrency, i18n.language)}</strong></div>
      </div>

      {mode === "view" && (
        <div className="form-actions">
          {can("aid.distribute") && b.status === "active" && (
            <button type="button" className="btn btn-primary" onClick={() => setMode("aid")}>
              <HandHeart size={16} /> {t("cases.distribute")}
            </button>
          )}
          {can("cases.manage") && (
            <button type="button" className="btn btn-secondary" onClick={() => setMode("reassess")}>
              {t("cases.reassess")}
            </button>
          )}
          {can("cases.manage") && <EditButton onClick={() => setMode("edit")} />}
        </div>
      )}

      {mode === "edit" && (
        <EditRecord
          title={t("cases.edit")}
          path={`/beneficiaries/${b.id}`}
          initial={b}
          note={t("cases.editNote")}
          onClose={() => setMode("view")}
          onSaved={(u) => onChanged(u as Beneficiary)}
          fields={[
            { key: "full_name", label: t("common.name"), required: true },
            { key: "national_id", label: t("donors.nationalId") },
            { key: "phone", label: t("common.phone"), type: "tel", dir: "ltr" },
            { key: "city", label: t("cases.city") },
            { key: "category", label: t("cases.category"), type: "select", required: true, options: ASNAF.map((a) => ({ value: a, label: t(`asnaf.${a}`) })) },
            {
              key: "status",
              label: t("common.status"),
              type: "select",
              required: true,
              options: ["active", "suspended", "closed"].map((s) => ({ value: s, label: t(`caseStatus.${s}`) })),
            },
          ]}
        />
      )}

      {mode === "reassess" && (
        <form onSubmit={reassess}>
          <ProfileFields value={profile} onChange={setProfile} />
          <Field label={t("cases.reassessNote")}>
            <input value={note} onChange={(e) => setNote(e.target.value)} />
          </Field>
          <FormActions busy={busy} onClose={() => setMode("view")} label={t("cases.reassess")} />
        </form>
      )}

      {mode === "aid" && <DistributionForm beneficiary={b} onDone={() => setMode("view")} />}
    </Modal>
  );
}

function DistributionForm({ beneficiary, onDone }: { beneficiary: Beneficiary; onDone: () => void }) {
  const { t, i18n } = useTranslation();
  const { funds, projects, baseCurrency } = useRefs();
  const zakatFund = funds.find((f) => f.donation_kind === "zakat");
  const [form, setForm] = useState<NewDistribution>({
    beneficiary_id: beneficiary.id,
    kind: "cash",
    fund_id: zakatFund?.id ?? funds[0]?.id ?? "",
    project_id: null,
    amount: "",
    currency: baseCurrency,
    description: "",
    items: null,
    distributed_on: today(),
    paid_from_bank: false,
  });
  const [itemsText, setItemsText] = useState("");
  const [result, setResult] = useState<Distribution | null>(null);
  const [requestKey] = useState(newRequestKey);
  const { busy, run } = useSubmit();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      // In-kind items are free text, one per line; stored as a JSON list.
      const items = form.kind === "in_kind" ? itemsText.split("\n").map((s) => s.trim()).filter(Boolean) : null;
      const r = await api.post<Distribution>("/aid/distributions", { ...form, items }, requestKey);
      toastSuccess(t("cases.distributed", { no: r.entry.entry_no }));
      setResult(r);
    });
  };

  if (result) {
    return (
      <div className="settings-stack">
        <div className="settings-note">
          {t("cases.distributed", { no: result.entry.entry_no })}
          {result.voucher_code && (
            <>
              <br />
              {t("cases.voucherCode")}: <code className="hash voucher">{result.voucher_code}</code>
            </>
          )}
        </div>
        <div className="form-actions">
          <button type="button" className="btn btn-secondary" onClick={onDone}>
            {t("common.close")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <form onSubmit={submit}>
      <div className="form-row three">
        <Field label={t("cases.aidKind")}>
          <select value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value as AidKind })}>
            {AID_KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`aidKinds.${k}`)}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("journal.fund")}>
          <FundSelect value={form.fund_id} onChange={(fund_id) => setForm({ ...form, fund_id, project_id: null })} />
        </Field>
        <Field label={t("journal.project")}>
          <select value={form.project_id ?? ""} onChange={(e) => setForm({ ...form, project_id: e.target.value || null })}>
            <option value="">{t("journal.noProject")}</option>
            {projects.filter((p) => p.fund_id === form.fund_id).map((p) => (
              <option key={p.id} value={p.id}>
                {p.code} · {localName(p, i18n.language)}
              </option>
            ))}
          </select>
        </Field>
      </div>
      <div className="form-row three">
        <Field label={t("common.amount")}>
          <input required inputMode="decimal" value={form.amount} onChange={(e) => setForm({ ...form, amount: e.target.value })} />
        </Field>
        <Field label={t("common.currency")}>
          <input required maxLength={3} pattern="[A-Z]{3}" title="ISO 4217 (SAR, USD…)" dir="ltr" value={form.currency} onChange={(e) => setForm({ ...form, currency: e.target.value.toUpperCase() })} />
        </Field>
        <Field label={t("common.date")}>
          <input type="date" required value={form.distributed_on} onChange={(e) => setForm({ ...form, distributed_on: e.target.value })} />
        </Field>
      </div>
      <Field label={t("common.description")}>
        <input required value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} />
      </Field>
      {form.kind === "in_kind" && (
        <Field label={t("cases.items")} hint={t("cases.itemsHint")}>
          <textarea rows={3} value={itemsText} onChange={(e) => setItemsText(e.target.value)} />
        </Field>
      )}
      {form.kind === "cash" && (
        <label className="checkbox-label">
          <input type="checkbox" checked={form.paid_from_bank} onChange={(e) => setForm({ ...form, paid_from_bank: e.target.checked })} />
          {t("cases.paidFromBank")}
        </label>
      )}
      <FormActions busy={busy || !isPositive(form.amount)} onClose={onDone} label={t("cases.distribute")} />
    </form>
  );
}

function RedeemForm({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState({ code: "", shop: "", redeemed_on: today() });
  const { busy, run } = useSubmit();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const entry = await api.post<PostedEntry>("/aid/vouchers/redeem", { ...form, code: form.code.trim() });
      toastSuccess(t("cases.redeemed", { no: entry.entry_no }));
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} size="sm" title={t("cases.redeemVoucher")}>
      <form onSubmit={submit}>
        <Field label={t("cases.voucherCode")}>
          <input required dir="ltr" autoFocus value={form.code} onChange={(e) => setForm({ ...form, code: e.target.value })} />
        </Field>
        <Field label={t("cases.shop")}>
          <input required value={form.shop} onChange={(e) => setForm({ ...form, shop: e.target.value })} />
        </Field>
        <Field label={t("common.date")}>
          <input type="date" required value={form.redeemed_on} onChange={(e) => setForm({ ...form, redeemed_on: e.target.value })} />
        </Field>
        <FormActions busy={busy} onClose={onClose} label={t("cases.redeem")} />
      </form>
    </Modal>
  );
}
