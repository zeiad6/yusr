import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, HandCoins, HandHeart, Wallet } from "lucide-react";
import { Bar, BarChart, CartesianGrid, Cell, Legend, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { api } from "../api/client";
import type { Dashboard as DashboardData } from "../api/types";
import { Badge, DataTable, ErrorNote, Field, Loading, PageHeader, StatCard, useLoad } from "../components/ui";
import { formatMoney, sum, today, toUnits } from "../utils/money";
import { localName } from "../store/refs";

// Categorical palette from the Yusr identity (teal, brass, …); fixed order keeps a kind's colour stable.
const KIND_COLORS = ["#0f766e", "#a98336", "#4f46e5", "#be185d", "#64748b"];

export default function Dashboard() {
  const { t, i18n } = useTranslation();
  const [asOf, setAsOf] = useState(today());
  const { data, error, loading, reload } = useLoad(() => api.get<DashboardData>("/reports/dashboard", { as_of: asOf }), [asOf]);

  if (loading && !data) return <Loading />;
  if (error && !data) return <ErrorNote message={error} onRetry={reload} />;
  if (!data) return null;

  const cur = data.base_currency;
  const money = (v: string) => formatMoney(v, cur, i18n.language);
  const donationsTotal = sum(data.donations_this_month.map((d) => d.total));
  const cashflow = data.cashflow.map((m) => ({ month: m.month, inflow: Number(m.inflow), outflow: Number(m.outflow) }));
  const kinds = data.donations_this_month.map((d) => ({ name: t(`kinds.${d.kind}`), value: Number(d.total) }));

  return (
    <div className="page dashboard">
      <PageHeader title={t("dashboard.title")}>
        <span className="page-subtitle">
          {t("dashboard.hijri")}: {data.hijri_as_of}
        </span>
        <Field label={t("dashboard.asOf")}>
          <input type="date" value={asOf} max={today()} onChange={(e) => e.target.value && setAsOf(e.target.value)} />
        </Field>
      </PageHeader>

      <div className="stats-grid">
        <StatCard tint="assets" icon={<Wallet size={20} />} value={money(data.cash_position)} label={t("dashboard.cash")} />
        <StatCard tint="income" icon={<HandCoins size={20} />} value={money(donationsTotal)} label={t("dashboard.donationsMonth")} />
        <StatCard tint="liabilities" icon={<HandHeart size={20} />} value={money(data.aid_this_month)} label={t("dashboard.aidMonth")} />
        <StatCard
          tint="equity"
          icon={<AlertTriangle size={20} />}
          value={`${data.active_cases} / ${data.critical_cases}`}
          label={t("dashboard.cases")}
        />
      </div>

      <div className="charts-grid">
        <div className="chart-card">
          <div className="card-head">
            <h3>{t("dashboard.cashflow")}</h3>
          </div>
          <ResponsiveContainer width="100%" height={260}>
            <BarChart data={cashflow}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
              <XAxis dataKey="month" tick={{ fontSize: 12 }} reversed={i18n.dir() === "rtl"} />
              <YAxis tick={{ fontSize: 12 }} orientation={i18n.dir() === "rtl" ? "right" : "left"} width={70} />
              <Tooltip formatter={(v: number) => money(String(v))} />
              <Legend />
              <Bar dataKey="inflow" name={t("dashboard.inflow")} fill="#0f766e" radius={[4, 4, 0, 0]} />
              <Bar dataKey="outflow" name={t("dashboard.outflow")} fill="#c2410c" radius={[4, 4, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>

        <div className="chart-card">
          <div className="card-head">
            <h3>{t("dashboard.byKind")}</h3>
          </div>
          {kinds.length === 0 ? (
            <div className="no-data">{t("common.noData")}</div>
          ) : (
            <ResponsiveContainer width="100%" height={260}>
              <PieChart>
                <Pie data={kinds} dataKey="value" nameKey="name" innerRadius={55} outerRadius={95} paddingAngle={2}>
                  {kinds.map((k, i) => (
                    <Cell key={k.name} fill={KIND_COLORS[i % KIND_COLORS.length]} />
                  ))}
                </Pie>
                <Tooltip formatter={(v: number) => money(String(v))} />
                <Legend />
              </PieChart>
            </ResponsiveContainer>
          )}
        </div>
      </div>

      <div className="card-head section-head">
        <h3>{t("dashboard.funds")}</h3>
      </div>
      <DataTable
        searchable={false}
        rows={data.funds}
        rowKey={(f) => f.fund_id}
        columns={[
          { header: t("common.code"), cell: (f) => f.code },
          { header: t("common.name"), cell: (f) => localName(f, i18n.language) },
          { header: t("funds.restriction"), cell: (f) => t(`restrictions.${f.restriction}`) },
          { header: t("funds.revenue"), cell: (f) => money(f.revenue), num: true },
          { header: t("funds.expenses"), cell: (f) => money(f.expenses), num: true },
          { header: t("funds.netAssets"), cell: (f) => money(f.net_assets), num: true },
        ]}
      />

      {data.budgets.length > 0 && (
        <>
          <div className="card-head section-head">
            <h3>{t("dashboard.budgets")}</h3>
          </div>
          <div className="budget-grid">
            {data.budgets.map((b) => {
              const approved = toUnits(b.approved) ?? 0n;
              const used = (toUnits(b.spent) ?? 0n) + (toUnits(b.committed) ?? 0n);
              const pct = approved > 0n ? Number((used * 1000n) / approved) / 10 : 0;
              const tone = pct >= 100 ? "danger" : pct >= 80 ? "warning" : "success";
              return (
                <div className="chart-card project-budget-gauge" key={b.id}>
                  <div className="gauge-header">
                    <strong>
                      {b.fund_code}
                      {b.project_code ? ` · ${b.project_code}` : ""}
                    </strong>
                    <Badge tone={tone}>{pct.toFixed(1)}%</Badge>
                  </div>
                  <div className="gauge-track">
                    <div className={`gauge-fill ${tone === "success" ? "safe" : tone}`} style={{ width: `${Math.min(pct, 100)}%` }} />
                  </div>
                  <small className="settings-hint">
                    {money(b.spent)} + {money(b.committed)} / {money(b.approved)}
                  </small>
                </div>
              );
            })}
          </div>
        </>
      )}

      {data.functional_expenses_ytd.length > 0 && (
        <>
          <div className="card-head section-head">
            <h3>{t("dashboard.functional")}</h3>
          </div>
          <DataTable
            searchable={false}
            rows={data.functional_expenses_ytd}
            rowKey={(r) => r.class}
            columns={[
              { header: t("accounts.functionalClass"), cell: (r) => t(`functional.${r.class}`, { defaultValue: r.class }) },
              { header: t("common.total"), cell: (r) => money(r.total), num: true },
            ]}
          />
        </>
      )}
    </div>
  );
}
