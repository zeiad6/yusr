import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ShieldAlert, ShieldCheck } from "lucide-react";
import { api } from "../api/client";
import type { AuditRecord, VerifyReport } from "../api/types";
import Modal from "../components/Modal";
import { actorName } from "../utils/actor";
import { DataTable, ErrorNote, Field, PageHeader, useLoad, useSubmit } from "../components/ui";

// Entity labels written by audit::append across crates/yusr-infra and yusr-server.
const ENTITIES = ["", "journal_entry", "donor", "beneficiary", "zatca_invoice", "user", "settings", "account", "fund", "project", "budget", "fx_rate"];

export default function Audit() {
  const { t, i18n } = useTranslation();
  const [entity, setEntity] = useState("");
  const [report, setReport] = useState<VerifyReport | null>(null);
  const [viewing, setViewing] = useState<AuditRecord | null>(null);
  const { busy, run } = useSubmit();
  const { data, error, reload } = useLoad(() => api.get<AuditRecord[]>("/audit", { entity, limit: 500 }), [entity]);
  const time = (iso: string) => new Date(iso).toLocaleString(i18n.language.startsWith("ar") ? "ar-SA-u-nu-latn-ca-gregory" : "en-GB");

  return (
    <div className="page">
      <PageHeader title={t("audit.title")}>
        <Field label={t("audit.entity")}>
          <select value={entity} onChange={(e) => setEntity(e.target.value)}>
            {ENTITIES.map((e) => (
              <option key={e} value={e}>
                {e ? t(`entities.${e}`, { defaultValue: e }) : t("common.all")}
              </option>
            ))}
          </select>
        </Field>
        <button type="button" className="btn btn-primary" disabled={busy} onClick={() => run(async () => setReport(await api.get<VerifyReport>("/audit/verify")))}>
          <ShieldCheck size={16} /> {t("audit.verify")}
        </button>
      </PageHeader>

      {report && (
        <div className={report.ok ? "settings-note" : "settings-warning"} role="status">
          {report.ok ? <ShieldCheck size={16} /> : <ShieldAlert size={16} />}{" "}
          {report.ok ? t("audit.chainOk", { count: report.records_checked }) : t("audit.chainBroken", { error: report.error ?? "" })}
          {report.head_hash && (
            <>
              {" "}
              · <code className="hash">{report.head_hash.slice(0, 16)}…</code>
            </>
          )}
        </div>
      )}

      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={data ?? []}
        rowKey={(r) => String(r.seq)}
        exportName="audit-log"
        onRowClick={setViewing}
        columns={[
          { header: "#", cell: (r) => String(r.seq), num: true },
          { header: t("audit.time"), cell: (r) => time(r.occurred_at), text: (r) => r.occurred_at },
          { header: t("audit.actor"), cell: (r) => actorName(r.actor), text: (r) => r.actor },
          { header: t("audit.action"), cell: (r) => t(`actions.${r.action}`, { defaultValue: r.action }), text: (r) => r.action },
          { header: t("audit.entity"), cell: (r) => t(`entities.${r.entity}`, { defaultValue: r.entity }), text: (r) => r.entity },
          { header: t("audit.hash"), text: (r) => r.hash, cell: (r) => <code className="hash">{r.hash.slice(0, 12)}…</code> },
        ]}
      />

      {viewing && (
        <Modal isOpen onClose={() => setViewing(null)} size="lg" title={`${t("audit.record")} #${viewing.seq}`}>
          <div className="detail-grid">
            <div><span>{t("audit.time")}</span><strong>{time(viewing.occurred_at)}</strong></div>
            <div><span>{t("audit.actor")}</span><strong>{actorName(viewing.actor)}</strong></div>
            <div><span>{t("audit.entity")}</span><strong>{viewing.entity} · {viewing.entity_id}</strong></div>
            <div><span>{t("audit.prevHash")}</span><code className="hash">{viewing.prev_hash}</code></div>
            <div><span>{t("audit.hash")}</span><code className="hash">{viewing.hash}</code></div>
          </div>
          <pre className="json-view" dir="ltr">{JSON.stringify(viewing.payload, null, 2)}</pre>
        </Modal>
      )}
    </div>
  );
}
