import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Calculator, Coins, RefreshCw, Edit3, CheckCircle } from "lucide-react";
import { api, errorMessage } from "../api/client";
import type { ZakatAssessment, ZakatInput } from "../api/types";
import { Badge, Field, PageHeader, useSubmit } from "../components/ui";
import { toastError, toastSuccess, toastInfo } from "../components/Toast";
import { useRefs } from "../store/refs";
import { formatMoney, today } from "../utils/money";

interface ZakatRatesData {
  current: {
    gold_gram_price: string;
    silver_gram_price: string;
    currency: string;
    gold_nisab_threshold: string;
    silver_nisab_threshold: string;
    source: string;
    updated_at: string | null;
    updated_by?: string;
  };
  history: Array<{
    id: string;
    gold_gram_price: string;
    silver_gram_price: string;
    source: string;
    updated_at: string;
  }>;
}

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
  const [ratesData, setRatesData] = useState<ZakatRatesData | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [editingRates, setEditingRates] = useState(false);
  const [manualGold, setManualGold] = useState("");
  const [manualSilver, setManualSilver] = useState("");

  const [input, setInput] = useState<ZakatInput>({
    cash_and_bank: "0",
    gold_grams: "0",
    silver_grams: "0",
    gold_price_per_gram: "295.50",
    silver_price_per_gram: "3.85",
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

  const loadRates = async () => {
    try {
      const data = await api.get<ZakatRatesData>("/zakat/rates");
      setRatesData(data);
      if (data.current) {
        setInput((prev) => ({
          ...prev,
          gold_price_per_gram: String(data.current.gold_gram_price),
          silver_price_per_gram: String(data.current.silver_gram_price),
        }));
        setManualGold(String(data.current.gold_gram_price));
        setManualSilver(String(data.current.silver_gram_price));
      }
    } catch {
      // offline / default
    }
  };

  useEffect(() => {
    loadRates();
  }, []);

  const handleSyncRates = async () => {
    setSyncing(true);
    try {
      const res = await api.post<{ synced: boolean; fallback?: boolean; message: string; gold_gram_price?: string; silver_gram_price?: string }>("/zakat/rates/sync", {});
      if (res.synced) {
        toastSuccess(res.message);
        loadRates();
      } else {
        toastInfo(res.message);
      }
    } catch (e) {
      toastError(errorMessage(e));
    } finally {
      setSyncing(false);
    }
  };

  const handleSaveManualRates = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.post("/zakat/rates", {
        gold_gram_price: manualGold,
        silver_gram_price: manualSilver,
        notes: "تعديل يدوي معتمد من المحاسب",
      });
      toastSuccess("تم حفظ أسعار النصاب الرسمية بنجاح");
      setEditingRates(false);
      loadRates();
    } catch (e) {
      toastError(errorMessage(e));
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => setResult(await api.post<ZakatAssessment>("/zakat/assess", input)));
  };

  return (
    <div className="page">
      <PageHeader title={t("zakat.title")} />
      <p className="settings-note">{t("zakat.intro")}</p>

      {/* Dynamic Zakat Nisab Configuration Panel */}
      <div className="chart-card" style={{ marginBottom: "20px" }}>
        <div className="card-head" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <Coins className="text-brass" size={20} />
            <h3 style={{ margin: 0 }}>إعدادات نصاب الزكاة وأسعار الذهب والفضة (Nisab Configuration)</h3>
          </div>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={() => setEditingRates(!editingRates)}
            >
              <Edit3 size={14} /> {editingRates ? "إلغاء التعديل" : "تعديل يدوي"}
            </button>
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={handleSyncRates}
              disabled={syncing}
            >
              <RefreshCw size={14} className={syncing ? "spin" : ""} />
              {syncing ? "جارٍ المزامنة…" : "مزامنة الأسعار عبر الإنترنت"}
            </button>
          </div>
        </div>

        {editingRates ? (
          <form onSubmit={handleSaveManualRates} style={{ marginTop: "14px", display: "flex", gap: "12px", alignItems: "flex-end", flexWrap: "wrap" }}>
            <Field label="سعر جرام الذهب عيار 24">
              <input
                type="number"
                step="0.01"
                required
                value={manualGold}
                onChange={(e) => setManualGold(e.target.value)}
                style={{ width: "160px" }}
              />
            </Field>
            <Field label="سعر جرام الفضة">
              <input
                type="number"
                step="0.01"
                required
                value={manualSilver}
                onChange={(e) => setManualSilver(e.target.value)}
                style={{ width: "160px" }}
              />
            </Field>
            <button type="submit" className="btn btn-primary btn-sm" style={{ height: "36px" }}>
              <CheckCircle size={14} /> حفظ واعتماد
            </button>
          </form>
        ) : (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))", gap: "16px", marginTop: "12px" }}>
            <div style={{ background: "var(--surface-sunken)", padding: "12px", borderRadius: "8px" }}>
              <span style={{ fontSize: "12px", color: "var(--text-tertiary)" }}>سعر جرام الذهب (24 قيراط)</span>
              <div style={{ fontSize: "18px", fontWeight: "bold", marginTop: "4px" }}>
                {ratesData?.current ? m(ratesData.current.gold_gram_price) : m(input.gold_price_per_gram)}
              </div>
              <small style={{ color: "var(--text-secondary)" }}>
                نصاب الذهب (85 جرام): {ratesData?.current ? m(ratesData.current.gold_nisab_threshold) : m(String(Number(input.gold_price_per_gram) * 85))}
              </small>
            </div>

            <div style={{ background: "var(--surface-sunken)", padding: "12px", borderRadius: "8px" }}>
              <span style={{ fontSize: "12px", color: "var(--text-tertiary)" }}>سعر جرام الفضة النقي</span>
              <div style={{ fontSize: "18px", fontWeight: "bold", marginTop: "4px" }}>
                {ratesData?.current ? m(ratesData.current.silver_gram_price) : m(input.silver_price_per_gram)}
              </div>
              <small style={{ color: "var(--text-secondary)" }}>
                نصاب الفضة (595 جرام): {ratesData?.current ? m(ratesData.current.silver_nisab_threshold) : m(String(Number(input.silver_price_per_gram) * 595))}
              </small>
            </div>

            <div style={{ background: "var(--surface-sunken)", padding: "12px", borderRadius: "8px" }}>
              <span style={{ fontSize: "12px", color: "var(--text-tertiary)" }}>مصدر التسعير وحالة التحديث</span>
              <div style={{ marginTop: "6px" }}>
                <Badge tone={ratesData?.current?.source === "api_sync" ? "success" : "secondary"}>
                  {ratesData?.current?.source === "api_sync" ? "مزامنة حية (Live API)" : "تسعير رسمي محلي (Offline/Manual)"}
                </Badge>
              </div>
              <small style={{ display: "block", marginTop: "6px", color: "var(--text-tertiary)" }}>
                {ratesData?.current?.updated_at ? new Date(ratesData.current.updated_at).toLocaleDateString() : "مثبت افتراضياً"}
              </small>
            </div>
          </div>
        )}
      </div>

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
