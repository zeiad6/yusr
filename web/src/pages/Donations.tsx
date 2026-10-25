import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Link2, Plus, Printer, UserPlus, Layers, RefreshCw } from "lucide-react";
import { api, errorMessage, newRequestKey, openHtml } from "../api/client";
import { DONATION_KINDS, type DonationKind, type DonationRow, type Donor, type NewDonation, type RecordedDonation } from "../api/types";
import Modal from "../components/Modal";
import { toastError, toastSuccess } from "../components/Toast";
import { DataTable, ErrorNote, Field, PageHeader, useLoad, useSubmit, Badge } from "../components/ui";
import { FormActions } from "../components/forms";
import EditRecord, { EditButton } from "../components/EditRecord";
import { useAuthStore } from "../store/auth";
import { localName, useRefs } from "../store/refs";
import { formatMoney, isPositive, sum, today } from "../utils/money";
import { DonorForm } from "./Donors";

interface StagedQueueItem {
  id: string;
  gateway: string;
  external_id: string;
  amount: string;
  currency: string;
  donor_name: string | null;
  status: string;
  received_at: string;
  reconciled_at: string | null;
}

const METHODS = ["cash", "bank", "transfer", "cheque", "sadad", "card"];

export async function printReceipt(id: string, zakatCertificate = false) {
  try {
    openHtml(await api.text(`/donations/${id}/receipt`, { zakat_certificate: zakatCertificate || undefined }));
  } catch (e) {
    toastError(errorMessage(e));
  }
}

