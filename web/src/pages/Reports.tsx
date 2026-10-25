import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  BookOpen,
  FileDown,
  FileSpreadsheet,
  FileText,
  HandCoins,
  HandHeart,
  Landmark,
  ListChecks,
  PieChart,
  Printer,
  Scale,
  Users,
  HeartHandshake,
  Clock,
  type LucideIcon,
} from "lucide-react";
import { api, errorMessage } from "../api/client";
import type {
  ActivityRow,
  AidSummary,
  Beneficiary,
  BudgetUsage,
  DonationSummary,
  EntryView,
  FundBalance,
  FundRestriction,
  GroupTotal,
  Organization,
  Sponsorship,
  TrialBalanceRow,
  VolunteerSummary,
} from "../api/types";
import { toastInfo } from "../components/Toast";
import PaperFrame from "../components/PaperFrame";
import { ErrorNote, Field, Loading, PageHeader } from "../components/ui";
import { useAuthStore } from "../store/auth";
import type { Permission } from "../store/permissions";
import { localName, useRefs } from "../store/refs";
import { gregorian, hijri } from "../utils/dates";
import { formatMoney, fromUnits, sum, today, toUnits } from "../utils/money";
import { downloadCsv, downloadWord, esc, paperHtml, printHtml, type CsvSection } from "../utils/paper";

type Key = "activities" | "funds" | "trial" | "donations" | "aid" | "budget" | "journal" | "beneficiaries" | "sponsorships" | "volunteers";
type T = (k: string, o?: Record<string, unknown>) => string;

interface Ctx {
  from: string;
  to: string;
  asOf: string;
  t: T;
  lang: string;
  currency: string;
}

interface Built {
  title: string;
  subtitle: string;
  body: string;
  csv: CsvSection[];
}

interface ReportDef {
  key: Key;
  icon: LucideIcon;
  permission: Permission;
  params: "period" | "asof" | "none";
  build: (c: Ctx) => Promise<Built>;
}

// ---- HTML building blocks (all values escaped) --------------------------------------------

const m = (v: string | number, lang: string) => formatMoney(v, undefined, lang);
const pct = (part: string, whole: string) => {
  const w = toUnits(whole) ?? 0n;
  return w > 0n ? `${(Number(((toUnits(part) ?? 0n) * 10000n) / w) / 100).toFixed(1)}%` : "—";
};

interface Col {
  label: string;
  num?: boolean;
}

function table(cols: Col[], rows: (string | number)[][], opts: { total?: (string | number)[]; groups?: Set<number> } = {}): string {
  if (!rows.length) return "";
  const th = cols.map((c) => `<th${c.num ? ' class="n"' : ""}>${esc(c.label)}</th>`).join("");
  const tr = (r: (string | number)[], cls = "") => `<tr${cls ? ` class="${cls}"` : ""}>${r.map((v, i) => `<td${cols[i]?.num ? ' class="n"' : ""}>${esc(v)}</td>`).join("")}</tr>`;
  const body = rows.map((r, i) => tr(r, opts.groups?.has(i) ? "group" : "")).join("");
  return `<table class="t"><thead><tr>${th}</tr></thead><tbody>${body}${opts.total ? tr(opts.total, "total") : ""}</tbody></table>`;
}

function kpis(items: [string, string][]): string {
  return `<div class="kpis">${items.map(([l, v]) => `<div class="kpi"><div class="l">${esc(l)}</div><div class="v">${esc(v)}</div></div>`).join("")}</div>`;
}

function section(title: string, html: string, empty: string): string {
  return `<h2 class="sec">${esc(title)}</h2>${html || `<div class="empty">${esc(empty)}</div>`}`;
}

function csvOf(heading: string, cols: Col[], rows: (string | number)[][], total?: (string | number)[]): CsvSection {
  return { heading, columns: cols.map((c) => c.label), rows: total ? [...rows, total] : rows };
}

function groupRows(list: GroupTotal[], label: (g: GroupTotal) => string, total: string, lang: string): (string | number)[][] {
  return list.map((g) => [label(g), g.count, m(g.total, lang), pct(g.total, total)]);
}

