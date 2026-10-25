import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Clock, Coins, Plus, Timer, UserCheck, Users } from "lucide-react";
import { api, newRequestKey } from "../api/client";
import type { HoursRow, Volunteer, VolunteerSummary } from "../api/types";
import Modal from "../components/Modal";
import { toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, StatCard, Tabs, useLoad, useSubmit } from "../components/ui";
import { FormActions } from "../components/forms";
import { useAuthStore } from "../store/auth";
import EditRecord, { EditButton } from "../components/EditRecord";
import { localName, useRefs } from "../store/refs";
import { formatMoney, fromUnits, isPositive, today, toUnits } from "../utils/money";

/** Default hourly value for the economic-value KPI; editable on the page, remembered per browser. */
const DEFAULT_HOURLY_VALUE = "50";
const RATE_KEY = "volunteer-hour-value";

function readRate(): string {
  try {
    return localStorage.getItem(RATE_KEY) || DEFAULT_HOURLY_VALUE;
  } catch {
    return DEFAULT_HOURLY_VALUE;
  }
}

export default function Volunteers() {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [tab, setTab] = useState<"registry" | "hours">("registry");
  const [editingVolunteer, setEditingVolunteer] = useState<Volunteer | null>(null);
  const [creating, setCreating] = useState(false);
  const [logging, setLogging] = useState<Volunteer | "pick" | null>(null);
  const [from, setFrom] = useState(`${new Date().getFullYear()}-01-01`);
  const [to, setTo] = useState(today());
  const [rate, setRate] = useState(readRate);
  const list = useLoad(() => api.get<Volunteer[]>("/volunteers", { limit: 500 }));
  const hours = useLoad(() => api.get<HoursRow[]>("/volunteers/hours", { from, to }), [from, to]);
  const summary = useLoad(() => api.get<VolunteerSummary>("/reports/volunteers-summary", { from, to }), [from, to]);
  const volunteers = list.data ?? [];
  const s = summary.data;
  // Hours keep two decimals; units of 10^-4 make hours × rate exact.
  const value = s ? fromUnits(((toUnits(s.total_hours) ?? 0n) * (toUnits(rate) ?? 0n)) / 10000n) : "0";

  const reloadAll = () => {
    list.reload();
    hours.reload();
    summary.reload();
  };

  return (
    <div className="page">
      <PageHeader title={t("volunteers.title")}>
        <Field label={t("common.from")}>
          <input type="date" value={from} max={to} onChange={(e) => e.target.value && setFrom(e.target.value)} />
        </Field>
        <Field label={t("common.to")}>
          <input type="date" value={to} min={from} onChange={(e) => e.target.value && setTo(e.target.value)} />
        </Field>
        {can("volunteers.hours") && (
          <button type="button" className="btn btn-secondary" onClick={() => setLogging("pick")} disabled={!volunteers.some((v) => v.status === "active")}>
            <Timer size={16} /> {t("volunteers.logHours")}
          </button>
        )}
        {can("volunteers.manage") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("volunteers.new")}
          </button>
        )}
      </PageHeader>

      <div className="stats-grid">
        <StatCard tint="assets" icon={<Users size={20} />} value={String(volunteers.length)} label={t("volunteers.registered")} />
        <StatCard tint="income" icon={<UserCheck size={20} />} value={String(s?.volunteers ?? 0)} label={t("volunteers.activeInPeriod")} />
        <StatCard tint="equity" icon={<Clock size={20} />} value={formatMoney(s?.total_hours ?? "0", undefined, i18n.language)} label={t("volunteers.hoursInPeriod")} />
        <StatCard tint="liabilities" icon={<Coins size={20} />} value={formatMoney(value, baseCurrency, i18n.language)} label={t("volunteers.economicValue")} />
      </div>
      <div className="section-actions">
        <Field label={t("volunteers.hourValue", { currency: baseCurrency })} hint={t("volunteers.hourValueHint")}>
          <input
            inputMode="decimal"
            value={rate}
            onChange={(e) => {
              setRate(e.target.value);
              try {
                localStorage.setItem(RATE_KEY, e.target.value);
              } catch {
                /* private mode */
              }
            }}
          />
        </Field>
      </div>

      <Tabs
        active={tab}
        onChange={setTab}
        tabs={[
          { key: "registry", label: t("volunteers.registry") },
          { key: "hours", label: t("volunteers.hoursLog") },
        ]}
      />
      {(list.error || hours.error) && <ErrorNote message={(list.error ?? hours.error)!} onRetry={reloadAll} />}
      {tab === "registry" ? (
        <DataTable
          rows={volunteers}
          rowKey={(v) => v.id}
          exportName="volunteers"
          empty={t("volunteers.empty")}
          onRowClick={can("volunteers.hours") ? (v) => v.status === "active" && setLogging(v) : undefined}
          columns={[
            { header: "#", cell: (v) => String(v.volunteer_no), num: true },
            { header: t("common.name"), cell: (v) => v.full_name },
            { header: t("common.phone"), cell: (v) => v.phone ?? "" },
            { header: t("cases.city"), cell: (v) => v.city ?? "" },
            { header: t("volunteers.skills"), cell: (v) => v.skills.join("، ") },
            { header: t("volunteers.totalHours"), cell: (v) => formatMoney(v.total_hours, undefined, i18n.language), text: (v) => v.total_hours, num: true },
            { header: t("volunteers.lastActivity"), cell: (v) => v.last_activity ?? "—" },
            {
              header: t("common.status"),
              text: (v) => v.status,
              cell: (v) => (
                <span className="action-btns">
                  <Badge tone={v.status === "active" ? "success" : "secondary"}>{t(`volunteers.status.${v.status}`)}</Badge>
                  {can("volunteers.manage") && (
                    <button
                      type="button"
                      className="btn btn-sm btn-ghost"
                      onClick={(e) => {
                        e.stopPropagation();
                        api.post(`/volunteers/${v.id}/status`, { status: v.status === "active" ? "inactive" : "active" }).then(reloadAll, reloadAll);
                      }}
                    >
                      {v.status === "active" ? t("volunteers.deactivate") : t("volunteers.activate")}
                    </button>
                  )}
                  {can("volunteers.manage") && <EditButton onClick={() => setEditingVolunteer(v)} />}
                </span>
              ),
            },
          ]}
        />
      ) : (
        <DataTable
          rows={hours.data ?? []}
          rowKey={(h) => String(h.id)}
          exportName={`volunteer-hours-${from}-${to}`}
          empty={t("common.noData")}
          columns={[
            { header: t("common.date"), cell: (h) => h.worked_on },
            { header: t("volunteers.volunteer"), cell: (h) => h.volunteer_name },
            { header: t("volunteers.activity"), cell: (h) => h.activity },
            { header: t("journal.project"), cell: (h) => h.project_code ?? "—" },
            { header: t("volunteers.hours"), cell: (h) => h.hours, num: true },
            { header: t("common.notes"), cell: (h) => h.notes ?? "" },
          ]}
        />
      )}

      {creating && <VolunteerForm onClose={() => setCreating(false)} onSaved={reloadAll} />}
      {editingVolunteer && (
        <EditRecord
          title={t("volunteers.edit")}
          path={`/volunteers/${editingVolunteer.id}`}
          initial={editingVolunteer}
          onClose={() => setEditingVolunteer(null)}
          onSaved={reloadAll}
          fields={[
            { key: "full_name", label: t("common.name"), required: true },
            { key: "national_id", label: t("donors.nationalId") },
            { key: "phone", label: t("common.phone"), type: "tel", dir: "ltr" },
            { key: "email", label: t("common.email"), type: "email", dir: "ltr" },
            { key: "city", label: t("cases.city") },
            { key: "skills", label: t("volunteers.skills"), type: "tags", hint: t("volunteers.skillsHint") },
            { key: "joined_on", label: t("volunteers.joined"), type: "date", required: true },
            { key: "notes", label: t("common.notes"), type: "textarea" },
          ]}
        />
      )}
      {logging && <HoursForm volunteers={volunteers.filter((v) => v.status === "active")} preset={logging === "pick" ? null : logging} onClose={() => setLogging(null)} onSaved={reloadAll} />}
    </div>
  );
}

function VolunteerForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState({ full_name: "", national_id: "", phone: "", email: "", city: "", skills: "", joined_on: today(), notes: "" });
  const { busy, run } = useSubmit();
  const opt = (v: string) => v.trim() || null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.post("/volunteers", {
        full_name: form.full_name,
        national_id: opt(form.national_id),
        phone: opt(form.phone),
        email: opt(form.email),
        city: opt(form.city),
        skills: form.skills.split(/[,،]/).map((x) => x.trim()).filter(Boolean),
        joined_on: form.joined_on,
        notes: opt(form.notes),
      });
      toastSuccess(t("common.saved"));
      onSaved();
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} title={t("volunteers.new")}>
      <form onSubmit={submit}>
        <div className="form-row">
          <Field label={t("common.name")}>
            <input required value={form.full_name} onChange={(e) => setForm({ ...form, full_name: e.target.value })} />
          </Field>
          <Field label={t("donors.nationalId")}>
            <input value={form.national_id} onChange={(e) => setForm({ ...form, national_id: e.target.value })} />
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("common.phone")}>
            <input type="tel" dir="ltr" value={form.phone} onChange={(e) => setForm({ ...form, phone: e.target.value })} />
          </Field>
          <Field label={t("common.email")}>
            <input type="email" dir="ltr" value={form.email} onChange={(e) => setForm({ ...form, email: e.target.value })} />
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("cases.city")}>
            <input value={form.city} onChange={(e) => setForm({ ...form, city: e.target.value })} />
          </Field>
          <Field label={t("volunteers.joined")}>
            <input type="date" required max={today()} value={form.joined_on} onChange={(e) => setForm({ ...form, joined_on: e.target.value })} />
          </Field>
        </div>
        <Field label={t("volunteers.skills")} hint={t("archive.tagsHint")}>
          <input value={form.skills} onChange={(e) => setForm({ ...form, skills: e.target.value })} />
        </Field>
        <Field label={t("common.notes")}>
          <textarea rows={2} value={form.notes} onChange={(e) => setForm({ ...form, notes: e.target.value })} />
        </Field>
        <FormActions busy={busy} onClose={onClose} />
      </form>
    </Modal>
  );
}

