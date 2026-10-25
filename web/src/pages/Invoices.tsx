import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import QRCode from "qrcode";
import { FileCode, Plus, QrCode, Trash2 } from "lucide-react";
import { api, download, errorMessage, newRequestKey } from "../api/client";
import type { Address, InvoiceLine, InvoiceRequest, Organization, Party, StoredInvoice, VatCategory } from "../api/types";
import Modal from "../components/Modal";
import { toastError, toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, useLoad, useSubmit } from "../components/ui";
import { FormActions } from "../components/forms";
import { useRefs } from "../store/refs";
import { formatMoney, fromUnits, toUnits } from "../utils/money";

const VAT_CATEGORIES: VatCategory[] = ["S", "Z", "E", "O"];
const STANDARD_VAT = "15";

const emptyAddress = (): Address => ({ street: "", building_number: "", district: "", city: "", postal_zone: "", country: "SA" });
const emptyParty = (): Party => ({ registration_name: "", vat_number: null, other_id: null, address: emptyAddress() });
const emptyLine = (): InvoiceLine => ({ name: "", quantity: "1", unit_price: "", vat_category: "S", vat_percent: STANDARD_VAT, exemption: null });

function nowLocal() {
  const d = new Date();
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset());
  return d.toISOString().slice(0, 19);
}

async function downloadXml(inv: StoredInvoice) {
  try {
    download(`${inv.number}.xml`, await api.text(`/invoices/${inv.id}/xml`), "application/xml");
  } catch (e) {
    toastError(errorMessage(e));
  }
}

export default function Invoices() {
  const { t } = useTranslation();
  const [creating, setCreating] = useState(false);
  const [qr, setQr] = useState<StoredInvoice | null>(null);
  const { data, error, reload } = useLoad(() => api.get<StoredInvoice[]>("/invoices", { limit: 500 }));

  return (
    <div className="page">
      <PageHeader title={t("invoices.title")}>
        <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
          <Plus size={16} /> {t("invoices.new")}
        </button>
      </PageHeader>
      <p className="settings-note">{t("invoices.intro")}</p>
      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={data ?? []}
        rowKey={(i) => i.id}
        exportName="zatca-invoices"
        columns={[
          { header: t("invoices.number"), cell: (i) => i.number },
          { header: t("invoices.icv"), cell: (i) => String(i.icv), num: true },
          { header: t("invoices.type"), cell: (i) => t(`invoices.types.${i.invoice_type}`) },
          { header: t("common.status"), text: (i) => i.status, cell: (i) => <Badge tone={i.status === "rejected" ? "danger" : "success"}>{t(`invoices.status.${i.status}`)}</Badge> },
          { header: t("audit.hash"), text: (i) => i.invoice_hash, cell: (i) => <code className="hash">{i.invoice_hash.slice(0, 12)}…</code> },
          {
            header: "",
            text: () => "",
            cell: (i) => (
              <div className="action-btns">
                <button type="button" className="btn-icon btn-view" title="QR" onClick={() => setQr(i)}>
                  <QrCode size={15} />
                </button>
                <button type="button" className="btn-icon btn-export" title="XML" onClick={() => downloadXml(i)}>
                  <FileCode size={15} />
                </button>
              </div>
            ),
          },
        ]}
      />
      {creating && (
        <InvoiceForm
          onClose={() => setCreating(false)}
          onIssued={(inv) => {
            reload();
            setQr(inv);
          }}
        />
      )}
      {qr && <QrModal invoice={qr} onClose={() => setQr(null)} />}
    </div>
  );
}

function QrModal({ invoice, onClose }: { invoice: StoredInvoice; onClose: () => void }) {
  const { t } = useTranslation();
  const [src, setSrc] = useState("");
  useEffect(() => {
    // The stored QR is ZATCA's base64 TLV payload; the code image encodes that text.
    QRCode.toDataURL(invoice.qr, { margin: 1, width: 240 }).then(setSrc).catch(() => setSrc(""));
  }, [invoice.qr]);
  return (
    <Modal isOpen onClose={onClose} size="sm" title={`${t("invoices.qr")} · ${invoice.number}`}>
      <div className="qr-box">
        {src ? <img src={src} alt={t("invoices.qr")} width={240} height={240} /> : null}
        <small className="settings-hint">ICV {invoice.icv}</small>
        <button type="button" className="btn btn-secondary btn-sm" onClick={() => downloadXml(invoice)}>
          <FileCode size={14} /> XML
        </button>
      </div>
    </Modal>
  );
}

