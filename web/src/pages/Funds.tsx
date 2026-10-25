import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { api } from "../api/client";
import { DONATION_KINDS, RESTRICTIONS, type BudgetUsage, type FundBalance, type NewBudget, type NewFund, type NewProject } from "../api/types";
import Modal from "../components/Modal";
import { toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, Tabs, useLoad, useSubmit } from "../components/ui";
import { FormActions, FundSelect } from "../components/forms";
import EditRecord, { EditButton, type FieldSpec } from "../components/EditRecord";
import { useAuthStore } from "../store/auth";
import { localName, useRefs } from "../store/refs";
import { formatMoney, isPositive, today, toUnits } from "../utils/money";

type Tab = "funds" | "projects" | "budgets";

export default function Funds() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("funds");
  return (
    <div className="page">
      <PageHeader title={t("funds.title")} />
      <Tabs
        active={tab}
        onChange={setTab}
        tabs={[
          { key: "funds", label: t("funds.funds") },
          { key: "projects", label: t("funds.projects") },
          { key: "budgets", label: t("funds.budgets") },
        ]}
      />
      {tab === "funds" && <FundsTab />}
      {tab === "projects" && <ProjectsTab />}
      {tab === "budgets" && <BudgetsTab />}
    </div>
  );
}

function useNameFields(): FieldSpec[] {
  const { t } = useTranslation();
  return [
    { key: "name_ar", label: t("common.nameAr"), required: true },
    { key: "name_en", label: t("common.nameEn"), required: true, dir: "ltr" },
  ];
}

function restrictionTone(r: string) {
  return r === "unrestricted" ? "success" : r === "permanently_restricted" ? "danger" : "warning";
}

function FundsTab() {
  const { t, i18n } = useTranslation();
  const { funds, baseCurrency, load } = useRefs();
  const can = useAuthStore((s) => s.can);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<(typeof funds)[number] | null>(null);
  const nameFields = useNameFields();
  const { data: balances, error, reload } = useLoad(() => api.get<FundBalance[]>("/reports/fund-balances"));
  const bal = (id: string) => balances?.find((b) => b.fund_id === id);
  const m = (v?: string) => formatMoney(v ?? "0", baseCurrency, i18n.language);

  return (
    <>
      {can("funds.manage") && (
        <div className="section-actions">
          <button type="button" className="btn btn-primary" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("funds.newFund")}
          </button>
        </div>
      )}
      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={funds}
        rowKey={(f) => f.id}
        exportName="funds"
        columns={[
          { header: t("common.code"), cell: (f) => f.code },
          { header: t("common.name"), cell: (f) => localName(f, i18n.language) },
          {
            header: t("funds.restriction"),
            text: (f) => t(`restrictions.${f.restriction}`),
            cell: (f) => <Badge tone={restrictionTone(f.restriction)}>{t(`restrictions.${f.restriction}`)}</Badge>,
          },
          { header: t("funds.donationKind"), cell: (f) => (f.donation_kind ? t(`kinds.${f.donation_kind}`) : "—") },
          { header: t("funds.revenue"), cell: (f) => m(bal(f.id)?.revenue), num: true },
          { header: t("funds.expenses"), cell: (f) => m(bal(f.id)?.expenses), num: true },
          { header: t("funds.netAssets"), cell: (f) => m(bal(f.id)?.net_assets), num: true },
          ...(can("funds.manage") ? [{ header: "", text: () => "", cell: (f: (typeof funds)[number]) => <EditButton onClick={() => setEditing(f)} /> }] : []),
        ]}
      />
      {creating && <FundForm onClose={() => setCreating(false)} onSaved={() => (load(true), reload())} />}
      {editing && (
        <EditRecord
          title={t("funds.editFund", { code: editing.code })}
          path={`/funds/${editing.id}`}
          initial={{ name_ar: editing.name_ar, name_en: editing.name_en }}
          note={t("funds.editNote")}
          fields={nameFields}
          onClose={() => setEditing(null)}
          onSaved={() => load(true)}
        />
      )}
    </>
  );
}

function FundForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState<NewFund>({ code: "", name_ar: "", name_en: "", restriction: "temporarily_restricted", donation_kind: null, purpose: null });
  const { busy, run } = useSubmit();
  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.post("/funds", form);
      toastSuccess(t("common.saved"));
      onSaved();
      onClose();
    });
  };
  return (
    <Modal isOpen onClose={onClose} title={t("funds.newFund")}>
      <form onSubmit={submit}>
        <div className="form-row">
          <Field label={t("common.code")}>
            <input required value={form.code} onChange={(e) => setForm({ ...form, code: e.target.value.toUpperCase() })} />
          </Field>
          <Field label={t("funds.restriction")}>
            <select value={form.restriction} onChange={(e) => setForm({ ...form, restriction: e.target.value as NewFund["restriction"] })}>
              {RESTRICTIONS.map((r) => (
                <option key={r} value={r}>
                  {t(`restrictions.${r}`)}
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
        <Field label={t("funds.donationKind")} hint={t("funds.donationKindHint")}>
          <select value={form.donation_kind ?? ""} onChange={(e) => setForm({ ...form, donation_kind: (e.target.value || null) as NewFund["donation_kind"] })}>
            <option value="">{t("funds.anyKind")}</option>
            {DONATION_KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`kinds.${k}`)}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("funds.purpose")}>
          <textarea rows={2} value={form.purpose ?? ""} onChange={(e) => setForm({ ...form, purpose: e.target.value || null })} />
        </Field>
        <FormActions busy={busy} onClose={onClose} />
      </form>
    </Modal>
  );
}

function ProjectsTab() {
  const { t, i18n } = useTranslation();
  const { projects, funds, load } = useRefs();
  const can = useAuthStore((s) => s.can);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<(typeof projects)[number] | null>(null);
  const nameFields = useNameFields();
  const [form, setForm] = useState<NewProject>({ code: "", name_ar: "", name_en: "", fund_id: funds[0]?.id ?? "" });
  const { busy, run } = useSubmit();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.post("/projects", form);
      toastSuccess(t("common.saved"));
      await load(true);
      setCreating(false);
    });
  };

  return (
    <>
      {can("funds.manage") && (
        <div className="section-actions">
          <button type="button" className="btn btn-primary" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("funds.newProject")}
          </button>
        </div>
      )}
      <DataTable
        rows={projects}
        rowKey={(p) => p.id}
        exportName="projects"
        columns={[
          { header: t("common.code"), cell: (p) => p.code },
          { header: t("common.name"), cell: (p) => localName(p, i18n.language) },
          { header: t("journal.fund"), cell: (p) => localName(funds.find((f) => f.id === p.fund_id), i18n.language) },
          ...(can("funds.manage") ? [{ header: "", text: () => "", cell: (p: (typeof projects)[number]) => <EditButton onClick={() => setEditing(p)} /> }] : []),
        ]}
      />
      {editing && (
        <EditRecord
          title={t("funds.editProject", { code: editing.code })}
          path={`/projects/${editing.id}`}
          initial={{ name_ar: editing.name_ar, name_en: editing.name_en }}
          fields={nameFields}
          onClose={() => setEditing(null)}
          onSaved={() => load(true)}
        />
      )}
      {creating && (
        <Modal isOpen onClose={() => setCreating(false)} title={t("funds.newProject")}>
          <form onSubmit={submit}>
            <div className="form-row">
              <Field label={t("common.code")}>
                <input required value={form.code} onChange={(e) => setForm({ ...form, code: e.target.value.toUpperCase() })} />
              </Field>
              <Field label={t("journal.fund")}>
                <FundSelect value={form.fund_id} onChange={(fund_id) => setForm({ ...form, fund_id })} />
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
            <FormActions busy={busy} onClose={() => setCreating(false)} />
          </form>
        </Modal>
      )}
    </>
  );
}

