import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Bot, DatabaseBackup, Download, FileCheck2, Pause, Play, Power, RotateCcw, ShieldAlert } from "lucide-react";
import { api, errorMessage } from "../api/client";
import ConfirmModal from "./ConfirmModal";
import { toastError, toastSuccess } from "./Toast";
import { Badge, Field, useLoad, useSubmit } from "./ui";
import { useAuthStore } from "../store/auth";

interface BackupSummary {
  created_at: string;
  created_by: string;
  app_version: string;
  base_currency: string;
  tables: number;
  rows: number;
  audit_records: number;
  compatible: boolean;
  problem: string | null;
  table_rows: { name: string; rows: number }[];
}

interface RestoreReport {
  tables: number;
  rows: number;
  audit_records: number;
  backup_created_at: string;
  verified: boolean;
}

function saveBlob(blob: Blob, name: string) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

/** Full backup download, integrity check of a backup file, and verified restore. */
export function BackupSection() {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const logout = useAuthStore((s) => s.logout);
  const fileInput = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<File | null>(null);
  const [summary, setSummary] = useState<BackupSummary | null>(null);
  const [lastHash, setLastHash] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [confirming, setConfirming] = useState(false);
  const { busy, run } = useSubmit();
  const when = (iso: string) => new Date(iso).toLocaleString(i18n.language === "ar" ? "ar-SA-u-ca-gregory" : "en-GB");

  const backup = () =>
    run(async () => {
      const res = await api.raw("GET", "/backup");
      const name = /filename="([^"]+)"/.exec(res.headers.get("content-disposition") ?? "")?.[1] ?? "yusr-backup.yusrbak";
      saveBlob(await res.blob(), name);
      setLastHash(res.headers.get("x-backup-sha256"));
      toastSuccess(t("backup.done", { rows: res.headers.get("x-backup-rows") ?? "?" }));
    });

  const pick = (f: File | null) => {
    setFile(f);
    setSummary(null);
    if (!f) return;
    run(async () => {
      const res = await api.raw("POST", "/backup/verify", f);
      setSummary((await res.json()) as BackupSummary);
    });
  };

  const restore = () =>
    run(async () => {
      if (!file) return;
      const form = new FormData();
      form.append("password", password);
      form.append("file", file);
      try {
        const res = await api.raw("POST", "/backup/restore", form);
        const report = (await res.json()) as RestoreReport;
        toastSuccess(t("backup.restored", { rows: report.rows, tables: report.tables }));
        // Every session was signed out by the restore, this one included.
        setTimeout(logout, 1500);
      } catch (e) {
        toastError(errorMessage(e));
      } finally {
        setConfirming(false);
        setPassword("");
      }
    });

  return (
    <section className="settings-section">
      <div className="settings-section-head">
        <h3>
          <DatabaseBackup size={17} /> {t("backup.title")}
        </h3>
        <small className="settings-hint">{t("backup.hint")}</small>
      </div>
      <ul className="backup-facts">
        <li>{t("backup.fact1")}</li>
        <li>{t("backup.fact2")}</li>
        <li>{t("backup.fact3")}</li>
      </ul>
      <div className="form-actions">
        <button type="button" className="btn btn-primary" disabled={busy} onClick={backup}>
          <Download size={16} /> {busy ? t("backup.working") : t("backup.create")}
        </button>
        <button type="button" className="btn btn-secondary" disabled={busy} onClick={() => fileInput.current?.click()}>
          <FileCheck2 size={16} /> {t("backup.check")}
        </button>
        <input ref={fileInput} type="file" accept=".yusrbak" hidden onChange={(e) => pick(e.target.files?.[0] ?? null)} />
      </div>
      {lastHash && (
        <p className="settings-hint">
          {t("backup.sha")}: <code dir="ltr" className="hash">{lastHash}</code>
        </p>
      )}

      {summary && (
        <div className={`backup-summary${summary.compatible ? "" : " is-bad"}`}>
          <div className="detail-grid">
            <div><span>{t("backup.createdAt")}</span><strong>{when(summary.created_at)}</strong></div>
            <div><span>{t("backup.createdBy")}</span><strong>{summary.created_by.replace(/^user:|^system:/, "")}</strong></div>
            <div><span>{t("backup.tables")}</span><strong>{summary.tables}</strong></div>
            <div><span>{t("backup.rows")}</span><strong>{summary.rows.toLocaleString(i18n.language)}</strong></div>
            <div><span>{t("backup.auditRecords")}</span><strong>{summary.audit_records}</strong></div>
            <div>
              <span>{t("backup.integrity")}</span>
              {summary.compatible ? <Badge tone="success">{t("backup.intact")}</Badge> : <Badge tone="danger">{t("backup.incompatible")}</Badge>}
            </div>
          </div>
          {summary.problem && <p className="settings-note">{summary.problem}</p>}
          {can("backup.restore") && summary.compatible && (
            <div className="restore-box">
              <p className="restore-warning">
                <ShieldAlert size={16} /> {t("backup.restoreWarning")}
              </p>
              <form
                className="form-row inline-action"
                onSubmit={(e: FormEvent) => {
                  e.preventDefault();
                  setConfirming(true);
                }}
              >
                <Field label={t("settings.currentPassword")}>
                  <input type="password" required autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} />
                </Field>
                <button type="submit" className="btn btn-danger" disabled={busy || !password}>
                  <RotateCcw size={16} /> {t("backup.restore")}
                </button>
              </form>
            </div>
          )}
        </div>
      )}
      <ConfirmModal
        isOpen={confirming}
        onClose={() => setConfirming(false)}
        onConfirm={restore}
        title={t("backup.confirmTitle")}
        message={t("backup.confirmText", { date: summary ? when(summary.created_at) : "" })}
        confirmText={t("backup.restore")}
        cancelText={t("common.cancel")}
        variant="danger"
      />
    </section>
  );
}