// ---- report definitions ------------------------------------------------------------------

const RESTRICTION_ORDER: FundRestriction[] = ["unrestricted", "temporarily_restricted", "permanently_restricted"];

const REPORTS: ReportDef[] = [
  {
    key: "activities",
    icon: Scale,
    permission: "reports.read",
    params: "period",
    async build({ from, to, t, lang }) {
      const rows = await api.get<ActivityRow[]>("/reports/activities", { from, to });
      const byAccount = new Map<string, { name: string; type: string; v: Record<FundRestriction, string> }>();
      for (const r of rows) {
        const e = byAccount.get(r.code) ?? { name: localName(r, lang), type: r.account_type, v: { unrestricted: "0", temporarily_restricted: "0", permanently_restricted: "0" } };
        e.v[r.restriction] = sum([e.v[r.restriction], r.amount]);
        byAccount.set(r.code, e);
      }
      const cols: Col[] = [{ label: t("common.code") }, { label: t("reports.account") }, ...RESTRICTION_ORDER.map((r) => ({ label: t(`restrictions.${r}`), num: true })), { label: t("common.total"), num: true }];
      const part = (type: string) => {
        const list = [...byAccount.entries()].filter(([, e]) => e.type === type);
        const lines = list.map(([code, e]) => [code, e.name, ...RESTRICTION_ORDER.map((r) => m(e.v[r], lang)), m(sum(RESTRICTION_ORDER.map((r) => e.v[r])), lang)]);
        const totals = RESTRICTION_ORDER.map((r) => sum(list.map(([, e]) => e.v[r])));
        return { lines, totals, all: sum(totals) };
      };
      const rev = part("revenue");
      const exp = part("expense");
      const net = RESTRICTION_ORDER.map((_, i) => fromUnits((toUnits(rev.totals[i]) ?? 0n) - (toUnits(exp.totals[i]) ?? 0n)));
      const netAll = fromUnits((toUnits(rev.all) ?? 0n) - (toUnits(exp.all) ?? 0n));
      const revTotal = ["", t("reports.totalRevenue"), ...rev.totals.map((v) => m(v, lang)), m(rev.all, lang)];
      const expTotal = ["", t("reports.totalExpenses"), ...exp.totals.map((v) => m(v, lang)), m(exp.all, lang)];
      const netRow = ["", t("reports.netChange"), ...net.map((v) => m(v, lang)), m(netAll, lang)];
      return {
        title: t("reports.activities"),
        subtitle: t("reports.periodLine", { from: gregorian(from, lang), to: gregorian(to, lang) }),
        body:
          kpis([
            [t("reports.totalRevenue"), m(rev.all, lang)],
            [t("reports.totalExpenses"), m(exp.all, lang)],
            [t("reports.netChange"), m(netAll, lang)],
            [t("reports.accounts"), String(byAccount.size)],
          ]) +
          section(t("reports.revenue"), table(cols, rev.lines, { total: revTotal }), t("common.noData")) +
          section(t("reports.expenses"), table(cols, exp.lines, { total: expTotal }), t("common.noData")) +
          section(t("reports.netChange"), table(cols, [netRow]), t("common.noData")),
        csv: [csvOf(t("reports.revenue"), cols, rev.lines, revTotal), csvOf(t("reports.expenses"), cols, exp.lines, expTotal), csvOf(t("reports.netChange"), cols, [netRow])],
      };
    },
  },
  {
    key: "funds",
    icon: Landmark,
    permission: "reports.read",
    params: "asof",
    async build({ asOf, t, lang }) {
      const rows = await api.get<FundBalance[]>("/reports/fund-balances", { as_of: asOf });
      const cols: Col[] = [{ label: t("common.code") }, { label: t("common.name") }, { label: t("funds.restriction") }, { label: t("funds.revenue"), num: true }, { label: t("funds.expenses"), num: true }, { label: t("funds.netAssets"), num: true }];
      const lines = rows.map((f) => [f.code, localName(f, lang), t(`restrictions.${f.restriction}`), m(f.revenue, lang), m(f.expenses, lang), m(f.net_assets, lang)]);
      const total = ["", t("common.total"), "", m(sum(rows.map((f) => f.revenue)), lang), m(sum(rows.map((f) => f.expenses)), lang), m(sum(rows.map((f) => f.net_assets)), lang)];
      const byRestriction = RESTRICTION_ORDER.map((r): [string, string] => [t(`restrictions.${r}`), m(sum(rows.filter((f) => f.restriction === r).map((f) => f.net_assets)), lang)]);
      return {
        title: t("reports.funds"),
        subtitle: t("reports.asOfLine", { date: gregorian(asOf, lang) }),
        body: kpis([...byRestriction, [t("funds.netAssets"), String(total[5])]]) + section(t("reports.funds"), table(cols, lines, { total }), t("common.noData")),
        csv: [csvOf(t("reports.funds"), cols, lines, total)],
      };
    },
  },
  {
    key: "trial",
    icon: BookOpen,
    permission: "ledger.read",
    params: "asof",
    async build({ asOf, t, lang }) {
      const rows = await api.get<TrialBalanceRow[]>("/reports/trial-balance", { as_of: asOf });
      const cols: Col[] = [{ label: t("common.code") }, { label: t("reports.account") }, { label: t("accounts.type") }, { label: t("journal.debit"), num: true }, { label: t("journal.credit"), num: true }];
      const lines = rows.map((r) => [r.code, localName(r, lang), t(`accountTypes.${r.account_type}`), m(r.debit, lang), m(r.credit, lang)]);
      const dr = sum(rows.map((r) => r.debit));
      const cr = sum(rows.map((r) => r.credit));
      const total = ["", t("common.total"), "", m(dr, lang), m(cr, lang)];
      return {
        title: t("accounts.trialBalance"),
        subtitle: t("reports.asOfLine", { date: gregorian(asOf, lang) }),
        body:
          kpis([
            [t("journal.debit"), m(dr, lang)],
            [t("journal.credit"), m(cr, lang)],
            [t("reports.difference"), m(fromUnits((toUnits(dr) ?? 0n) - (toUnits(cr) ?? 0n)), lang)],
            [t("reports.accounts"), String(rows.length)],
          ]) + section(t("accounts.trialBalance"), table(cols, lines, { total }), t("common.noData")),
        csv: [csvOf(t("accounts.trialBalance"), cols, lines, total)],
      };
    },
  },
  {
    key: "donations",
    icon: HandCoins,
    permission: "donations.read",
    params: "period",
    async build({ from, to, t, lang }) {
      const s = await api.get<DonationSummary>("/reports/donations-summary", { from, to });
      const g: Col[] = [{ label: t("reports.item") }, { label: t("reports.count"), num: true }, { label: t("common.amount"), num: true }, { label: t("reports.share"), num: true }];
      const named = (x: GroupTotal) => (lang.startsWith("ar") ? x.name_ar : x.name_en) ?? x.key;
      const sections: [string, (string | number)[][]][] = [
        [t("reports.byKind"), groupRows(s.by_kind, (x) => t(`kinds.${x.key}`), s.total, lang)],
        [t("reports.byFund"), groupRows(s.by_fund, (x) => `${x.key} · ${named(x)}`, s.total, lang)],
        [t("reports.byMethod"), groupRows(s.by_method, (x) => t(`methods.${x.key}`, { defaultValue: x.key }), s.total, lang)],
        [t("reports.byMonth"), groupRows(s.by_month, (x) => x.key, s.total, lang)],
        [t("reports.topDonors"), groupRows(s.top_donors, named, s.total, lang)],
      ];
      const avg = s.count ? fromUnits((toUnits(s.total) ?? 0n) / BigInt(s.count)) : "0";
      return {
        title: t("reports.donations"),
        subtitle: t("reports.periodLine", { from: gregorian(from, lang), to: gregorian(to, lang) }),
        body:
          kpis([
            [t("reports.totalDonations"), m(s.total, lang)],
            [t("reports.donationCount"), String(s.count)],
            [t("reports.donorCount"), String(s.donors)],
            [t("reports.average"), m(avg, lang)],
          ]) + sections.map(([h, rows]) => section(h, table(g, rows), t("common.noData"))).join(""),
        csv: sections.map(([h, rows]) => csvOf(h, g, rows)),
      };
    },
  },
  {
    key: "aid",
    icon: HandHeart,
    permission: "reports.read",
    params: "period",
    async build({ from, to, t, lang }) {
      const s = await api.get<AidSummary>("/reports/aid-summary", { from, to });
      const g: Col[] = [{ label: t("reports.item") }, { label: t("reports.count"), num: true }, { label: t("common.amount"), num: true }, { label: t("reports.share"), num: true }];
      const named = (x: GroupTotal) => `${x.key} · ${(lang.startsWith("ar") ? x.name_ar : x.name_en) ?? ""}`;
      const sections: [string, (string | number)[][]][] = [
        [t("reports.byAidKind"), groupRows(s.by_kind, (x) => t(`aidKinds.${x.key}`), s.total, lang)],
        [t("reports.byCategory"), groupRows(s.by_category, (x) => t(`asnaf.${x.key}`), s.total, lang)],
        [t("reports.byFund"), groupRows(s.by_fund, named, s.total, lang)],
        [t("reports.byMonth"), groupRows(s.by_month, (x) => x.key, s.total, lang)],
      ];
      const perCase = s.beneficiaries ? fromUnits((toUnits(s.total) ?? 0n) / BigInt(s.beneficiaries)) : "0";
      return {
        title: t("reports.aid"),
        subtitle: t("reports.periodLine", { from: gregorian(from, lang), to: gregorian(to, lang) }),
        body:
          kpis([
            [t("reports.totalAid"), m(s.total, lang)],
            [t("reports.distributions"), String(s.count)],
            [t("reports.beneficiariesServed"), String(s.beneficiaries)],
            [t("reports.perBeneficiary"), m(perCase, lang)],
          ]) + sections.map(([h, rows]) => section(h, table(g, rows), t("common.noData"))).join(""),
        csv: sections.map(([h, rows]) => csvOf(h, g, rows)),
      };
    },
  },
  {
    key: "budget",
    icon: PieChart,
    permission: "reports.read",
    params: "asof",
    async build({ asOf, t, lang }) {
      const rows = await api.get<BudgetUsage[]>("/budgets", { as_of: asOf });
      const cols: Col[] = [
        { label: t("journal.fund") },
        { label: t("journal.project") },
        { label: t("funds.period") },
        { label: t("funds.approved"), num: true },
        { label: t("funds.committed"), num: true },
        { label: t("funds.spent"), num: true },
        { label: t("reports.remaining"), num: true },
        { label: t("funds.usage"), num: true },
      ];
      const remaining = (b: BudgetUsage) => fromUnits((toUnits(b.approved) ?? 0n) - (toUnits(b.committed) ?? 0n) - (toUnits(b.spent) ?? 0n));
      const lines = rows.map((b) => [b.fund_code, b.project_code ?? "—", `${b.period_start} → ${b.period_end}`, m(b.approved, lang), m(b.committed, lang), m(b.spent, lang), m(remaining(b), lang), pct(sum([b.spent, b.committed]), b.approved)]);
      const ap = sum(rows.map((b) => b.approved));
      const used = sum(rows.flatMap((b) => [b.spent, b.committed]));
      const total = [t("common.total"), "", "", m(ap, lang), m(sum(rows.map((b) => b.committed)), lang), m(sum(rows.map((b) => b.spent)), lang), m(fromUnits((toUnits(ap) ?? 0n) - (toUnits(used) ?? 0n)), lang), pct(used, ap)];
      return {
        title: t("reports.budget"),
        subtitle: t("reports.asOfLine", { date: gregorian(asOf, lang) }),
        body:
          kpis([
            [t("funds.approved"), m(ap, lang)],
            [t("reports.used"), m(used, lang)],
            [t("reports.remaining"), String(total[6])],
            [t("funds.usage"), pct(used, ap)],
          ]) + section(t("reports.budget"), table(cols, lines, { total }), t("reports.noBudgets")),
        csv: [csvOf(t("reports.budget"), cols, lines, total)],
      };
    },
  },
  {
    key: "journal",
    icon: ListChecks,
    permission: "ledger.read",
    params: "period",
    async build({ from, to, t, lang }) {
      const rows = (await api.get<EntryView[]>("/journal", { from, to, limit: 500 })).slice().reverse();
      const cols: Col[] = [{ label: t("journal.no"), num: true }, { label: t("common.date") }, { label: t("common.hijri") }, { label: t("common.description") }, { label: t("journal.reference") }, { label: t("journal.source") }, { label: t("common.amount"), num: true }];
      const amount = (e: EntryView) => sum(e.lines.filter((l) => l.side === "debit").map((l) => l.base_amount));
      const lines = rows.map((e) => [e.entry_no, e.entry_date, e.hijri_date, e.description, e.reference ?? "", t(`sources.${e.source}`, { defaultValue: e.source }), m(amount(e), lang)]);
      const total = ["", "", "", t("common.total"), "", "", m(sum(rows.map(amount)), lang)];
      return {
        title: t("reports.journal"),
        subtitle: t("reports.periodLine", { from: gregorian(from, lang), to: gregorian(to, lang) }),
        body: kpis([
          [t("reports.entries"), String(rows.length)],
          [t("reports.posted"), String(rows.filter((e) => e.status === "posted").length)],
          [t("reports.reversed"), String(rows.filter((e) => e.status === "reversed").length)],
          [t("common.total"), String(total[6])],
        ]) + section(t("reports.journal"), table(cols, lines, { total }), t("common.noData")),
        csv: [csvOf(t("reports.journal"), cols, lines, total)],
      };
    },
  },
  {
    key: "beneficiaries",
    icon: Users,
    permission: "cases.read",
    params: "none",
    async build({ t, lang }) {
      const rows = await api.get<Beneficiary[]>("/beneficiaries", { limit: 500 });
      const cols: Col[] = [{ label: t("cases.fileNo"), num: true }, { label: t("common.name") }, { label: t("cases.category") }, { label: t("cases.city") }, { label: t("cases.score"), num: true }, { label: t("cases.priority") }, { label: t("common.status") }];
      const lines = rows.map((b) => [b.file_no, b.full_name, t(`asnaf.${b.category}`), b.city ?? "", b.score, t(`priority.${b.priority}`), t(`caseStatus.${b.status}`, { defaultValue: b.status })]);
      return {
        title: t("reports.beneficiaries"),
        subtitle: t("reports.asOfLine", { date: gregorian(today(), lang) }),
        body: kpis([
          [t("reports.cases"), String(rows.length)],
          [t("caseStatus.active"), String(rows.filter((b) => b.status === "active").length)],
          [t("priority.critical"), String(rows.filter((b) => b.priority === "critical").length)],
          [t("priority.high"), String(rows.filter((b) => b.priority === "high").length)],
        ]) + section(t("reports.beneficiaries"), table(cols, lines), t("common.noData")),
        csv: [csvOf(t("reports.beneficiaries"), cols, lines)],
      };
    },
  },
  {
    key: "sponsorships",
    icon: HeartHandshake,
    permission: "donations.read",
    params: "none",
    async build({ t, lang }) {
      const rows = await api.get<Sponsorship[]>("/sponsorships", { limit: 500 });
      const cols: Col[] = [
        { label: "#", num: true },
        { label: t("sponsorships.sponsor") },
        { label: t("sponsorships.beneficiary") },
        { label: t("sponsorships.program") },
        { label: t("sponsorships.monthly"), num: true },
        { label: t("sponsorships.paidDue"), num: true },
        { label: t("sponsorships.arrears"), num: true },
        { label: t("common.status") },
      ];
      const lines = rows.map((s) => [
        s.sponsorship_no,
        s.donor_name,
        s.beneficiary_name ?? t("sponsorships.generalProgram"),
        t(`programs.${s.program}`),
        `${m(s.monthly_amount, lang)} ${s.currency}`,
        t("sponsorships.paidOfDue", { paid: s.months_paid, due: s.months_due }),
        s.arrears_months,
        t(`sponsorships.status.${s.status}`),
      ]);
      const active = rows.filter((s) => s.status === "active");
      return {
        title: t("reports.sponsorships"),
        subtitle: t("reports.asOfLine", { date: gregorian(today(), lang) }),
        body:
          kpis([
            [t("sponsorships.active"), String(active.length)],
            [t("sponsorships.monthlyCommitment"), m(sum(active.map((s) => s.monthly_amount)), lang)],
            [t("sponsorships.inArrears"), String(active.filter((s) => s.arrears_months > 0).length)],
            [t("sponsorships.collected"), m(sum(rows.map((s) => s.amount_paid)), lang)],
          ]) + section(t("reports.sponsorships"), table(cols, lines), t("sponsorships.empty")),
        csv: [csvOf(t("reports.sponsorships"), cols, lines)],
      };
    },
  },
  {
    key: "volunteers",
    icon: Clock,
    permission: "reports.read",
    params: "period",
    async build({ from, to, t, lang }) {
      const s = await api.get<VolunteerSummary>("/reports/volunteers-summary", { from, to });
      const g: Col[] = [{ label: t("reports.item") }, { label: t("reports.count"), num: true }, { label: t("volunteers.hours"), num: true }];
      const rows = (list: VolunteerSummary["by_month"]) => list.map((x) => [x.key, x.entries, m(x.hours, lang)]);
      const sections: [string, (string | number)[][]][] = [
        [t("reports.byVolunteer"), rows(s.by_volunteer)],
        [t("reports.byActivity"), rows(s.by_activity)],
        [t("reports.byMonth"), rows(s.by_month)],
      ];
      return {
        title: t("reports.volunteers"),
        subtitle: t("reports.periodLine", { from: gregorian(from, lang), to: gregorian(to, lang) }),
        body:
          kpis([
            [t("volunteers.hoursInPeriod"), m(s.total_hours, lang)],
            [t("volunteers.activeInPeriod"), String(s.volunteers)],
            [t("reports.entries"), String(s.entries)],
            [t("reports.hoursPerVolunteer"), s.volunteers ? m(String(Number(s.total_hours) / s.volunteers), lang) : "0"],
          ]) + sections.map(([h, r]) => section(h, table(g, r), t("common.noData"))).join(""),
        csv: sections.map(([h, r]) => csvOf(h, g, r)),
      };
    },
  },
];