export default function Donations() {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [from, setFrom] = useState(`${new Date().getFullYear()}-01-01`);
  const [to, setTo] = useState(today());
  const [recording, setRecording] = useState(false);
  const [checkout, setCheckout] = useState(false);
  const [editing, setEditing] = useState<DonationRow | null>(null);
  const [showQueue, setShowQueue] = useState(false);
  const [queueItems, setQueueItems] = useState<StagedQueueItem[]>([]);
  const [reconciling, setReconciling] = useState(false);

  const loadQueue = async () => {
    try {
      setQueueItems(await api.get<StagedQueueItem[]>("/queue/transactions"));
    } catch {
      // offline
    }
  };

  const handleReconcile = async () => {
    setReconciling(true);
    try {
      const res = await api.post<{ reconciled_count: number; message: string }>("/queue/reconcile", {});
      toastSuccess(res.message);
      loadQueue();
      reload();
    } catch (e) {
      toastError(errorMessage(e));
    } finally {
      setReconciling(false);
    }
  };
  const { data, error, reload } = useLoad(() => api.get<DonationRow[]>("/donations", { from, to, limit: 500 }), [from, to]);
  const rows = data ?? [];

  return (
    <div className="page">
      <PageHeader title={t("donations.title")}>
        <Field label={t("common.from")}>
          <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        </Field>
        <Field label={t("common.to")}>
          <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        </Field>
        {can("payments.checkout") && (
          <button type="button" className="btn btn-secondary" onClick={() => setCheckout(true)}>
            <Link2 size={16} /> {t("donations.paymentLink")}
          </button>
        )}
        <button type="button" className="btn btn-secondary" onClick={() => { setShowQueue(true); loadQueue(); }}>
          <Layers size={16} /> طابور بوابات الدفع
        </button>
        {can("donations.record") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setRecording(true)}>
            <Plus size={16} /> {t("donations.record")}
          </button>
        )}
      </PageHeader>

      <div className="totals-bar">
        <span>{t("common.rows", { count: rows.length })}</span>
        <span>
          {t("common.total")}: <strong>{formatMoney(sum(rows.map((r) => r.base_amount)), baseCurrency, i18n.language)}</strong>
        </span>
      </div>

      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={rows}
        rowKey={(r) => r.id}
        exportName={`donations-${from}-${to}`}
        columns={[
          { header: t("donations.receiptNo"), cell: (r) => r.receipt_no },
          { header: t("common.date"), cell: (r) => r.donation_date },
          { header: t("donations.donor"), cell: (r) => r.donor_name },
          { header: t("donations.kind"), cell: (r) => t(`kinds.${r.kind}`) },
          { header: t("journal.fund"), cell: (r) => r.fund_code },
          { header: t("donations.method"), cell: (r) => t(`methods.${r.payment_method}`, { defaultValue: r.payment_method }) },
          { header: t("common.amount"), cell: (r) => formatMoney(r.amount, r.currency, i18n.language), text: (r) => r.amount, num: true },
          {
            header: t("donations.receipt"),
            text: () => "",
            cell: (r) => (
              <div className="action-btns">
                <button type="button" className="btn-icon btn-print" title={t("donations.receipt")} onClick={() => printReceipt(r.id)}>
                  <Printer size={15} />
                </button>
                {r.kind === "zakat" && (
                  <button type="button" className="btn btn-sm btn-outline" onClick={() => printReceipt(r.id, true)}>
                    {t("donations.zakatCertificate")}
                  </button>
                )}
                {can("donations.edit") && <EditButton onClick={() => setEditing(r)} />}
              </div>
            ),
          },
        ]}
      />

      {recording && <DonationForm onClose={() => setRecording(false)} onSaved={reload} />}
      {checkout && <CheckoutForm onClose={() => setCheckout(false)} />}
      {editing && (
        <EditRecord
          title={t("donations.edit", { no: editing.receipt_no })}
          path={`/donations/${editing.id}`}
          method="patch"
          initial={editing}
          note={t("donations.editNote")}
          onClose={() => setEditing(null)}
          onSaved={reload}
          fields={[
            { key: "payment_ref", label: t("donations.paymentRef"), dir: "ltr" },
            { key: "note", label: t("common.notes"), type: "textarea" },
          ]}
        />
      )}

      {showQueue && (
        <Modal isOpen={showQueue} title="طابور بوابات الدفع والتحصيلات الإلكترونية (Staging Queue)" onClose={() => setShowQueue(false)}>
          <div style={{ marginBottom: "16px", display: "flex", justifyContent: "space-between", alignItems: "center", gap: "10px" }}>
            <p className="settings-note" style={{ margin: 0 }}>
              يحتفظ النظام بالحركات الرقمية المستلمة في طابور محلي مستقل لحين التأكيد والمطابقة مع الدفاتر.
            </p>
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={handleReconcile}
              disabled={reconciling || queueItems.filter((q) => q.status === "staged").length === 0}
            >
              <RefreshCw size={14} className={reconciling ? "spin" : ""} />
              {reconciling ? "جارٍ المطابقة…" : "مطابقة وترحيل الدفعات (Reconcile Now)"}
            </button>
          </div>

          <div style={{ maxHeight: "350px", overflowY: "auto" }}>
            <table className="data-table" style={{ width: "100%" }}>
              <thead>
                <tr>
                  <th>البوابة</th>
                  <th>المعرف الخارجي</th>
                  <th>المتبرع</th>
                  <th>المبلغ</th>
                  <th>الحالة</th>
                  <th>وقت الاستلام</th>
                </tr>
              </thead>
              <tbody>
                {queueItems.length === 0 ? (
                  <tr>
                    <td colSpan={6} style={{ textAlign: "center", padding: "20px" }}>
                      لا توجد حركات معلقة في الطابور حالياً
                    </td>
                  </tr>
                ) : (
                  queueItems.map((item) => (
                    <tr key={item.id}>
                      <td><Badge tone="secondary">{item.gateway.toUpperCase()}</Badge></td>
                      <td style={{ fontFamily: "monospace" }}>{item.external_id}</td>
                      <td>{item.donor_name || "—"}</td>
                      <td className="num"><strong>{formatMoney(item.amount, item.currency, i18n.language)}</strong></td>
                      <td>
                        <Badge tone={item.status === "reconciled" ? "success" : item.status === "staged" ? "warning" : "secondary"}>
                          {item.status === "reconciled" ? "مرحل للدفاتر" : item.status === "staged" ? "معلق بالطابور" : item.status}
                        </Badge>
                      </td>
                      <td style={{ fontSize: "12px" }}>{new Date(item.received_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </Modal>
      )}
    </div>
  );
}

/** Server-side donor search with an inline "new donor" shortcut. */
export function DonorPicker({ value, onChange }: { value: Donor | null; onChange: (d: Donor | null) => void }) {
  const { t } = useTranslation();
  const [q, setQ] = useState("");
  const [results, setResults] = useState<Donor[]>([]);
  const [adding, setAdding] = useState(false);

  useEffect(() => {
    if (value || q.trim().length < 2) return setResults([]);
    const id = setTimeout(() => {
      api.get<Donor[]>("/donors", { q: q.trim(), limit: 10 }).then(setResults).catch(() => setResults([]));
    }, 300);
    return () => clearTimeout(id);
  }, [q, value]);

  if (value) {
    return (
      <div className="picked">
        <strong>{value.full_name}</strong>
        {value.national_id && <span className="settings-hint">{value.national_id}</span>}
        <button type="button" className="btn btn-sm btn-ghost" onClick={() => onChange(null)}>
          {t("common.change")}
        </button>
      </div>
    );
  }
  return (
    <div className="picker">
      <div className="picker-input">
        <input placeholder={t("donors.searchHint")} value={q} onChange={(e) => setQ(e.target.value)} />
        <button type="button" className="btn btn-sm btn-secondary" onClick={() => setAdding(true)} title={t("donors.new")}>
          <UserPlus size={15} />
        </button>
      </div>
      {results.length > 0 && (
        <ul className="picker-results" role="listbox">
          {results.map((d) => (
            <li key={d.id}>
              <button type="button" onClick={() => onChange(d)}>
                {d.full_name} <small>{d.national_id ?? d.phone ?? ""}</small>
              </button>
            </li>
          ))}
        </ul>
      )}
      {adding && (
        <DonorForm
          onClose={() => setAdding(false)}
          onSaved={(id) => api.get<Donor>(`/donors/${id}`).then(onChange).catch(() => undefined)}
        />
      )}
    </div>
  );
}

function DonationForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const { t, i18n } = useTranslation();
  const { funds, projects, baseCurrency } = useRefs();
  const [donor, setDonor] = useState<Donor | null>(null);
  const [form, setForm] = useState<Omit<NewDonation, "donor_id">>({
    donation_date: today(),
    kind: "sadaqah",
    donor_designated: false,
    fund_id: null,
    project_id: null,
    amount: "",
    currency: baseCurrency,
    payment_method: "cash",
    payment_ref: null,
    note: null,
  });
  const [done, setDone] = useState<RecordedDonation | null>(null);
  const [requestKey] = useState(newRequestKey);
  const { busy, run } = useSubmit();
  // A fund bound to a kind only takes that kind (enforced again by the server).
  const fundOptions = funds.filter((f) => !f.donation_kind || f.donation_kind === form.kind);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!donor) return;
    run(async () => {
      const r = await api.post<RecordedDonation>("/donations", { ...form, donor_id: donor.id }, requestKey);
      toastSuccess(t("donations.recorded", { no: r.receipt_no }));
      setDone(r);
      onSaved();
    });
  };

  if (done) {
    return (
      <Modal isOpen onClose={onClose} title={t("donations.recordedTitle")}>
        <div className="detail-grid">
          <div><span>{t("donations.receiptNo")}</span><strong>{done.receipt_no}</strong></div>
          <div><span>{t("journal.entry")}</span><strong>#{done.entry.entry_no} · {done.entry.hijri_date}</strong></div>
          <div><span>{t("journal.fund")}</span><strong>{localName(funds.find((f) => f.id === done.fund_id), i18n.language)}</strong></div>
          <div><span>{t("audit.hash")}</span><code className="hash">{done.entry.audit_hash.slice(0, 16)}…</code></div>
        </div>
        <div className="form-actions">
          <button type="button" className="btn btn-primary" onClick={() => printReceipt(done.donation_id)}>
            <Printer size={16} /> {t("donations.printReceipt")}
          </button>
          {done.kind === "zakat" && (
            <button type="button" className="btn btn-secondary" onClick={() => printReceipt(done.donation_id, true)}>
              {t("donations.zakatCertificate")}
            </button>
          )}
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            {t("common.close")}
          </button>
        </div>
      </Modal>
    );
  }

  return (
    <Modal isOpen onClose={onClose} size="lg" title={t("donations.record")}>
      <form onSubmit={submit}>
        <Field label={t("donations.donor")}>
          <DonorPicker value={donor} onChange={setDonor} />
        </Field>
        <div className="form-row three">
          <Field label={t("common.date")}>
            <input type="date" required value={form.donation_date} max={today()} onChange={(e) => setForm({ ...form, donation_date: e.target.value })} />
          </Field>
          <Field label={t("donations.kind")}>
            <select value={form.kind ?? ""} onChange={(e) => setForm({ ...form, kind: (e.target.value || null) as DonationKind | null, fund_id: null })}>
              {DONATION_KINDS.map((k) => (
                <option key={k} value={k}>
                  {t(`kinds.${k}`)}
                </option>
              ))}
              <option value="">{t("donations.kindFromNote")}</option>
            </select>
          </Field>
          <Field label={t("donations.method")}>
            <select value={form.payment_method} onChange={(e) => setForm({ ...form, payment_method: e.target.value })}>
              {METHODS.map((m) => (
                <option key={m} value={m}>
                  {t(`methods.${m}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="form-row three">
          <Field label={t("common.amount")}>
            <input required inputMode="decimal" placeholder="0.00" value={form.amount} onChange={(e) => setForm({ ...form, amount: e.target.value })} />
          </Field>
          <Field label={t("common.currency")}>
            <input required maxLength={3} pattern="[A-Z]{3}" title="ISO 4217 (SAR, USD…)" dir="ltr" value={form.currency} onChange={(e) => setForm({ ...form, currency: e.target.value.toUpperCase() })} />
          </Field>
          <Field label={t("donations.paymentRef")}>
            <input dir="ltr" value={form.payment_ref ?? ""} onChange={(e) => setForm({ ...form, payment_ref: e.target.value || null })} />
          </Field>
        </div>
        <div className="form-row">
          <Field label={t("journal.fund")} hint={t("donations.fundHint")}>
            <select value={form.fund_id ?? ""} onChange={(e) => setForm({ ...form, fund_id: e.target.value || null, project_id: null })}>
              <option value="">{t("donations.autoFund")}</option>
              {fundOptions.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.code} · {localName(f, i18n.language)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("journal.project")}>
            <select value={form.project_id ?? ""} onChange={(e) => setForm({ ...form, project_id: e.target.value || null })}>
              <option value="">{t("journal.noProject")}</option>
              {projects.filter((p) => !form.fund_id || p.fund_id === form.fund_id).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.code} · {localName(p, i18n.language)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        {form.kind === "sadaqah" && (
          <label className="checkbox-label">
            <input type="checkbox" checked={form.donor_designated} onChange={(e) => setForm({ ...form, donor_designated: e.target.checked })} />
            {t("donations.designated")}
          </label>
        )}
        <Field label={t("common.notes")} hint={form.kind === null ? t("donations.noteHint") : undefined}>
          <textarea rows={2} required={form.kind === null} value={form.note ?? ""} onChange={(e) => setForm({ ...form, note: e.target.value || null })} />
        </Field>
        <FormActions busy={busy || !donor || !isPositive(form.amount)} onClose={onClose} label={t("donations.recordAndIssue")} />
      </form>
    </Modal>
  );
}

interface CheckoutResult {
  intent: { id: string };
  checkout: { gateway_ref: string; redirect_url: string | null };
}

function CheckoutForm({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const { data: gateways, error } = useLoad(() => api.get<string[]>("/payments/gateways"));
  const [donor, setDonor] = useState<Donor | null>(null);
  const [form, setForm] = useState({ gateway: "", kind: "sadaqah" as DonationKind, amount: "", currency: baseCurrency, method: "any" });
  const [result, setResult] = useState<CheckoutResult | null>(null);
  const [requestKey] = useState(newRequestKey);
  const { busy, run } = useSubmit();
  const gateway = form.gateway || gateways?.[0] || "";

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!donor) return;
    run(async () => {
      setResult(await api.post<CheckoutResult>("/payments/checkout", { ...form, gateway, donor_id: donor.id, fund_id: null }, requestKey));
    });
  };

  return (
    <Modal isOpen onClose={onClose} title={t("donations.paymentLink")}>
      {error && <ErrorNote message={error} />}
      {gateways && gateways.length === 0 && <div className="settings-warning">{t("donations.noGateways")}</div>}
      {result ? (
        <div className="settings-stack">
          <p>{t("donations.linkReady")}</p>
          {result.checkout.redirect_url ? (
            <div className="picker-input">
              <input readOnly dir="ltr" value={result.checkout.redirect_url} onFocus={(e) => e.target.select()} />
              <button type="button" className="btn btn-secondary btn-sm" onClick={() =>
                  navigator.clipboard
                    ?.writeText(result.checkout.redirect_url ?? "")
                    .then(() => toastSuccess(t("common.copied")))
                    .catch(() => toastError(t("common.copyFailed")))
                }>
                {t("common.copy")}
              </button>
            </div>
          ) : (
            <code className="hash">{result.checkout.gateway_ref}</code>
          )}
          <small className="settings-hint">{t("donations.webhookNote")}</small>
        </div>
      ) : (
        gateways &&
        gateways.length > 0 && (
          <form onSubmit={submit}>
            <Field label={t("donations.donor")}>
              <DonorPicker value={donor} onChange={setDonor} />
            </Field>
            <div className="form-row">
              <Field label={t("donations.gateway")}>
                <select value={gateway} onChange={(e) => setForm({ ...form, gateway: e.target.value })}>
                  {gateways.map((g) => (
                    <option key={g} value={g}>
                      {g}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label={t("donations.kind")}>
                <select value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value as DonationKind })}>
                  {DONATION_KINDS.map((k) => (
                    <option key={k} value={k}>
                      {t(`kinds.${k}`)}
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
              <Field label={t("donations.method")}>
                <select value={form.method} onChange={(e) => setForm({ ...form, method: e.target.value })}>
                  {["any", "card", "mada", "apple_pay", "knet", "benefit", "fawry_reference"].map((m) => (
                    <option key={m} value={m}>
                      {t(`gatewayMethods.${m}`)}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
            <FormActions busy={busy || !donor || !isPositive(form.amount)} onClose={onClose} label={t("donations.createLink")} />
          </form>
        )
      )}
    </Modal>
  );
}