interface AdvisorStatus {
  enabled: boolean;
  model: string;
  url: string | null;
  reachable: boolean;
  models: string[] | null;
  error: string | null;
  /** Only in the desktop package that downloads its model. */
  local?: LocalModel | null;
}

interface LocalModel {
  state: "missing" | "downloading" | "paused" | "stopped" | "running";
  downloaded: number;
  total: number;
  error: string | null;
}

/** How often the page refreshes while a download runs or the model is loading. */
const LOCAL_MODEL_POLL_MS = 2000;
const MB = 1024 * 1024;

/** Download (in the background, resumable), start and stop the app's own model. */
function LocalModelPanel({ local, reachable, onChange }: { local: LocalModel; reachable: boolean; onChange: () => void }) {
  const { t } = useTranslation();
  const { busy, run } = useSubmit();
  const act = (action: string) => run(async () => {
    await api.post(`/advisor/local/${action}`, {});
    onChange();
  });
  const mb = (n: number) => (n / MB).toLocaleString(undefined, { maximumFractionDigits: 0 });
  const tone = { missing: "secondary", downloading: "info", paused: "warning", stopped: "secondary", running: "success" } as const;
  const button = {
    missing: { action: "download", icon: <Download size={15} />, label: t("advisor.local.download") },
    downloading: { action: "pause", icon: <Pause size={15} />, label: t("advisor.local.pause") },
    paused: { action: "download", icon: <Play size={15} />, label: t("advisor.local.resume") },
    stopped: { action: "start", icon: <Power size={15} />, label: t("advisor.local.start") },
    running: { action: "stop", icon: <Power size={15} />, label: t("advisor.local.stop") },
  }[local.state];
  const showProgress = local.state === "downloading" || local.state === "paused";

  return (
    <div className="local-model">
      <p className="advisor-state">
        <Badge tone={tone[local.state]}>{t(`advisor.local.state.${local.state}`)}</Badge>
        {local.state === "running" && !reachable && <small className="settings-hint">{t("advisor.local.loading")}</small>}
        {local.state === "missing" && <small className="settings-hint">{t("advisor.local.missingHint")}</small>}
      </p>
      {showProgress && (
        <p className="advisor-state">
          <progress max={local.total || undefined} value={local.downloaded} />
          <small className="settings-hint" dir="ltr">
            {local.total ? `${mb(local.downloaded)} / ${mb(local.total)} MB` : `${mb(local.downloaded)} MB`}
          </small>
        </p>
      )}
      {local.error && <small className="settings-hint" dir="ltr">{local.error}</small>}
      <div className="form-actions">
        <button type="button" className={`btn ${local.state === "running" ? "btn-secondary" : "btn-primary"}`} disabled={busy} onClick={() => act(button.action)}>
          {button.icon} {button.label}
        </button>
      </div>
    </div>
  );
}

/** Where the advisor's local model runs; admins only. */
export function AdvisorSection() {
  const { t } = useTranslation();
  const { data, reload } = useLoad(() => api.get<AdvisorStatus>("/advisor/status"));
  const local = data?.local;
  const polling = local?.state === "downloading" || (local?.state === "running" && !data?.reachable);
  useEffect(() => {
    if (!polling) return;
    const id = setInterval(reload, LOCAL_MODEL_POLL_MS);
    return () => clearInterval(id);
  }, [polling, reload]);
  const [form, setForm] = useState<{ enabled: boolean; url: string; model: string } | null>(null);
  const { busy, run } = useSubmit();
  const current = form ?? (data ? { enabled: data.enabled, url: data.url ?? "", model: data.model } : null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!current) return;
    run(async () => {
      await api.put("/advisor/settings", { ...current, url: current.url.trim().replace(/\/+$/, ""), model: current.model.trim() });
      toastSuccess(t("common.saved"));
      setForm(null);
      reload();
    });
  };

  return (
    <form className="settings-section" onSubmit={submit}>
      <div className="settings-section-head">
        <h3>
          <Bot size={17} /> {t("advisor.settingsTitle")}
        </h3>
        <small className="settings-hint">{t("advisor.settingsHint")}</small>
      </div>
      {data && (
        <p className="advisor-state">
          {data.reachable ? <Badge tone="success">{t("advisor.connected")}</Badge> : <Badge tone="warning">{t("advisor.notConnected")}</Badge>}
          {!data.reachable && data.error && <small className="settings-hint" dir="ltr">{data.error}</small>}
        </p>
      )}
      {local && <LocalModelPanel local={local} reachable={!!data?.reachable} onChange={reload} />}
      {current && (
        <>
          <label className="checkbox-label">
            <input type="checkbox" checked={current.enabled} onChange={(e) => setForm({ ...current, enabled: e.target.checked })} />
            {t("advisor.enabled")}
          </label>
          <div className="form-row">
            <Field label={t("advisor.url")} hint={t("advisor.urlHint")}>
              <input dir="ltr" required value={current.url} onChange={(e) => setForm({ ...current, url: e.target.value })} />
            </Field>
            <Field label={t("advisor.model")} hint={t("advisor.modelHint")}>
              <input dir="ltr" required list="advisor-models" value={current.model} onChange={(e) => setForm({ ...current, model: e.target.value })} />
              <datalist id="advisor-models">
                {(data?.models ?? []).map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            </Field>
          </div>
          <div className="form-actions">
            <button type="submit" className="btn btn-primary" disabled={busy}>
              {t("common.save")}
            </button>
            <button type="button" className="btn btn-secondary" disabled={busy} onClick={reload}>
              {t("advisor.test")}
            </button>
          </div>
        </>
      )}
    </form>
  );
}