/** First day of the fiscal year containing today, from the organisation's start month. */
function fiscalYearStart(month: number | undefined): string {
  const start = Math.min(12, Math.max(1, month ?? 1));
  const now = new Date();
  const year = now.getMonth() + 1 >= start ? now.getFullYear() : now.getFullYear() - 1;
  return `${year}-${String(start).padStart(2, "0")}-01`;
}

function documentHtml(b: Built, c: Ctx, org: Organization, user: string): string {
  const meta = `<div class="meta">
<div><b>${esc(c.t("reports.currencyNote"))}</b>${esc(c.currency)}</div>
<div><b>${esc(c.t("reports.printedOn"))}</b>${esc(gregorian(new Date(), c.lang))} · ${esc(hijri(new Date(), c.lang))}</div>
<div><b>${esc(c.t("reports.preparedBy"))}</b>${esc(user)}</div></div>`;
  const signatures = `<div class="sign"><div><span>${esc(c.t("reports.preparedBy"))}</span></div><div><span>${esc(c.t("reports.reviewedBy"))}</span></div><div><span>${esc(c.t("reports.approvedBy"))}</span></div></div>`;
  return paperHtml({
    title: b.title,
    lang: c.lang,
    org,
    body: `<h1 class="title">${esc(b.title)}</h1><div class="subtitle">${esc(b.subtitle)}</div>${meta}${b.body}${signatures}`,
    footer: [`${c.lang.startsWith("ar") ? org.name_ar : org.name_en} — ${b.title}`, `${c.t("reports.generatedBy")} ${c.t("app.name")}`],
  });
}

