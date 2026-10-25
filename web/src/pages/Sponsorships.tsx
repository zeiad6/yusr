import { useMemo, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, CalendarClock, HandCoins, HeartHandshake, Pause, Play, Plus, Printer, Square } from "lucide-react";
import { api, newRequestKey } from "../api/client";
import {
  SPONSORSHIP_PROGRAMS,
  type Beneficiary,
  type DonationKind,
  type Donor,
  type NewSponsorship,
  type RecordedDonation,
  type Sponsorship,
  type SponsorshipDetail,
  type SponsorshipStatus,
} from "../api/types";
import Modal from "../components/Modal";
import { toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, Loading, PageHeader, StatCard, Tabs, useLoad, useSubmit } from "../components/ui";
import { FormActions } from "../components/forms";
import { useAuthStore } from "../store/auth";
import EditRecord, { EditButton } from "../components/EditRecord";
import { localName, useRefs } from "../store/refs";
import { formatMoney, isPositive, sum, today } from "../utils/money";
import { DonorPicker, printReceipt } from "./Donations";

/** Same list as the donation form: cash and transfers go to the right deposit account. */
const METHODS = ["cash", "bank", "transfer", "cheque", "sadad", "card"];
const STATUS_TONE: Record<SponsorshipStatus, "success" | "warning" | "secondary"> = { active: "success", paused: "warning", ended: "secondary" };

type Filter = "all" | "active" | "arrears" | "paused" | "ended";

export default function Sponsorships() {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [filter, setFilter] = useState<Filter>("active");
  const [creating, setCreating] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);
  const { data, error, reload } = useLoad(() => api.get<Sponsorship[]>("/sponsorships", { limit: 500 }));
  const all = data ?? [];
  const rows = all.filter((s) => (filter === "all" ? true : filter === "arrears" ? s.arrears_months > 0 : s.status === filter));
  const active = all.filter((s) => s.status === "active");
  const stats = useMemo(
    () => ({
      monthly: sum(active.filter((s) => s.currency === baseCurrency).map((s) => s.monthly_amount)),
      arrears: active.filter((s) => s.arrears_months > 0).length,
      collected: sum(all.filter((s) => s.currency === baseCurrency).map((s) => s.amount_paid)),
    }),
    [all, active, baseCurrency],
  );
  const m = (v: string, c?: string) => formatMoney(v, c, i18n.language);

  return (
    <div className="page">
      <PageHeader title={t("sponsorships.title")}>
        {can("sponsorships.manage") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("sponsorships.new")}
          </button>
        )}
      </PageHeader>

      <div className="stats-grid">
        <StatCard tint="assets" icon={<HeartHandshake size={20} />} value={String(active.length)} label={t("sponsorships.active")} />
        <StatCard tint="income" icon={<CalendarClock size={20} />} value={m(stats.monthly, baseCurrency)} label={t("sponsorships.monthlyCommitment")} />
        <StatCard tint="liabilities" icon={<AlertTriangle size={20} />} value={String(stats.arrears)} label={t("sponsorships.inArrears")} />
        <StatCard tint="equity" icon={<HandCoins size={20} />} value={m(stats.collected, baseCurrency)} label={t("sponsorships.collected")} />
      </div>

      <Tabs
        active={filter}
        onChange={setFilter}
        tabs={(["active", "arrears", "paused", "ended", "all"] as Filter[]).map((k) => ({ key: k, label: t(`sponsorships.filter.${k}`) }))}
      />
      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={rows}
        rowKey={(s) => s.id}
        exportName="sponsorships"
        onRowClick={(s) => setOpenId(s.id)}
        empty={t("sponsorships.empty")}
        columns={[
          { header: "#", cell: (s) => String(s.sponsorship_no), num: true },
          { header: t("sponsorships.sponsor"), cell: (s) => s.donor_name },
          { header: t("sponsorships.beneficiary"), cell: (s) => s.beneficiary_name ?? "—" },
          { header: t("sponsorships.program"), cell: (s) => t(`programs.${s.program}`) },
          { header: t("sponsorships.monthly"), cell: (s) => m(s.monthly_amount, s.currency), text: (s) => s.monthly_amount, num: true },
          { header: t("sponsorships.paidDue"), cell: (s) => t("sponsorships.paidOfDue", { paid: s.months_paid, due: s.months_due }), num: true },
          {
            header: t("sponsorships.arrears"),
            text: (s) => String(s.arrears_months),
            cell: (s) => (s.arrears_months > 0 ? <Badge tone="danger">{t("sponsorships.monthsLate", { count: s.arrears_months })}</Badge> : <Badge tone="success">{t("sponsorships.upToDate")}</Badge>),
          },
          { header: t("common.status"), text: (s) => s.status, cell: (s) => <Badge tone={STATUS_TONE[s.status]}>{t(`sponsorships.status.${s.status}`)}</Badge> },
        ]}
      />

      {creating && <SponsorshipForm onClose={() => setCreating(false)} onSaved={(id) => (reload(), setOpenId(id))} />}
      {openId && <SponsorshipModal id={openId} onClose={() => setOpenId(null)} onChanged={reload} />}
    </div>
  );
}

