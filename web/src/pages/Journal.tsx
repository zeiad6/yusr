import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Plus, RotateCcw, Trash2 } from "lucide-react";
import { api, newRequestKey } from "../api/client";
import type { EntryDraft, EntryView, LineDraft, PostedEntry, Side } from "../api/types";
import Modal from "../components/Modal";
import { toastInfo, toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, useLoad, useSubmit } from "../components/ui";
import { useAuthStore } from "../store/auth";
import { localName, useRefs } from "../store/refs";
import { formatMoney, sum, today, toUnits } from "../utils/money";
import { actorName } from "../utils/actor";

function yearStart() {
  return `${new Date().getFullYear()}-01-01`;
}

export default function Journal() {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const [from, setFrom] = useState(yearStart());
  const [to, setTo] = useState(today());
  const [viewing, setViewing] = useState<EntryView | null>(null);
  const [creating, setCreating] = useState<EntryDraft | true | false>(false);
  const { data, error, reload } = useLoad(() => api.get<EntryView[]>("/journal", { from, to, limit: 500 }), [from, to]);

  const debitTotal = (e: EntryView) => sum(e.lines.filter((l) => l.side === "debit").map((l) => l.base_amount));

  return (
    <div className="page">
      <PageHeader title={t("journal.title")}>
        <Field label={t("common.from")}>
          <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        </Field>
        <Field label={t("common.to")}>
          <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        </Field>
        {can("ledger.post") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("journal.new")}
          </button>
        )}
      </PageHeader>

      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={data ?? []}
        rowKey={(e) => e.id}
        exportName="journal"
        onRowClick={setViewing}
        columns={[
          { header: t("journal.no"), cell: (e) => String(e.entry_no) },
          { header: t("common.date"), cell: (e) => e.entry_date },
          { header: t("common.hijri"), cell: (e) => e.hijri_date },
          { header: t("common.description"), cell: (e) => e.description },
          { header: t("journal.reference"), cell: (e) => e.reference ?? "" },
          { header: t("journal.source"), cell: (e) => t(`sources.${e.source}`, { defaultValue: e.source }) },
          {
            header: t("common.status"),
            text: (e) => e.status,
            cell: (e) => <Badge tone={e.status === "posted" ? "success" : "secondary"}>{t(`entryStatus.${e.status}`)}</Badge>,
          },
          { header: t("common.amount"), cell: (e) => formatMoney(debitTotal(e), undefined, i18n.language), text: debitTotal, num: true },
        ]}
      />

      {viewing && <EntryModal entry={viewing} onClose={() => setViewing(null)} onReversed={reload} onCorrect={setCreating} />}
      {creating && <EntryForm initial={creating === true ? undefined : creating} onClose={() => setCreating(false)} onPosted={reload} />}
    </div>
  );
}

/** The entry's lines as a new draft, for posting the corrected version after a reversal. */
function asDraft(entry: EntryView): EntryDraft {
  return {
    entry_date: today(),
    description: entry.description,
    reference: entry.reference,
    lines: entry.lines.map((l) => ({ account_id: l.account_id, fund_id: l.fund_id, project_id: l.project_id, side: l.side, amount: l.amount, currency: l.currency, memo: l.memo })),
  };
}