export default function Reports() {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const user = useAuthStore((s) => s.session?.fullName ?? "");
  const currency = useRefs((s) => s.baseCurrency);
  const available = useMemo(() => REPORTS.filter((r) => can(r.permission)), [can]);
  const [key, setKey] = useState<Key>(available[0]?.key ?? "activities");
  const [from, setFrom] = useState(`${new Date().getFullYear()}-01-01`);
  const [to, setTo] = useState(today());
  const [asOf, setAsOf] = useState(today());
  const [org, setOrg] = useState<Organization | null>(null);
  const [state, setState] = useState<{ html: string; built: Built; ctx: Ctx } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const def = available.find((r) => r.key === key);

  useEffect(() => {
    api
      .get<Organization>("/settings/organization")
      .then((o) => {
        setOrg(o);
        setFrom(fiscalYearStart(o.fiscal_year_start));
      })
      .catch((e) => setError(errorMessage(e)));
  }, []);

  useEffect(() => {
    if (!def || !org) return;
    let cancelled = false;
    const ctx: Ctx = { from, to, asOf, t, lang: i18n.language, currency };
    setLoading(true);
    def
      .build(ctx)
      .then((built) => !cancelled && (setState({ html: documentHtml(built, ctx, org, user), built, ctx }), setError(null)))
      .catch((e) => !cancelled && setError(errorMessage(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [def, org, from, to, asOf, t, i18n.language, currency, user]);

  const fileName = state ? `${state.built.title}-${def?.params === "period" ? `${from}_${to}` : asOf}`.replace(/\s+/g, "-") : "report";

  return (
    <div className="page reports-page">
      <PageHeader title={t("reports.title")} />
      <div className="reports-layout">
        <nav className="report-list" aria-label={t("reports.title")}>
          {available.map((r) => (
            <button key={r.key} type="button" className={`report-card${r.key === key ? " active" : ""}`} onClick={() => setKey(r.key)}>
              <span className="rc-icon">
                <r.icon size={18} />
              </span>
              <span className="rc-text">
                <strong>{t(`reports.${r.key}`)}</strong>
                <small>{t(`reports.desc.${r.key}`)}</small>
              </span>
            </button>
          ))}
        </nav>

        <section className="report-stage">
          <div className="report-toolbar">
            <div className="report-params">
              {def?.params === "period" && (
                <>
                  <Field label={t("common.from")}>
                    <input type="date" value={from} max={to} onChange={(e) => e.target.value && setFrom(e.target.value)} />
                  </Field>
                  <Field label={t("common.to")}>
                    <input type="date" value={to} min={from} onChange={(e) => e.target.value && setTo(e.target.value)} />
                  </Field>
                </>
              )}
              {def?.params === "asof" && (
                <Field label={t("dashboard.asOf")}>
                  <input type="date" value={asOf} onChange={(e) => e.target.value && setAsOf(e.target.value)} />
                </Field>
              )}
            </div>
            <div className="report-actions">
              <button type="button" className="btn btn-primary" data-shortcut="print" disabled={!state || loading} onClick={() => state && printHtml(state.html)}>
                <Printer size={16} /> {t("reports.print")}
              </button>
              <button
                type="button"
                className="btn btn-secondary"
                disabled={!state || loading}
                onClick={() => {
                  toastInfo(t("reports.pdfHint"));
                  if (state) printHtml(state.html);
                }}
              >
                <FileDown size={16} /> PDF
              </button>
              <button type="button" className="btn btn-secondary" disabled={!state || loading} onClick={() => state && downloadWord(state.html, fileName)}>
                <FileText size={16} /> Word
              </button>
              <button type="button" className="btn btn-secondary" disabled={!state || loading} onClick={() => state && downloadCsv(fileName, state.built.csv)}>
                <FileSpreadsheet size={16} /> Excel
              </button>
            </div>
          </div>
          {error && <ErrorNote message={error} />}
          <div className={`paper-stage${loading ? " is-loading" : ""}`}>
            {state ? <PaperFrame title={state.built.title} html={state.html} /> : <Loading />}
          </div>
        </section>
      </div>
    </div>
  );
}
