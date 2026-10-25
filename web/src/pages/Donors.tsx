import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Search } from "lucide-react";
import { api } from "../api/client";
import type { DonationRow, Donor, NewDonor } from "../api/types";
import Modal from "../components/Modal";
import { toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, useLoad, useSubmit } from "../components/ui";
import { FormActions } from "../components/forms";
import EditRecord, { EditButton } from "../components/EditRecord";
import { useAuthStore } from "../store/auth";
import { useRefs } from "../store/refs";
import { formatMoney, sum } from "../utils/money";

/** Debounce delay before a typed query hits the server-side donor search. */
const SEARCH_DELAY_MS = 300;

export default function Donors() {
  const { t } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const [q, setQ] = useState("");
  const [query, setQuery] = useState("");
  const [creating, setCreating] = useState(false);
  const [viewing, setViewing] = useState<Donor | null>(null);

  useEffect(() => {
    const id = setTimeout(() => setQuery(q.trim()), SEARCH_DELAY_MS);
    return () => clearTimeout(id);
  }, [q]);

  const { data, error, reload } = useLoad(() => api.get<Donor[]>("/donors", { q: query, limit: 500 }), [query]);

  return (
    <div className="page">
      <PageHeader title={t("donors.title")}>
        {can("donors.manage") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("donors.new")}
          </button>
        )}
      </PageHeader>

      <div className="toolbar">
        <div className="search-box">
          <Search size={16} />
          <input type="search" placeholder={t("donors.searchHint")} value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
      </div>

      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        searchable={false}
        rows={data ?? []}
        rowKey={(d) => d.id}
        exportName="donors"
        onRowClick={setViewing}
        columns={[
          { header: t("common.name"), cell: (d) => d.full_name },
          {
            header: t("donors.type"),
            text: (d) => (d.is_organization ? t("donors.organization") : t("donors.individual")),
            cell: (d) => <Badge tone={d.is_organization ? "info" : "secondary"}>{d.is_organization ? t("donors.organization") : t("donors.individual")}</Badge>,
          },
          { header: t("donors.nationalId"), cell: (d) => d.national_id ?? "" },
          { header: t("common.phone"), cell: (d) => d.phone ?? "" },
          { header: t("common.email"), cell: (d) => d.email ?? "" },
          { header: t("donors.country"), cell: (d) => d.country ?? "" },
        ]}
      />

      {creating && <DonorForm onClose={() => setCreating(false)} onSaved={reload} />}
      {viewing && (
        <DonorModal
          donor={viewing}
          onClose={() => setViewing(null)}
          onChanged={(d) => {
            setViewing(d);
            reload();
          }}
        />
      )}
    </div>
  );
}

export function DonorForm({ onClose, onSaved }: { onClose: () => void; onSaved: (id: string) => void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState<NewDonor>({
    is_organization: false,
    full_name: "",
    national_id: null,
    phone: null,
    email: null,
    country: "SA",
    preferred_language: "ar",
    notes: null,
  });
  const { busy, run } = useSubmit();
  const opt = (v: string) => v.trim() || null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const r = await api.post<{ id: string }>("/donors", form);
      toastSuccess(t("common.saved"));
      onSaved(r.id);
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} title={t("donors.new")}>
      <form onSubmit={submit}>
        <div className="form-row">
          <Field label={t("common.name")}>
            <input required value={form.full_name} onChange={(e) => setForm({ ...form, full_name: e.target.value })} />
          </Field>
          <Field label={t("donors.type")}>
            <select value={form.is_organization ? "org" : "ind"} onChange={(e) => setForm({ ...form, is_organization: e.target.value === "org" })}>
              <option value="ind">{t("donors.individual")}</option>
              <option value="org">{t("donors.organization")}</option>
            </select>
          </Field>
        </div>
        <div className="form-row">
          <Field label={form.is_organization ? t("donors.crNumber") : t("donors.nationalId")}>
            <input value={form.national_id ?? ""} onChange={(e) => setForm({ ...form, national_id: opt(e.target.value) })} />
          </Field>
          <Field label={t("common.phone")}>
            <input type="tel" dir="ltr" value={form.phone ?? ""} onChange={(e) => setForm({ ...form, phone: opt(e.target.value) })} />
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("common.email")}>
            <input type="email" dir="ltr" value={form.email ?? ""} onChange={(e) => setForm({ ...form, email: opt(e.target.value) })} />
          </Field>
          <Field label={t("donors.country")}>
            <input maxLength={2} dir="ltr" value={form.country ?? ""} onChange={(e) => setForm({ ...form, country: opt(e.target.value.toUpperCase()) })} />
          </Field>
        </div>
        <Field label={t("donors.language")}>
          <select value={form.preferred_language} onChange={(e) => setForm({ ...form, preferred_language: e.target.value })}>
            <option value="ar">العربية</option>
            <option value="en">English</option>
          </select>
        </Field>
        <Field label={t("common.notes")}>
          <textarea rows={2} value={form.notes ?? ""} onChange={(e) => setForm({ ...form, notes: opt(e.target.value) })} />
        </Field>
        <FormActions busy={busy} onClose={onClose} />
      </form>
    </Modal>
  );
}