function SponsorshipForm({ onClose, onSaved }: { onClose: () => void; onSaved: (id: string) => void }) {
  const { t, i18n } = useTranslation();
  const { funds, baseCurrency } = useRefs();
  const [donor, setDonor] = useState<Donor | null>(null);
  const { data: beneficiaries } = useLoad(() => api.get<Beneficiary[]>("/beneficiaries", { limit: 500 }).catch(() => [] as Beneficiary[]));
  const [form, setForm] = useState<Omit<NewSponsorship, "donor_id">>({
    beneficiary_id: null,
    program: "orphan",
    kind: "sadaqah",
    fund_id: null,
    monthly_amount: "",
    currency: baseCurrency,
    start_date: today(),
    end_date: null,
    notes: null,
  });
  const { busy, run } = useSubmit();
  const fundOptions = funds.filter((f) => !f.donation_kind || f.donation_kind === form.kind);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!donor) return;
    run(async () => {
      const s = await api.post<Sponsorship>("/sponsorships", { ...form, donor_id: donor.id });
      toastSuccess(t("sponsorships.created", { no: s.sponsorship_no }));
      onSaved(s.id);
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} size="lg" title={t("sponsorships.new")}>
      <form onSubmit={submit}>
        <Field label={t("sponsorships.sponsor")}>
          <DonorPicker value={donor} onChange={setDonor} />
        </Field>
        <div className="form-row">
          <Field label={t("sponsorships.beneficiary")} hint={t("sponsorships.beneficiaryHint")}>
            <select value={form.beneficiary_id ?? ""} onChange={(e) => setForm({ ...form, beneficiary_id: e.target.value || null })}>
              <option value="">{t("sponsorships.generalProgram")}</option>
              {(beneficiaries ?? []).map((b) => (
                <option key={b.id} value={b.id}>
                  #{b.file_no} · {b.full_name}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("sponsorships.program")}>
            <select value={form.program} onChange={(e) => setForm({ ...form, program: e.target.value as NewSponsorship["program"] })}>
              {SPONSORSHIP_PROGRAMS.map((p) => (
                <option key={p} value={p}>
                  {t(`programs.${p}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="form-row three">
          <Field label={t("sponsorships.monthly")}>
            <input required inputMode="decimal" placeholder="0.00" value={form.monthly_amount} onChange={(e) => setForm({ ...form, monthly_amount: e.target.value })} />
          </Field>
          <Field label={t("common.currency")}>
            <input required maxLength={3} pattern="[A-Z]{3}" dir="ltr" value={form.currency} onChange={(e) => setForm({ ...form, currency: e.target.value.toUpperCase() })} />
          </Field>
          <Field label={t("donations.kind")}>
            <select value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value as DonationKind, fund_id: null })}>
              {(["sadaqah", "zakat", "general"] as DonationKind[]).map((k) => (
                <option key={k} value={k}>
                  {t(`kinds.${k}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="form-row three">
          <Field label={t("sponsorships.start")}>
            <input type="date" required value={form.start_date} onChange={(e) => setForm({ ...form, start_date: e.target.value })} />
          </Field>
          <Field label={t("sponsorships.endDate")}>
            <input type="date" min={form.start_date} value={form.end_date ?? ""} onChange={(e) => setForm({ ...form, end_date: e.target.value || null })} />
          </Field>
          <Field label={t("journal.fund")}>
            <select value={form.fund_id ?? ""} onChange={(e) => setForm({ ...form, fund_id: e.target.value || null })}>
              <option value="">{t("donations.autoFund")}</option>
              {fundOptions.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.code} · {localName(f, i18n.language)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <Field label={t("common.notes")}>
          <textarea rows={2} value={form.notes ?? ""} onChange={(e) => setForm({ ...form, notes: e.target.value || null })} />
        </Field>
        <FormActions busy={busy || !donor || !isPositive(form.monthly_amount)} onClose={onClose} />
      </form>
    </Modal>
  );
}

function SponsorshipModal({ id, onClose, onChanged }: { id: string; onClose: () => void; onChanged: () => void }) {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const { data: sp, error, reload } = useLoad(() => api.get<SponsorshipDetail>(`/sponsorships/${id}`), [id]);
  const [pay, setPay] = useState({ months: 1, donation_date: today(), payment_method: "transfer", payment_ref: "" });
  const [requestKey, setRequestKey] = useState(newRequestKey);
  const [editing, setEditing] = useState(false);
  const { busy, run } = useSubmit();
  const m = (v: string, c?: string) => formatMoney(v, c, i18n.language);

  const submitPay = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const r = await api.post<RecordedDonation>(`/sponsorships/${id}/payments`, { ...pay, payment_ref: pay.payment_ref || null, note: null }, requestKey);
      toastSuccess(t("donations.recorded", { no: r.receipt_no }));
      setRequestKey(newRequestKey());
      reload();
      onChanged();
    });
  };

  const setStatus = (status: SponsorshipStatus) =>
    run(async () => {
      await api.post(`/sponsorships/${id}/status`, { status });
      reload();
      onChanged();
    });

  return (
    <Modal isOpen onClose={onClose} size="lg" title={sp ? `${t("sponsorships.one")} #${sp.sponsorship_no}` : t("common.loading")}>
      {error && <ErrorNote message={error} onRetry={reload} />}
      {!sp ? (
        <Loading />
      ) : (
        <>
          <div className="detail-grid">
            <div><span>{t("sponsorships.sponsor")}</span><strong>{sp.donor_name}</strong></div>
            <div><span>{t("sponsorships.beneficiary")}</span><strong>{sp.beneficiary_name ?? t("sponsorships.generalProgram")}</strong></div>
            <div><span>{t("sponsorships.program")}</span><strong>{t(`programs.${sp.program}`)}</strong></div>
            <div><span>{t("sponsorships.monthly")}</span><strong>{m(sp.monthly_amount, sp.currency)}</strong></div>
            <div><span>{t("sponsorships.start")}</span><strong>{sp.start_date}{sp.end_date ? ` → ${sp.end_date}` : ""}</strong></div>
            <div><span>{t("sponsorships.paidDue")}</span><strong>{t("sponsorships.paidOfDue", { paid: sp.months_paid, due: sp.months_due })}</strong></div>
            <div><span>{t("sponsorships.collected")}</span><strong>{m(sp.amount_paid, sp.currency)}</strong></div>
            <div>
              <span>{t("sponsorships.arrears")}</span>
              {sp.arrears_months > 0 ? <Badge tone="danger">{t("sponsorships.monthsLate", { count: sp.arrears_months })}</Badge> : <Badge tone="success">{t("sponsorships.upToDate")}</Badge>}
            </div>
          </div>

          {can("sponsorships.pay") && sp.status !== "ended" && (
            <form className="settings-section pay-box" onSubmit={submitPay}>
              <div className="settings-section-head">
                <h3>{t("sponsorships.recordPayment")}</h3>
              </div>
              <div className="form-row three">
                <Field label={t("sponsorships.months")} hint={m(String(Number(sp.monthly_amount) * pay.months), sp.currency)}>
                  <input type="number" min={1} max={60} required value={pay.months} onChange={(e) => setPay({ ...pay, months: Math.max(1, Math.min(60, Number(e.target.value) || 1)) })} />
                </Field>
                <Field label={t("common.date")}>
                  <input type="date" required max={today()} value={pay.donation_date} onChange={(e) => setPay({ ...pay, donation_date: e.target.value })} />
                </Field>
                <Field label={t("donations.method")}>
                  <select value={pay.payment_method} onChange={(e) => setPay({ ...pay, payment_method: e.target.value })}>
                    {METHODS.map((x) => (
                      <option key={x} value={x}>
                        {t(`methods.${x}`)}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>
              <Field label={t("donations.paymentRef")}>
                <input dir="ltr" value={pay.payment_ref} onChange={(e) => setPay({ ...pay, payment_ref: e.target.value })} />
              </Field>
              <div className="form-actions">
                <button type="submit" className="btn btn-primary" disabled={busy}>
                  <HandCoins size={16} /> {t("sponsorships.payAndReceipt")}
                </button>
              </div>
            </form>
          )}

          <h3 className="section-head">{t("sponsorships.payments")}</h3>
          <DataTable
            searchable={false}
            rows={sp.payments}
            rowKey={(p) => p.donation_id}
            empty={t("sponsorships.noPayments")}
            columns={[
              { header: t("donations.receiptNo"), cell: (p) => p.receipt_no },
              { header: t("common.date"), cell: (p) => p.donation_date },
              { header: t("sponsorships.months"), cell: (p) => String(p.months), num: true },
              { header: t("common.amount"), cell: (p) => m(p.amount, p.currency), text: (p) => p.amount, num: true },
              {
                header: "",
                text: () => "",
                cell: (p) => (
                  <button type="button" className="btn-icon btn-print" title={t("donations.printReceipt")} onClick={() => printReceipt(p.donation_id)}>
                    <Printer size={15} />
                  </button>
                ),
              },
            ]}
          />

          {can("sponsorships.manage") && (
            <div className="form-actions">
              {sp.status === "active" && (
                <button type="button" className="btn btn-secondary" disabled={busy} onClick={() => setStatus("paused")}>
                  <Pause size={16} /> {t("sponsorships.pause")}
                </button>
              )}
              {sp.status === "paused" && (
                <button type="button" className="btn btn-secondary" disabled={busy} onClick={() => setStatus("active")}>
                  <Play size={16} /> {t("sponsorships.resume")}
                </button>
              )}
              {sp.status !== "ended" && (
                <button type="button" className="btn btn-danger" disabled={busy} onClick={() => setStatus("ended")}>
                  <Square size={16} /> {t("sponsorships.endAction")}
                </button>
              )}
              <EditButton onClick={() => setEditing(true)} />
            </div>
          )}
          {editing && (
            <EditRecord
              title={t("sponsorships.edit")}
              path={`/sponsorships/${sp.id}`}
              initial={sp}
              note={t("sponsorships.editNote")}
              onClose={() => setEditing(false)}
              onSaved={() => (reload(), onChanged())}
              fields={[
                { key: "monthly_amount", label: t("sponsorships.monthly"), type: "number", step: "0.01", required: true, dir: "ltr" },
                { key: "end_date", label: t("sponsorships.endDate"), type: "date" },
                { key: "notes", label: t("common.notes"), type: "textarea" },
              ]}
            />
          )}
        </>
      )}
    </Modal>
  );
}
