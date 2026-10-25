import { useMemo, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { api } from "../api/client";
import { ACCOUNT_TYPES, type Account, type NewAccount, type TrialBalanceRow } from "../api/types";
import Modal from "../components/Modal";
import { toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, Tabs, useLoad, useSubmit } from "../components/ui";
import { useAuthStore } from "../store/auth";
import EditRecord, { EditButton } from "../components/EditRecord";
import { localName, useRefs } from "../store/refs";
import { formatMoney, sum, today } from "../utils/money";

const FUNCTIONAL = ["program", "management_general", "fundraising"];

/** Orders accounts depth-first so children sit under their parent. */
function treeOrder(accounts: Account[]): { a: Account; depth: number }[] {
  const byParent = new Map<string | null, Account[]>();
  for (const a of accounts) byParent.set(a.parent_id, [...(byParent.get(a.parent_id) ?? []), a]);
  const out: { a: Account; depth: number }[] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const a of (byParent.get(parent) ?? []).sort((x, y) => x.code.localeCompare(y.code))) {
      out.push({ a, depth });
      walk(a.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

export default function Accounts() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"chart" | "trial">("chart");
  return (
    <div className="page">
      <PageHeader title={t("accounts.title")} />
      <Tabs
        active={tab}
        onChange={setTab}
        tabs={[
          { key: "chart", label: t("accounts.chart") },
          { key: "trial", label: t("accounts.trialBalance") },
        ]}
      />
      {tab === "chart" ? <Chart /> : <TrialBalance />}
    </div>
  );
}

function Chart() {
  const { t, i18n } = useTranslation();
  const { accounts, load } = useRefs();
  const can = useAuthStore((s) => s.can);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Account | null>(null);
  const rows = useMemo(() => treeOrder(accounts), [accounts]);

  return (
    <>
      {can("accounts.manage") && (
        <div className="section-actions">
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("accounts.new")}
          </button>
        </div>
      )}
      <DataTable
        rows={rows}
        rowKey={(r) => r.a.id}
        exportName="chart-of-accounts"
        columns={[
          { header: t("common.code"), cell: (r) => r.a.code },
          {
            header: t("common.name"),
            text: (r) => localName(r.a, i18n.language),
            cell: (r) => (
              <span style={{ paddingInlineStart: r.depth * 20, fontWeight: r.a.is_postable ? 400 : 700 }}>{localName(r.a, i18n.language)}</span>
            ),
          },
          { header: t("accounts.type"), cell: (r) => t(`accountTypes.${r.a.account_type}`) },
          { header: t("accounts.functionalClass"), cell: (r) => (r.a.functional_class ? t(`functional.${r.a.functional_class}`) : "") },
          {
            header: t("accounts.postable"),
            text: (r) => (r.a.is_postable ? t("common.yes") : t("common.no")),
            cell: (r) => <Badge tone={r.a.is_postable ? "info" : "secondary"}>{r.a.is_postable ? t("accounts.detail") : t("accounts.header")}</Badge>,
          },
          {
            header: t("common.status"),
            text: (r) => (r.a.is_active ? t("common.active") : t("common.inactive")),
            cell: (r) => (
              <span className="action-btns">
                {!r.a.is_active && <Badge tone="secondary">{t("common.inactive")}</Badge>}
                {can("accounts.manage") && <EditButton onClick={() => setEditing(r.a)} />}
              </span>
            ),
          },
        ]}
      />
      {editing && (
        <EditRecord
          title={t("accounts.edit", { code: editing.code })}
          path={`/accounts/${editing.id}`}
          initial={editing}
          note={t("accounts.editNote")}
          onClose={() => setEditing(null)}
          onSaved={() => load(true)}
          fields={[
            { key: "name_ar", label: t("common.nameAr"), required: true },
            { key: "name_en", label: t("common.nameEn"), required: true, dir: "ltr" },
            { key: "is_active", label: t("common.active"), type: "checkbox" },
          ]}
        />
      )}
      {creating && <AccountForm accounts={accounts} onClose={() => setCreating(false)} onSaved={() => load(true)} />}
    </>
  );
}

function AccountForm({ accounts, onClose, onSaved }: { accounts: Account[]; onClose: () => void; onSaved: () => void }) {
  const { t, i18n } = useTranslation();
  const [form, setForm] = useState<NewAccount>({ code: "", name_ar: "", name_en: "", account_type: "expense", parent_id: null, is_postable: true, functional_class: null });
  const { busy, run } = useSubmit();
  const parents = accounts.filter((a) => !a.is_postable && a.account_type === form.account_type);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.post("/accounts", { ...form, functional_class: form.account_type === "expense" ? form.functional_class : null });
      toastSuccess(t("common.saved"));
      onSaved();
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} title={t("accounts.new")}>
      <form onSubmit={submit}>
        <div className="form-row">
          <Field label={t("common.code")}>
            <input required inputMode="numeric" pattern="[0-9]{1,10}" maxLength={10} title={t("validation.accountCode")} dir="ltr" value={form.code} onChange={(e) => setForm({ ...form, code: e.target.value.replace(/\D/g, "") })} />
          </Field>
          <Field label={t("accounts.type")}>
            <select value={form.account_type} onChange={(e) => setForm({ ...form, account_type: e.target.value as NewAccount["account_type"], parent_id: null })}>
              {ACCOUNT_TYPES.map((k) => (
                <option key={k} value={k}>
                  {t(`accountTypes.${k}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("common.nameAr")}>
            <input required dir="rtl" value={form.name_ar} onChange={(e) => setForm({ ...form, name_ar: e.target.value })} />
          </Field>
          <Field label={t("common.nameEn")}>
            <input required dir="ltr" value={form.name_en} onChange={(e) => setForm({ ...form, name_en: e.target.value })} />
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("accounts.parent")}>
            <select value={form.parent_id ?? ""} onChange={(e) => setForm({ ...form, parent_id: e.target.value || null })}>
              <option value="">—</option>
              {parents.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.code} {localName(a, i18n.language)}
                </option>
              ))}
            </select>
          </Field>
          {form.account_type === "expense" && (
            <Field label={t("accounts.functionalClass")}>
              <select value={form.functional_class ?? ""} onChange={(e) => setForm({ ...form, functional_class: e.target.value || null })}>
                <option value="">—</option>
                {FUNCTIONAL.map((f) => (
                  <option key={f} value={f}>
                    {t(`functional.${f}`)}
                  </option>
                ))}
              </select>
            </Field>
          )}
        </div>
        <label className="checkbox-label">
          <input type="checkbox" checked={form.is_postable} onChange={(e) => setForm({ ...form, is_postable: e.target.checked })} />
          {t("accounts.postableHint")}
        </label>
        <div className="form-actions">
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? t("common.saving") : t("common.save")}
          </button>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            {t("common.cancel")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function TrialBalance() {
  const { t, i18n } = useTranslation();
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [asOf, setAsOf] = useState(today());
  const { data, error, reload } = useLoad(() => api.get<TrialBalanceRow[]>("/reports/trial-balance", { as_of: asOf }), [asOf]);
  const rows = data ?? [];
  const dr = sum(rows.map((r) => r.debit));
  const cr = sum(rows.map((r) => r.credit));
  const m = (v: string) => formatMoney(v, undefined, i18n.language);

  return (
    <>
      <div className="section-actions">
        <Field label={t("dashboard.asOf")}>
          <input type="date" value={asOf} onChange={(e) => e.target.value && setAsOf(e.target.value)} />
        </Field>
      </div>
      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={rows}
        rowKey={(r) => r.account_id}
        exportName={`trial-balance-${asOf}`}
        columns={[
          { header: t("common.code"), cell: (r) => r.code },
          { header: t("common.name"), cell: (r) => localName(r, i18n.language) },
          { header: t("accounts.type"), cell: (r) => t(`accountTypes.${r.account_type}`) },
          { header: `${t("journal.debit")} (${baseCurrency})`, cell: (r) => m(r.debit), text: (r) => r.debit, num: true },
          { header: `${t("journal.credit")} (${baseCurrency})`, cell: (r) => m(r.credit), text: (r) => r.credit, num: true },
        ]}
      />
      <div className="totals-bar">
        <span>{t("journal.debit")}: <strong>{m(dr)}</strong></span>
        <span>{t("journal.credit")}: <strong>{m(cr)}</strong></span>
        <Badge tone={dr === cr ? "success" : "danger"}>{dr === cr ? t("journal.balanced") : t("journal.unbalanced")}</Badge>
      </div>
    </>
  );
}