function DonorModal({ donor, onClose, onChanged }: { donor: Donor; onClose: () => void; onChanged: (d: Donor) => void }) {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const [editing, setEditing] = useState(false);
  const baseCurrency = useRefs((s) => s.baseCurrency);
  // All-time statement: from the first possible date to today.
  const { data, error } = useLoad(() => api.get<DonationRow[]>("/donations", { donor_id: donor.id, from: "1900-01-01", limit: 500 }), [donor.id]);
  const rows = data ?? [];

  return (
    <Modal isOpen onClose={onClose} size="lg" title={donor.full_name}>
      <div className="detail-grid">
        <div><span>{t("donors.nationalId")}</span><strong>{donor.national_id ?? "—"}</strong></div>
        <div><span>{t("common.phone")}</span><strong dir="ltr">{donor.phone ?? "—"}</strong></div>
        <div><span>{t("common.email")}</span><strong dir="ltr">{donor.email ?? "—"}</strong></div>
        <div><span>{t("donors.totalGiven")}</span><strong>{formatMoney(sum(rows.map((r) => r.base_amount)), baseCurrency, i18n.language)}</strong></div>
      </div>
      {donor.notes && <p className="settings-note">{donor.notes}</p>}
      {can("donors.manage") && (
        <div className="section-actions">
          <EditButton onClick={() => setEditing(true)} />
        </div>
      )}
      {editing && (
        <EditRecord
          title={t("donors.edit")}
          path={`/donors/${donor.id}`}
          initial={donor}
          onClose={() => setEditing(false)}
          onSaved={(d) => onChanged(d as Donor)}
          fields={[
            { key: "full_name", label: t("common.name"), required: true },
            {
              key: "is_organization",
              label: t("donors.organization"),
              type: "checkbox",
            },
            { key: "national_id", label: t("donors.nationalId") },
            { key: "phone", label: t("common.phone"), type: "tel", dir: "ltr" },
            { key: "email", label: t("common.email"), type: "email", dir: "ltr" },
            { key: "country", label: t("donors.country"), dir: "ltr" },
            {
              key: "preferred_language",
              label: t("donors.language"),
              type: "select",
              required: true,
              options: [
                { value: "ar", label: "العربية" },
                { value: "en", label: "English" },
              ],
            },
            { key: "notes", label: t("common.notes"), type: "textarea" },
          ]}
        />
      )}
      {error && <ErrorNote message={error} />}
      <DataTable
        searchable={false}
        rows={rows}
        rowKey={(r) => r.id}
        exportName={`donor-statement-${donor.id.slice(0, 8)}`}
        columns={[
          { header: t("donations.receiptNo"), cell: (r) => r.receipt_no },
          { header: t("common.date"), cell: (r) => r.donation_date },
          { header: t("donations.kind"), cell: (r) => t(`kinds.${r.kind}`) },
          { header: t("journal.fund"), cell: (r) => r.fund_code },
          { header: t("common.amount"), cell: (r) => formatMoney(r.amount, r.currency, i18n.language), text: (r) => r.amount, num: true },
        ]}
      />
    </Modal>
  );
}
