import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Download, Search } from "lucide-react";
import { download, errorMessage } from "../api/client";
import { toastError } from "./Toast";

/** Loads data on mount and on `reload()`; surfaces failures as a toast and an inline error. */
export function useLoad<T>(load: () => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const run = useCallback(load, deps);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      setData(await run());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
      toastError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [run]);

  useEffect(() => {
    reload();
  }, [reload]);

  return { data, error, loading, reload, setData };
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="page-header">
      <h2>{title}</h2>
      {children && <div className="header-actions">{children}</div>}
    </div>
  );
}

export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <div className="form-group">
      <label>{label}</label>
      {children}
      {hint && <small className="settings-hint">{hint}</small>}
    </div>
  );
}

export function Badge({ tone = "secondary", children }: { tone?: "success" | "warning" | "danger" | "info" | "secondary"; children: ReactNode }) {
  return <span className={`badge badge-${tone}`}>{children}</span>;
}

export function StatCard({ icon, tint, value, label }: { icon: ReactNode; tint: "income" | "equity" | "assets" | "liabilities"; value: ReactNode; label: string }) {
  return (
    <div className="stat-card">
      <div className={`stat-icon tint-${tint}`}>{icon}</div>
      <div className="stat-info">
        <span className="stat-value">{value}</span>
        <span className="stat-label">{label}</span>
      </div>
    </div>
  );
}

export function Tabs<K extends string>({ tabs, active, onChange }: { tabs: { key: K; label: string }[]; active: K; onChange: (k: K) => void }) {
  return (
    <div className="report-tabs" role="tablist">
      {tabs.map((tab) => (
        <button
          key={tab.key}
          type="button"
          role="tab"
          aria-selected={active === tab.key}
          className={`report-tab ${active === tab.key ? "active" : ""}`}
          onClick={() => onChange(tab.key)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

export interface Column<T> {
  header: string;
  cell: (row: T) => ReactNode;
  /** Plain value for search and CSV; defaults to the rendered cell when it is a string. */
  text?: (row: T) => string;
  num?: boolean;
}

const PAGE_SIZE = 25;

function csvEscape(v: string) {
  return /[",\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v;
}

/** Client-side searchable, paginated table with CSV export: lists are capped server-side (≤ 500 rows). */
export function DataTable<T>({
  rows,
  columns,
  rowKey,
  exportName,
  onRowClick,
  searchable = true,
  empty,
}: {
  rows: T[];
  columns: Column<T>[];
  rowKey: (row: T) => string;
  exportName?: string;
  onRowClick?: (row: T) => void;
  searchable?: boolean;
  empty?: string;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);

  const textOf = useCallback(
    (row: T, c: Column<T>) => {
      if (c.text) return c.text(row);
      const v = c.cell(row);
      return typeof v === "string" || typeof v === "number" ? String(v) : "";
    },
    [],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter((r) => columns.some((c) => textOf(r, c).toLowerCase().includes(q)));
  }, [rows, columns, query, textOf]);

  const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const current = Math.min(page, pages - 1);
  const visible = filtered.slice(current * PAGE_SIZE, (current + 1) * PAGE_SIZE);

  const exportCsv = () => {
    const lines = [columns.map((c) => csvEscape(c.header)).join(",")];
    for (const r of filtered) lines.push(columns.map((c) => csvEscape(textOf(r, c))).join(","));
    // BOM so Excel opens Arabic text as UTF-8.
    download(`${exportName}.csv`, "﻿" + lines.join("\n"), "text/csv;charset=utf-8");
  };

  return (
    <>
      {(searchable || exportName) && (
        <div className="toolbar">
          {searchable ? (
            <div className="search-box">
              <Search size={16} />
              <input
                type="search"
                placeholder={t("common.search")}
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setPage(0);
                }}
              />
            </div>
          ) : (
            <span />
          )}
          {exportName && (
            <button type="button" className="toolbar-btn" onClick={exportCsv} disabled={filtered.length === 0}>
              <Download size={14} /> {t("common.exportCsv")}
            </button>
          )}
        </div>
      )}
      <div className="table-container">
        <table className="data-table">
          <thead>
            <tr>
              {columns.map((c) => (
                <th key={c.header} className={c.num ? "num" : undefined}>
                  {c.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visible.length === 0 ? (
              <tr>
                <td colSpan={columns.length} className="no-data">
                  {empty ?? t("common.noData")}
                </td>
              </tr>
            ) : (
              visible.map((r) => (
                <tr key={rowKey(r)} onClick={onRowClick ? () => onRowClick(r) : undefined} style={onRowClick ? { cursor: "pointer" } : undefined}>
                  {columns.map((c) => (
                    <td key={c.header} className={c.num ? "num" : undefined}>
                      {c.cell(r)}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {pages > 1 && (
        <div className="pagination">
          <span className="pagination-current-page">
            {t("common.pageOf", { page: current + 1, pages })} · {t("common.rows", { count: filtered.length })}
          </span>
          <div className="pagination-controls">
            <button type="button" className="btn btn-sm btn-secondary" disabled={current === 0} onClick={() => setPage(current - 1)}>
              {t("common.prev")}
            </button>
            <button type="button" className="btn btn-sm btn-secondary" disabled={current >= pages - 1} onClick={() => setPage(current + 1)}>
              {t("common.next")}
            </button>
          </div>
        </div>
      )}
    </>
  );
}

export function Loading() {
  const { t } = useTranslation();
  return <div className="loading">{t("common.loading")}</div>;
}

export function ErrorNote({ message, onRetry }: { message: string; onRetry?: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="error-message" role="alert">
      {message}{" "}
      {onRetry && (
        <button type="button" className="btn btn-sm btn-secondary" onClick={onRetry}>
          {t("common.retry")}
        </button>
      )}
    </div>
  );
}

/** Wraps a submit handler with a busy flag and error toast. */
export function useSubmit() {
  const [busy, setBusy] = useState(false);
  const run = useCallback(async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      toastError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }, []);
  return { busy, run };
}