function BudgetsTab() {
  const { t, i18n } = useTranslation();
  const { funds, projects, baseCurrency } = useRefs();
  const can = useAuthStore((s) => s.can);
  const [asOf, setAsOf] = useState(today());
  const [editing, setEditing] = useState<BudgetUsage | null>(null);
  const [creating, setCreating] = useState(false);
  const year = new Date().getFullYear();
  const [form, setForm] = useState<NewBudget>({ fund_id: funds[0]?.id ?? "", project_id: null, period_start: `${year}-01-01`, period_end: `${year}-12-31`, approved: "", committed: "0" });
  const { busy, run } = useSubmit();
  const { data, error, reload } = useLoad(() => api.get<BudgetUsage[]>("/budgets", { as_of: asOf }), [asOf]);
  const m = (v: string) => formatMoney(v, baseCurrency, i18n.language);

  const usage = (b: BudgetUsage) => {
    const approved = toUnits(b.approved) ?? 0n;
    const used = (toUnits(b.spent) ?? 0n) + (toUnits(b.committed) ?? 0n);
    return approved > 0n ? Number((used * 1000n) / approved) / 10 : 0;
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.post("/budgets", form);
      toastSuccess(t("common.saved"));
      reload();
      setCreating(false);
    });
  };

  return (
    <>
      <div className="section-actions">
        <Field label={t("dashboard.asOf")}>
          <input type="date" value={asOf} onChange={(e) => e.target.value && setAsOf(e.target.value)} />
        </Field>
        {can("budgets.manage") && (
          <button type="button" className="btn btn-primary" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("funds.newBudget")}
          </button>
        )}
      </div>
      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={data ?? []}
        rowKey={(b) => b.id}
        exportName="budgets"
        columns={[
          { header: t("journal.fund"), cell: (b) => b.fund_code },
          { header: t("journal.project"), cell: (b) => b.project_code ?? "—" },
          { header: t("funds.period"), cell: (b) => `${b.period_start} → ${b.period_end}` },
          { header: t("funds.approved"), cell: (b) => m(b.approved), text: (b) => b.approved, num: true },
          { header: t("funds.committed"), cell: (b) => m(b.committed), text: (b) => b.committed, num: true },
          { header: t("funds.spent"), cell: (b) => m(b.spent), text: (b) => b.spent, num: true },
          {
            header: t("funds.usage"),
            text: (b) => `${usage(b).toFixed(1)}%`,
            cell: (b) => {
              const p = usage(b);
              return <Badge tone={p >= 100 ? "danger" : p >= 80 ? "warning" : "success"}>{p.toFixed(1)}%</Badge>;
            },
          },
          ...(can("budgets.manage") ? [{ header: "", text: () => "", cell: (b: BudgetUsage) => <EditButton onClick={() => setEditing(b)} /> }] : []),
        ]}
      />
      {editing && (
        <EditRecord
          title={t("funds.editBudget", { fund: editing.fund_code })}
          path={`/budgets/${editing.id}`}
          initial={editing}
          onClose={() => setEditing(null)}
          onSaved={reload}
          fields={[
            { key: "period_end", label: t("funds.periodEnd"), type: "date", required: true },
            { key: "approved", label: t("funds.approved"), type: "number", step: "0.01", required: true, dir: "ltr" },
            { key: "committed", label: t("funds.committed"), type: "number", step: "0.01", required: true, dir: "ltr" },
          ]}
        />
      )}
      {creating && (
        <Modal isOpen onClose={() => setCreating(false)} title={t("funds.newBudget")}>
          <form onSubmit={submit}>
            <div className="form-row">
              <Field label={t("journal.fund")}>
                <FundSelect value={form.fund_id} onChange={(fund_id) => setForm({ ...form, fund_id, project_id: null })} />
              </Field>
              <Field label={t("journal.project")}>
                <select value={form.project_id ?? ""} onChange={(e) => setForm({ ...form, project_id: e.target.value || null })}>
                  <option value="">{t("journal.noProject")}</option>
                  {projects.filter((p) => p.fund_id === form.fund_id).map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.code} {localName(p, i18n.language)}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
            <div className="form-row">
              <Field label={t("common.from")}>
                <input type="date" required value={form.period_start} onChange={(e) => setForm({ ...form, period_start: e.target.value })} />
              </Field>
              <Field label={t("common.to")}>
                <input type="date" required value={form.period_end} min={form.period_start} onChange={(e) => setForm({ ...form, period_end: e.target.value })} />
              </Field>
            </div>
            <div className="form-row">
              <Field label={`${t("funds.approved")} (${baseCurrency})`}>
                <input required inputMode="decimal" value={form.approved} onChange={(e) => setForm({ ...form, approved: e.target.value })} />
              </Field>
              <Field label={`${t("funds.committed")} (${baseCurrency})`}>
                <input inputMode="decimal" value={form.committed} onChange={(e) => setForm({ ...form, committed: e.target.value })} />
              </Field>
            </div>
            <FormActions busy={busy || !isPositive(form.approved)} onClose={() => setCreating(false)} />
          </form>
        </Modal>
      )}
    </>
  );
}
