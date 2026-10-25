import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Calculator } from "lucide-react";
import { api } from "../api/client";
import type { ZakatAssessment, ZakatInput } from "../api/types";
import { Badge, Field, PageHeader, useSubmit } from "../components/ui";
import { useRefs } from "../store/refs";
import { formatMoney, today } from "../utils/money";

const ASSET_FIELDS = [
  "cash_and_bank",
  "gold_grams",
  "silver_grams",
  "gold_price_per_gram",
  "silver_price_per_gram",
  "trade_goods_market_value",
  "collectible_receivables",
  "trading_investments",
  "zakatable_share_of_long_term_investments",
  "current_liabilities",
] as const;

export default function Zakat() {
  const { t, i18n } = useTranslation();
  const baseCurrency = useRefs((s) => s.baseCurrency);
  const [input, setInput] = useState<ZakatInput>({
    cash_and_bank: "0",
    gold_grams: "0",
    silver_grams: "0",
    gold_price_per_gram: "",
    silver_price_per_gram: "",
    trade_goods_market_value: "0",
    collectible_receivables: "0",
    trading_investments: "0",
    zakatable_share_of_long_term_investments: "0",
    current_liabilities: "0",
    nisab_basis: "gold",
    year_basis: "lunar",
    hawl_start: null,
    assessment_date: today(),
  });
  const [result, setResult] = useState<ZakatAssessment | null>(null);
  const { busy, run } = useSubmit();
  const m = (v: string) => formatMoney(v, baseCurrency, i18n.language);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => setResult(await api.post<ZakatAssessment>("/zakat/assess", input)));
  };

  return (
    <div className="page">
      <PageHeader title={t("zakat.title")} />
      <p className="settings-note">{t("zakat.intro")}</p>
      <div className="split">
        <form className="chart-card" onSubmit={submit}>
          <div className="form-row">
            {ASSET_FIELDS.map((k) => (
              <Field key={k} label={t(`zakat.${k}`)}>
                <input
                  inputMode="decimal"
                  required={k.endsWith("price_per_gram")}
                  value={input[k]}
                  onChange={(e) => setInput({ ...input, [k]: e.target.value })}
                />
              </Field>
            ))}
          </div>
          <div className="form-row">
            <Field label={t("zakat.nisab_basis")}>
              <select value={input.nisab_basis} onChange={(e) => setInput({ ...input, nisab_basis: e.target.value as ZakatInput["nisab_basis"] })}>
                <option value="gold">{t("zakat.gold")}</option>
                <option value="silver">{t("zakat.silver")}</option>
                <option value="lower_of_gold_and_silver">{t("zakat.lower")}</option>
              </select>
            </Field>
            <Field label={t("zakat.year_basis")}>
              <select value={input.year_basis} onChange={(e) => setInput({ ...input, year_basis: e.target.value as ZakatInput["year_basis"] })}>
                <option value="lunar">{t("zakat.lunar")}</option>
                <option value="solar">{t("zakat.solar")}</option>
              </select>
            </Field>
            <Field label={t("zakat.hawl_start")}>
              <input type="date" value={input.hawl_start ?? ""} onChange={(e) => setInput({ ...input, hawl_start: e.target.value || null })} />
            </Field>
            <Field label={t("zakat.assessment_date")}>
              <input type="date" required value={input.assessment_date} onChange={(e) => setInput({ ...input, assessment_date: e.target.value })} />
            </Field>
          </div>
          <div className="form-actions">
            <button type="submit" className="btn btn-primary" disabled={busy}>
              <Calculator size={16} /> {t("zakat.calculate")}
            </button>
          </div>
        </form>

        {result && (
          <div className="chart-card zakat-result">
            <div className="card-head">
              <h3>{t("zakat.result")}</h3>
              <Badge tone={result.exemption_reason ? "secondary" : "success"}>
                {result.exemption_reason ? t("zakat.exempt") : t("zakat.due")}
              </Badge>
            </div>
            <dl className="report-grid">
              <div className="report-item"><dt className="report-label">{t("zakat.gross")}</dt><dd className="report-value">{m(result.gross_zakatable_assets)}</dd></div>
              <div className="report-item"><dt className="report-label">{t("zakat.deductible")}</dt><dd className="report-value">{m(result.deductible_liabilities)}</dd></div>
              <div className="report-item"><dt className="report-label">{t("zakat.net")}</dt><dd className="report-value">{m(result.net_zakatable_assets)}</dd></div>
              <div className="report-item"><dt className="report-label">{t("zakat.nisab")}</dt><dd className="report-value">{m(result.nisab_value)}</dd></div>
              <div className="report-item"><dt className="report-label">{t("zakat.rate")}</dt><dd className="report-value">{(Number(result.rate) * 100).toFixed(4)}%</dd></div>
              <div className="report-item">
                <dt className="report-label">{t("zakat.hawl")}</dt>
                <dd className="report-value">
                  {result.hawl_complete ? t("common.yes") : t("common.no")}
                  {result.hawl_due_hijri && ` · ${result.hawl_due_hijri.year}/${result.hawl_due_hijri.month}/${result.hawl_due_hijri.day} هـ`}
                </dd>
              </div>
            </dl>
            <div className="zakat-due">
              <span>{t("zakat.zakatDue")}</span>
              <strong>{m(result.zakat_due)}</strong>
            </div>
            {result.exemption_reason && <p className="settings-hint">{t(`zakat.reasons.${result.exemption_reason}`, { defaultValue: result.exemption_reason })}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