function EntryModal({
  entry,
  onClose,
  onReversed,
  onCorrect,
}: {
  entry: EntryView;
  onClose: () => void;
  onReversed: () => void;
  onCorrect: (draft: EntryDraft) => void;
}) {
  const { t, i18n } = useTranslation();
  const { accounts, funds, projects } = useRefs();
  const can = useAuthStore((s) => s.can);
  const [reason, setReason] = useState("");
  const { busy, run } = useSubmit();
  const canReverse = can("ledger.reverse") && entry.status === "posted" && !entry.reverses_id;

  const reverse = (correct: boolean) =>
    run(async () => {
      const r = await api.post<PostedEntry>(`/journal/${entry.id}/reverse`, { reason, date: today() });
      toastSuccess(t("journal.reversed", { no: r.entry_no }));
      onReversed();
      onClose();
      if (correct) onCorrect(asDraft(entry));
    });

  return (
    <Modal isOpen onClose={onClose} size="xl" title={`${t("journal.entry")} #${entry.entry_no}`}>
      <div className="detail-grid">
        <div><span>{t("common.date")}</span><strong>{entry.entry_date} · {entry.hijri_date}</strong></div>
        <div><span>{t("common.description")}</span><strong>{entry.description}</strong></div>
        <div><span>{t("journal.reference")}</span><strong>{entry.reference ?? "—"}</strong></div>
        <div><span>{t("journal.createdBy")}</span><strong>{actorName(entry.created_by)}</strong></div>
      </div>
      <div className="table-container">
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("journal.account")}</th>
              <th>{t("journal.fund")}</th>
              <th>{t("journal.project")}</th>
              <th className="num">{t("journal.debit")}</th>
              <th className="num">{t("journal.credit")}</th>
              <th className="num">{t("journal.fxRate")}</th>
              <th>{t("journal.memo")}</th>
            </tr>
          </thead>
          <tbody>
            {entry.lines.map((l, i) => {
              const a = accounts.find((x) => x.id === l.account_id);
              const amount = `${formatMoney(l.amount, l.currency, i18n.language)}`;
              return (
                <tr key={i}>
                  <td>{a ? `${a.code} ${localName(a, i18n.language)}` : l.account_id}</td>
                  <td>{funds.find((f) => f.id === l.fund_id)?.code ?? "—"}</td>
                  <td>{projects.find((p) => p.id === l.project_id)?.code ?? "—"}</td>
                  <td className="num">{l.side === "debit" ? amount : ""}</td>
                  <td className="num">{l.side === "credit" ? amount : ""}</td>
                  <td className="num">{l.fx_rate}</td>
                  <td>{l.memo ?? ""}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {canReverse && (
        <div className="form-row inline-action">
          <Field label={t("journal.reverseReason")}>
            <input value={reason} onChange={(e) => setReason(e.target.value)} />
          </Field>
          <button type="button" className="btn btn-danger" disabled={busy || !reason.trim()} onClick={() => reverse(false)}>
            <RotateCcw size={16} /> {t("journal.reverse")}
          </button>
          {can("ledger.post") && (
            <button type="button" className="btn btn-secondary" disabled={busy || !reason.trim()} onClick={() => reverse(true)} title={t("journal.correctHint")}>
              <Pencil size={16} /> {t("journal.correct")}
            </button>
          )}
        </div>
      )}
    </Modal>
  );
}

function blankLine(currency: string, side: Side, fundId: string): LineDraft {
  return { account_id: "", fund_id: fundId, project_id: null, side, amount: "", currency, memo: null };
}

function EntryForm({ initial, onClose, onPosted }: { initial?: EntryDraft; onClose: () => void; onPosted: () => void }) {
  const { t, i18n } = useTranslation();
  const { accounts, funds, projects, baseCurrency } = useRefs();
  const defaultFund = funds.find((f) => f.code === "GEN")?.id ?? funds[0]?.id ?? "";
  const [draft, setDraft] = useState<EntryDraft>(initial ?? {
    entry_date: today(),
    description: "",
    reference: null,
    lines: [blankLine(baseCurrency, "debit", defaultFund), blankLine(baseCurrency, "credit", defaultFund)],
  });
  const [requestKey] = useState(newRequestKey);
  const { busy, run } = useSubmit();
  const postable = accounts.filter((a) => a.is_postable && a.is_active);

  const setLine = (i: number, patch: Partial<LineDraft>) =>
    setDraft((d) => ({ ...d, lines: d.lines.map((l, j) => (j === i ? { ...l, ...patch } : l)) }));

  // Only same-currency entries can be checked here; the server converts and checks the rest.
  const singleCurrency = new Set(draft.lines.map((l) => l.currency)).size === 1;
  const dr = sum(draft.lines.filter((l) => l.side === "debit").map((l) => l.amount));
  const cr = sum(draft.lines.filter((l) => l.side === "credit").map((l) => l.amount));
  const fundsOff = singleCurrency && fundsUnbalanced(draft);
  const balanced = !singleCurrency || (toUnits(dr) === toUnits(cr) && !fundsOff);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const r = await api.post<PostedEntry>("/journal", { ...draft, reference: draft.reference || null }, requestKey);
      toastSuccess(t("journal.posted", { no: r.entry_no }));
      if (r.budget_warnings.length) toastInfo(t("journal.budgetWarning"));
      onPosted();
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} size="xl" title={t("journal.new")}>
      <form onSubmit={submit}>
        <div className="form-row three">
          <Field label={t("common.date")}>
            <input type="date" required value={draft.entry_date} onChange={(e) => setDraft({ ...draft, entry_date: e.target.value })} />
          </Field>
          <Field label={t("journal.reference")}>
            <input value={draft.reference ?? ""} onChange={(e) => setDraft({ ...draft, reference: e.target.value })} />
          </Field>
          <Field label={t("common.description")}>
            <input required value={draft.description} onChange={(e) => setDraft({ ...draft, description: e.target.value })} />
          </Field>
        </div>

        <div className="lines-section">
          {draft.lines.map((l, i) => (
            <div className="line-row" key={i}>
              <select required value={l.account_id} onChange={(e) => setLine(i, { account_id: e.target.value })} aria-label={t("journal.account")}>
                <option value="">{t("journal.account")}…</option>
                {postable.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.code} {localName(a, i18n.language)}
                  </option>
                ))}
              </select>
              <select required value={l.fund_id} onChange={(e) => setLine(i, { fund_id: e.target.value })} aria-label={t("journal.fund")}>
                {funds.map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.code}
                  </option>
                ))}
              </select>
              <select value={l.project_id ?? ""} onChange={(e) => setLine(i, { project_id: e.target.value || null })} aria-label={t("journal.project")}>
                <option value="">{t("journal.noProject")}</option>
                {projects.filter((p) => p.fund_id === l.fund_id).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.code}
                  </option>
                ))}
              </select>
              <select value={l.side} onChange={(e) => setLine(i, { side: e.target.value as Side })} aria-label={t("journal.side")}>
                <option value="debit">{t("journal.debit")}</option>
                <option value="credit">{t("journal.credit")}</option>
              </select>
              <input required inputMode="decimal" placeholder="0.00" value={l.amount} onChange={(e) => setLine(i, { amount: e.target.value })} aria-label={t("common.amount")} />
              <input required maxLength={3} pattern="[A-Z]{3}" title="ISO 4217 (SAR, USD…)" value={l.currency} onChange={(e) => setLine(i, { currency: e.target.value.toUpperCase() })} aria-label={t("common.currency")} className="currency-input" />
              <input placeholder={t("journal.memo")} value={l.memo ?? ""} onChange={(e) => setLine(i, { memo: e.target.value || null })} />
              <button
                type="button"
                className="btn-icon btn-delete"
                disabled={draft.lines.length <= 2}
                onClick={() => setDraft({ ...draft, lines: draft.lines.filter((_, j) => j !== i) })}
                aria-label={t("common.delete")}
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
          <button type="button" className="btn btn-secondary btn-sm" onClick={() => setDraft({ ...draft, lines: [...draft.lines, blankLine(baseCurrency, "debit", defaultFund)] })}>
            <Plus size={14} /> {t("journal.addLine")}
          </button>
        </div>

        <div className="totals-bar">
          <span>{t("journal.debit")}: <strong>{formatMoney(dr, undefined, i18n.language)}</strong></span>
          <span>{t("journal.credit")}: <strong>{formatMoney(cr, undefined, i18n.language)}</strong></span>
          {singleCurrency ? (
            <Badge tone={balanced ? "success" : "danger"}>{balanced ? t("journal.balanced") : t("journal.unbalanced")}</Badge>
          ) : (
            <Badge tone="info">{t("journal.multiCurrency")}</Badge>
          )}
        </div>

        {fundsOff && toUnits(dr) === toUnits(cr) && <ErrorNote message={t("journal.fundsMustBalance")} />}

        <div className="form-actions">
          <button type="submit" className="btn btn-primary" disabled={busy || !balanced}>
            {busy ? t("common.saving") : t("journal.post")}
          </button>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            {t("common.cancel")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** Each fund must balance on its own (crates/yusr-domain/src/ledger.rs); flag it before the server rejects it. */
function fundsUnbalanced(d: EntryDraft): boolean {
  const byFund = new Map<string, bigint>();
  for (const l of d.lines) {
    const u = toUnits(l.amount) ?? 0n;
    byFund.set(l.fund_id, (byFund.get(l.fund_id) ?? 0n) + (l.side === "debit" ? u : -u));
  }
  return [...byFund.values()].some((v) => v !== 0n);
}