function AddressFields({ value, onChange }: { value: Address; onChange: (a: Address) => void }) {
  const { t } = useTranslation();
  const f = (k: keyof Address, required = true) => (
    <Field label={t(`invoices.address.${k}`)}>
      <input required={required} value={value[k]} onChange={(e) => onChange({ ...value, [k]: e.target.value })} />
    </Field>
  );
  return (
    <div className="form-row three">
      {f("street")}
      {f("building_number")}
      {f("district")}
      {f("city")}
      {f("postal_zone")}
      {f("country")}
    </div>
  );
}

function lineTotals(l: InvoiceLine) {
  const net = ((toUnits(l.quantity) ?? 0n) * (toUnits(l.unit_price) ?? 0n)) / 10000n;
  const vat = (net * (toUnits(l.vat_percent) ?? 0n)) / 1000000n;
  return { net, vat };
}

function InvoiceForm({ onClose, onIssued }: { onClose: () => void; onIssued: (inv: StoredInvoice) => void }) {
  const { t, i18n } = useTranslation();
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [req, setReq] = useState<InvoiceRequest>({
    number: null,
    issued_at: nowLocal(),
    invoice_type: "simplified",
    kind: "invoice",
    billing_reference: null,
    note_reason: null,
    seller: emptyParty(),
    buyer: null,
    currency: baseCurrency,
    payment_means: "10",
    lines: [emptyLine()],
  });
  const [requestKey] = useState(newRequestKey);
  const { busy, run } = useSubmit();

  useEffect(() => {
    api
      .get<Organization>("/settings/organization")
      .then((o) => setReq((r) => ({ ...r, seller: { ...r.seller, registration_name: o.name_ar || o.name_en, vat_number: o.vat_number } })))
      .catch(() => undefined);
  }, []);

  const setLine = (i: number, patch: Partial<InvoiceLine>) => setReq((r) => ({ ...r, lines: r.lines.map((l, j) => (j === i ? { ...l, ...patch } : l)) }));
  const totals = req.lines.map(lineTotals);
  const net = totals.reduce((a, x) => a + x.net, 0n);
  const vat = totals.reduce((a, x) => a + x.vat, 0n);
  const needsBuyer = req.invoice_type === "standard";
  const isNote = req.kind !== "invoice";

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const inv = await api.post<StoredInvoice>("/invoices", { ...req, issued_at: req.issued_at.length === 16 ? `${req.issued_at}:00` : req.issued_at }, requestKey);
      toastSuccess(t("invoices.issued", { no: inv.number }));
      onIssued(inv);
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} size="xl" title={t("invoices.new")}>
      <form onSubmit={submit}>
        <div className="form-row three">
          <Field label={t("invoices.type")}>
            <select
              value={req.invoice_type}
              onChange={(e) => {
                const invoice_type = e.target.value as InvoiceRequest["invoice_type"];
                setReq({ ...req, invoice_type, buyer: invoice_type === "standard" ? (req.buyer ?? emptyParty()) : req.buyer });
              }}
            >
              <option value="simplified">{t("invoices.types.simplified")}</option>
              <option value="standard">{t("invoices.types.standard")}</option>
            </select>
          </Field>
          <Field label={t("invoices.kind")}>
            <select value={req.kind} onChange={(e) => setReq({ ...req, kind: e.target.value as InvoiceRequest["kind"] })}>
              <option value="invoice">{t("invoices.kinds.invoice")}</option>
              <option value="credit_note">{t("invoices.kinds.credit_note")}</option>
              <option value="debit_note">{t("invoices.kinds.debit_note")}</option>
            </select>
          </Field>
          <Field label={t("invoices.issuedAt")}>
            <input type="datetime-local" step={1} required value={req.issued_at} onChange={(e) => setReq({ ...req, issued_at: e.target.value })} />
          </Field>
        </div>
        {isNote && (
          <div className="form-row">
            <Field label={t("invoices.billingReference")}>
              <input required value={req.billing_reference ?? ""} onChange={(e) => setReq({ ...req, billing_reference: e.target.value || null })} />
            </Field>
            <Field label={t("invoices.noteReason")}>
              <input required value={req.note_reason ?? ""} onChange={(e) => setReq({ ...req, note_reason: e.target.value || null })} />
            </Field>
          </div>
        )}

        <fieldset className="settings-section">
          <legend>{t("invoices.seller")}</legend>
          <div className="form-row">
            <Field label={t("invoices.registrationName")}>
              <input required value={req.seller.registration_name} onChange={(e) => setReq({ ...req, seller: { ...req.seller, registration_name: e.target.value } })} />
            </Field>
            <Field label={t("settings.vatNumber")}>
              <input required dir="ltr" value={req.seller.vat_number ?? ""} onChange={(e) => setReq({ ...req, seller: { ...req.seller, vat_number: e.target.value || null } })} />
            </Field>
          </div>
          <AddressFields value={req.seller.address} onChange={(address) => setReq({ ...req, seller: { ...req.seller, address } })} />
        </fieldset>

        {req.buyer ? (
          <fieldset className="settings-section">
            <legend>{t("invoices.buyer")}</legend>
            <div className="form-row">
              <Field label={t("invoices.registrationName")}>
                <input required value={req.buyer.registration_name} onChange={(e) => setReq({ ...req, buyer: { ...req.buyer!, registration_name: e.target.value } })} />
              </Field>
              <Field label={t("settings.vatNumber")}>
                <input required={needsBuyer} dir="ltr" value={req.buyer.vat_number ?? ""} onChange={(e) => setReq({ ...req, buyer: { ...req.buyer!, vat_number: e.target.value || null } })} />
              </Field>
            </div>
            <AddressFields value={req.buyer.address} onChange={(address) => setReq({ ...req, buyer: { ...req.buyer!, address } })} />
            {!needsBuyer && (
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => setReq({ ...req, buyer: null })}>
                {t("invoices.removeBuyer")}
              </button>
            )}
          </fieldset>
        ) : (
          <button type="button" className="btn btn-secondary btn-sm" onClick={() => setReq({ ...req, buyer: emptyParty() })}>
            <Plus size={14} /> {t("invoices.addBuyer")}
          </button>
        )}

        <div className="lines-section">
          {req.lines.map((l, i) => (
            <div className="line-row invoice-line" key={i}>
              <input required placeholder={t("invoices.item")} value={l.name} onChange={(e) => setLine(i, { name: e.target.value })} />
              <input required inputMode="decimal" placeholder={t("invoices.qty")} value={l.quantity} onChange={(e) => setLine(i, { quantity: e.target.value })} aria-label={t("invoices.qty")} />
              <input required inputMode="decimal" placeholder={t("invoices.unitPrice")} value={l.unit_price} onChange={(e) => setLine(i, { unit_price: e.target.value })} aria-label={t("invoices.unitPrice")} />
              <select
                value={l.vat_category}
                onChange={(e) => {
                  const c = e.target.value as VatCategory;
                  setLine(i, { vat_category: c, vat_percent: c === "S" ? STANDARD_VAT : "0", exemption: c === "S" ? null : (l.exemption ?? ["", ""]) });
                }}
                aria-label={t("invoices.vatCategory")}
              >
                {VAT_CATEGORIES.map((c) => (
                  <option key={c} value={c}>
                    {t(`invoices.vat.${c}`)}
                  </option>
                ))}
              </select>
              <input required inputMode="decimal" value={l.vat_percent} onChange={(e) => setLine(i, { vat_percent: e.target.value })} aria-label="VAT %" className="currency-input" />
              {l.vat_category !== "S" && (
                <>
                  <input required dir="ltr" placeholder={t("invoices.exemptionCode")} value={l.exemption?.[0] ?? ""} onChange={(e) => setLine(i, { exemption: [e.target.value, l.exemption?.[1] ?? ""] })} />
                  <input required placeholder={t("invoices.exemptionReason")} value={l.exemption?.[1] ?? ""} onChange={(e) => setLine(i, { exemption: [l.exemption?.[0] ?? "", e.target.value] })} />
                </>
              )}
              <button type="button" className="btn-icon btn-delete" disabled={req.lines.length <= 1} onClick={() => setReq({ ...req, lines: req.lines.filter((_, j) => j !== i) })} aria-label={t("common.delete")}>
                <Trash2 size={15} />
              </button>
            </div>
          ))}
          <button type="button" className="btn btn-secondary btn-sm" onClick={() => setReq({ ...req, lines: [...req.lines, emptyLine()] })}>
            <Plus size={14} /> {t("journal.addLine")}
          </button>
        </div>

        <div className="totals-bar">
          <span>{t("invoices.net")}: <strong>{formatMoney(fromUnits(net), req.currency, i18n.language)}</strong></span>
          <span>{t("invoices.vatTotal")}: <strong>{formatMoney(fromUnits(vat), req.currency, i18n.language)}</strong></span>
          <span>{t("invoices.gross")}: <strong>{formatMoney(fromUnits(net + vat), req.currency, i18n.language)}</strong></span>
        </div>
        <small className="settings-hint">{t("invoices.totalsHint")}</small>

        <FormActions busy={busy} onClose={onClose} label={t("invoices.signAndIssue")} />
      </form>
    </Modal>
  );
}