function HoursForm({ volunteers, preset, onClose, onSaved }: { volunteers: Volunteer[]; preset: Volunteer | null; onClose: () => void; onSaved: () => void }) {
  const { t, i18n } = useTranslation();
  const projects = useRefs((s) => s.projects);
  const [form, setForm] = useState({ volunteer_id: preset?.id ?? volunteers[0]?.id ?? "", activity: "", project_id: "", worked_on: today(), hours: "", notes: "" });
  const [requestKey] = useState(newRequestKey);
  const { busy, run } = useSubmit();
  const valid = isPositive(form.hours) && (toUnits(form.hours) ?? 0n) <= 240000n;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.post(
        "/volunteers/hours",
        { volunteer_id: form.volunteer_id, activity: form.activity, project_id: form.project_id || null, worked_on: form.worked_on, hours: form.hours, notes: form.notes.trim() || null },
        requestKey,
      );
      toastSuccess(t("volunteers.hoursLogged"));
      onSaved();
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} title={t("volunteers.logHours")}>
      <form onSubmit={submit}>
        <Field label={t("volunteers.volunteer")}>
          <select required value={form.volunteer_id} onChange={(e) => setForm({ ...form, volunteer_id: e.target.value })}>
            {volunteers.map((v) => (
              <option key={v.id} value={v.id}>
                #{v.volunteer_no} · {v.full_name}
              </option>
            ))}
          </select>
        </Field>
        <div className="form-row">
          <Field label={t("volunteers.activity")}>
            <input required value={form.activity} onChange={(e) => setForm({ ...form, activity: e.target.value })} />
          </Field>
          <Field label={t("journal.project")}>
            <select value={form.project_id} onChange={(e) => setForm({ ...form, project_id: e.target.value })}>
              <option value="">{t("journal.noProject")}</option>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.code} · {localName(p, i18n.language)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("common.date")}>
            <input type="date" required max={today()} value={form.worked_on} onChange={(e) => setForm({ ...form, worked_on: e.target.value })} />
          </Field>
          <Field label={t("volunteers.hours")} hint={t("volunteers.hoursHint")}>
            <input required inputMode="decimal" value={form.hours} onChange={(e) => setForm({ ...form, hours: e.target.value })} />
          </Field>
        </div>
        <Field label={t("common.notes")}>
          <input value={form.notes} onChange={(e) => setForm({ ...form, notes: e.target.value })} />
        </Field>
        <FormActions busy={busy || !valid || !form.volunteer_id} onClose={onClose} />
      </form>
    </Modal>
  );
}
