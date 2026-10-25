import { useEffect, useMemo, useRef, useState, type DragEvent, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import {
  Ban,
  Download,
  Eye,
  File as FileIcon,
  FileImage,
  FileSpreadsheet,
  FileText,
  FolderOpen,
  HardDrive,
  Paperclip,
  Plus,
  Printer,
  Search,
  UploadCloud,
  X,
} from "lucide-react";
import { api, errorMessage } from "../api/client";
import {
  DOCUMENT_CATEGORIES,
  type ArchiveDocument,
  type DocumentCategory,
  type DocumentDetail,
  type DocumentFile,
  type NewDocument,
  type Organization,
} from "../api/types";
import Modal from "../components/Modal";
import { toastError, toastSuccess } from "../components/Toast";
import { Badge, ErrorNote, Field, Loading, PageHeader, StatCard, useLoad, useSubmit } from "../components/ui";
import { useAuthStore } from "../store/auth";
import EditRecord, { EditButton } from "../components/EditRecord";
import { gregorian, hijri, humanSize } from "../utils/dates";
import { today } from "../utils/money";
import { esc, paperHtml, printHtml } from "../utils/paper";
import { fetchBlob, MAX_FILE_BYTES, uploadFile } from "../utils/upload";
import { actorName } from "../utils/actor";

/** Only these open in the in-app viewer; anything else downloads, so no uploaded HTML ever runs. */
const PREVIEWABLE = new Set(["application/pdf", "image/png", "image/jpeg", "image/gif", "image/webp"]);

const CATEGORY_TONE: Record<DocumentCategory, string> = {
  incoming: "cat-teal",
  outgoing: "cat-brass",
  contract: "cat-indigo",
  financial: "cat-green",
  report: "cat-slate",
  beneficiary: "cat-rose",
  donor: "cat-amber",
  hr: "cat-violet",
  legal: "cat-red",
  other: "cat-slate",
};

function fileIcon(type: string) {
  if (type.startsWith("image/")) return FileImage;
  if (type === "application/pdf" || type.startsWith("text/")) return FileText;
  if (type.includes("sheet") || type.includes("excel") || type.includes("csv")) return FileSpreadsheet;
  return FileIcon;
}

export default function Archive() {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const [q, setQ] = useState("");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<DocumentCategory | "">("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [showVoided, setShowVoided] = useState(false);
  const [creating, setCreating] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);

  useEffect(() => {
    const id = setTimeout(() => setQuery(q.trim()), 300);
    return () => clearTimeout(id);
  }, [q]);

  // Counts per category come from the unfiltered-by-category list, so the chips stay informative.
  const { data, error, loading, reload } = useLoad(
    () => api.get<ArchiveDocument[]>("/archive/documents", { q: query, from, to, include_voided: showVoided || undefined, limit: 500 }),
    [query, from, to, showVoided],
  );
  const all = data ?? [];
  const docs = category ? all.filter((d) => d.category === category) : all;
  const counts = useMemo(() => {
    const m = new Map<DocumentCategory, number>();
    for (const d of all) m.set(d.category, (m.get(d.category) ?? 0) + 1);
    return m;
  }, [all]);
  const totals = useMemo(
    () => ({
      files: all.reduce((a, d) => a + d.file_count, 0),
      bytes: all.reduce((a, d) => a + d.total_bytes, 0),
      month: all.filter((d) => d.document_date.slice(0, 7) === today().slice(0, 7)).length,
    }),
    [all],
  );

  return (
    <div className="page archive">
      <PageHeader title={t("archive.title")}>
        {can("archive.write") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setCreating(true)}>
            <Plus size={16} /> {t("archive.new")}
          </button>
        )}
      </PageHeader>

      <div className="stats-grid">
        <StatCard tint="assets" icon={<FolderOpen size={20} />} value={String(all.length)} label={t("archive.documents")} />
        <StatCard tint="income" icon={<Paperclip size={20} />} value={String(totals.files)} label={t("archive.files")} />
        <StatCard tint="liabilities" icon={<HardDrive size={20} />} value={humanSize(totals.bytes, i18n.language)} label={t("archive.storage")} />
        <StatCard tint="equity" icon={<Plus size={20} />} value={String(totals.month)} label={t("archive.thisMonth")} />
      </div>

      <div className="archive-filters">
        <div className="search-box">
          <Search size={16} />
          <input type="search" placeholder={t("archive.searchHint")} value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <Field label={t("common.from")}>
          <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
        </Field>
        <Field label={t("common.to")}>
          <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
        </Field>
        <label className="checkbox-label">
          <input type="checkbox" checked={showVoided} onChange={(e) => setShowVoided(e.target.checked)} />
          {t("archive.showVoided")}
        </label>
      </div>

      <div className="chips" role="tablist" aria-label={t("archive.category")}>
        <button type="button" role="tab" aria-selected={category === ""} className={`chip${category === "" ? " active" : ""}`} onClick={() => setCategory("")}>
          {t("common.all")} <span>{all.length}</span>
        </button>
        {DOCUMENT_CATEGORIES.map((c) => (
          <button key={c} type="button" role="tab" aria-selected={category === c} className={`chip ${CATEGORY_TONE[c]}${category === c ? " active" : ""}`} onClick={() => setCategory(c)}>
            {t(`docCategories.${c}`)} <span>{counts.get(c) ?? 0}</span>
          </button>
        ))}
      </div>

      {error && <ErrorNote message={error} onRetry={reload} />}
      {loading && !data ? (
        <Loading />
      ) : docs.length === 0 ? (
        <div className="empty-state">
          <FolderOpen size={40} strokeWidth={1.25} />
          <p>{t("archive.empty")}</p>
          {can("archive.write") && (
            <button type="button" className="btn btn-primary" onClick={() => setCreating(true)}>
              <Plus size={16} /> {t("archive.new")}
            </button>
          )}
        </div>
      ) : (
        <div className="doc-grid">
          {docs.map((d) => (
            <button type="button" key={d.id} className={`doc-card${d.voided_at ? " voided" : ""}`} onClick={() => setOpenId(d.id)}>
              <div className="doc-card-top">
                <span className={`cat-pill ${CATEGORY_TONE[d.category]}`}>{t(`docCategories.${d.category}`)}</span>
                <code className="doc-no">{d.doc_no}</code>
              </div>
              <h3>{d.title}</h3>
              {d.party && <p className="doc-party">{d.party}</p>}
              <div className="doc-card-foot">
                <span>{gregorian(d.document_date, i18n.language)}</span>
                <span className="doc-files">
                  <Paperclip size={13} /> {d.file_count}
                </span>
              </div>
              {d.voided_at && <span className="void-stamp">{t("archive.voided")}</span>}
            </button>
          ))}
        </div>
      )}

      {creating && (
        <DocumentForm
          onClose={() => setCreating(false)}
          onSaved={(id) => {
            reload();
            setOpenId(id);
          }}
        />
      )}
      {openId && <DocumentModal id={openId} onClose={() => setOpenId(null)} onChanged={reload} />}
    </div>
  );
}

interface Pending {
  file: File;
  progress: number;
  error?: string;
}

function useDropzone() {
  const [pending, setPending] = useState<Pending[]>([]);
  const add = (files: FileList | File[]) => {
    const list = Array.from(files);
    const tooBig = list.filter((f) => f.size > MAX_FILE_BYTES);
    if (tooBig.length) toastError(`${tooBig.map((f) => f.name).join("، ")}: > ${MAX_FILE_BYTES / 1024 / 1024} MB`);
    setPending((p) => [...p, ...list.filter((f) => f.size > 0 && f.size <= MAX_FILE_BYTES).map((file) => ({ file, progress: 0 }))]);
  };
  return { pending, setPending, add };
}

function Dropzone({ onFiles }: { onFiles: (f: FileList) => void }) {
  const { t } = useTranslation();
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const drop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    if (e.dataTransfer.files.length) onFiles(e.dataTransfer.files);
  };
  return (
    <div
      className={`dropzone${over ? " over" : ""}`}
      role="button"
      tabIndex={0}
      onClick={() => input.current?.click()}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && input.current?.click()}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={drop}
    >
      <UploadCloud size={30} strokeWidth={1.5} />
      <strong>{t("archive.dropTitle")}</strong>
      <span>{t("archive.dropHint", { max: MAX_FILE_BYTES / 1024 / 1024 })}</span>
      <input ref={input} type="file" multiple hidden onChange={(e) => e.target.files && (onFiles(e.target.files), (e.target.value = ""))} />
    </div>
  );
}

function PendingList({ items, onRemove, busy }: { items: Pending[]; onRemove: (i: number) => void; busy: boolean }) {
  const { i18n } = useTranslation();
  if (!items.length) return null;
  return (
    <ul className="pending-files">
      {items.map((p, i) => {
        const Icon = fileIcon(p.file.type);
        return (
          <li key={`${p.file.name}-${i}`} className={p.error ? "failed" : p.progress >= 1 ? "done" : ""}>
            <Icon size={18} />
            <div>
              <span className="pf-name">{p.file.name}</span>
              <span className="pf-meta">{p.error ?? humanSize(p.file.size, i18n.language)}</span>
              <div className="pf-bar">
                <i style={{ width: `${Math.round(p.progress * 100)}%` }} />
              </div>
            </div>
            {!busy && p.progress === 0 && (
              <button type="button" className="btn-icon" onClick={() => onRemove(i)} aria-label="remove">
                <X size={14} />
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** Uploads one by one so each file gets its own progress bar and its own error. */
async function uploadAll(docId: string, pending: Pending[], setPending: (f: (p: Pending[]) => Pending[]) => void): Promise<number> {
  let failed = 0;
  for (let i = 0; i < pending.length; i++) {
    if (pending[i].progress >= 1) continue;
    try {
      await uploadFile(`/archive/documents/${docId}/files`, pending[i].file, (f) => setPending((p) => p.map((x, j) => (j === i ? { ...x, progress: Math.min(f, 0.99) } : x))));
      setPending((p) => p.map((x, j) => (j === i ? { ...x, progress: 1 } : x)));
    } catch (e) {
      failed++;
      setPending((p) => p.map((x, j) => (j === i ? { ...x, error: errorMessage(e) } : x)));
    }
  }
  return failed;
}

function DocumentForm({ onClose, onSaved }: { onClose: () => void; onSaved: (id: string) => void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState<NewDocument & { tagsText: string }>({
    title: "",
    category: "incoming",
    reference_no: null,
    document_date: today(),
    party: null,
    description: null,
    tags: [],
    tagsText: "",
  });
  const { pending, setPending, add } = useDropzone();
  const { busy, run } = useSubmit();
  const opt = (v: string) => v || null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const { tagsText, ...rest } = form;
      const doc = await api.post<ArchiveDocument>("/archive/documents", { ...rest, tags: tagsText.split(/[,،]/).map((s) => s.trim()).filter(Boolean) });
      const failed = await uploadAll(doc.id, pending, setPending);
      toastSuccess(t("archive.saved", { no: doc.doc_no }));
      if (failed) toastError(t("archive.someFailed", { count: failed }));
      onSaved(doc.id);
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} size="lg" title={t("archive.new")}>
      <form onSubmit={submit}>
        <div className="form-row">
          <Field label={t("archive.docTitle")}>
            <input required value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} />
          </Field>
          <Field label={t("archive.category")}>
            <select value={form.category} onChange={(e) => setForm({ ...form, category: e.target.value as DocumentCategory })}>
              {DOCUMENT_CATEGORIES.map((c) => (
                <option key={c} value={c}>
                  {t(`docCategories.${c}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <div className="form-row three">
          <Field label={t("archive.docDate")}>
            <input type="date" required value={form.document_date} onChange={(e) => setForm({ ...form, document_date: e.target.value })} />
          </Field>
          <Field label={t("archive.referenceNo")}>
            <input value={form.reference_no ?? ""} onChange={(e) => setForm({ ...form, reference_no: opt(e.target.value) })} />
          </Field>
          <Field label={t("archive.party")}>
            <input value={form.party ?? ""} onChange={(e) => setForm({ ...form, party: opt(e.target.value) })} />
          </Field>
        </div>
        <Field label={t("common.description")}>
          <textarea rows={2} value={form.description ?? ""} onChange={(e) => setForm({ ...form, description: opt(e.target.value) })} />
        </Field>
        <Field label={t("archive.tags")} hint={t("archive.tagsHint")}>
          <input value={form.tagsText} onChange={(e) => setForm({ ...form, tagsText: e.target.value })} />
        </Field>
        <Dropzone onFiles={add} />
        <PendingList items={pending} busy={busy} onRemove={(i) => setPending((p) => p.filter((_, j) => j !== i))} />
        <div className="form-actions">
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? t("common.saving") : t("archive.saveAndUpload")}
          </button>
          <button type="button" className="btn btn-secondary" onClick={onClose} disabled={busy}>
            {t("common.cancel")}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function DocumentModal({ id, onClose, onChanged }: { id: string; onClose: () => void; onChanged: () => void }) {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const { data: doc, error, reload } = useLoad(() => api.get<DocumentDetail>(`/archive/documents/${id}`), [id]);
  const [preview, setPreview] = useState<{ file: DocumentFile; url: string } | null>(null);
  const [voiding, setVoiding] = useState(false);
  const [editing, setEditing] = useState(false);
  const [reason, setReason] = useState("");
  const { pending, setPending, add } = useDropzone();
  const { busy, run } = useSubmit();

  useEffect(() => () => {
    if (preview) URL.revokeObjectURL(preview.url);
  }, [preview]);

  const open = (f: DocumentFile) =>
    run(async () => {
      const blob = await fetchBlob(`/archive/files/${f.id}`);
      if (PREVIEWABLE.has(f.content_type)) {
        // Re-typed explicitly: the viewer only ever gets a PDF or image MIME type.
        setPreview({ file: f, url: URL.createObjectURL(new Blob([blob], { type: f.content_type })) });
      } else {
        saveBlob(blob, f.file_name);
      }
    });

  const save = (f: DocumentFile) => run(async () => saveBlob(await fetchBlob(`/archive/files/${f.id}?download=true`), f.file_name));

  const uploadMore = () =>
    run(async () => {
      const failed = await uploadAll(id, pending, setPending);
      if (failed) toastError(t("archive.someFailed", { count: failed }));
      else toastSuccess(t("archive.uploaded"));
      setPending([]);
      reload();
      onChanged();
    });

  const voidDoc = () =>
    run(async () => {
      await api.post(`/archive/documents/${id}/void`, { reason });
      toastSuccess(t("archive.voidedOk"));
      setVoiding(false);
      reload();
      onChanged();
    });

  const printCard = () =>
    run(async () => {
      if (!doc) return;
      const org = await api.get<Organization>("/settings/organization");
      await printHtml(archiveCardHtml(doc, org, t, i18n.language));
    });

  return (
    <Modal isOpen onClose={onClose} size="xl" title={doc ? `${doc.doc_no} · ${doc.title}` : t("common.loading")}>
      {error && <ErrorNote message={error} onRetry={reload} />}
      {!doc ? (
        <Loading />
      ) : (
        <div className="doc-detail">
          {doc.voided_at && (
            <div className="settings-warning">
              <Ban size={15} /> {t("archive.voidedNote", { reason: doc.void_reason })}
            </div>
          )}
          <div className="detail-grid">
            <div><span>{t("archive.category")}</span><strong>{t(`docCategories.${doc.category}`)}</strong></div>
            <div><span>{t("archive.docDate")}</span><strong>{gregorian(doc.document_date, i18n.language)} · {hijri(doc.document_date, i18n.language)}</strong></div>
            <div><span>{t("archive.referenceNo")}</span><strong>{doc.reference_no ?? "—"}</strong></div>
            <div><span>{t("archive.party")}</span><strong>{doc.party ?? "—"}</strong></div>
            <div><span>{t("archive.filedBy")}</span><strong>{actorName(doc.created_by)}</strong></div>
            <div><span>{t("archive.tags")}</span><strong>{doc.tags.length ? doc.tags.map((g) => <Badge key={g} tone="info">{g}</Badge>) : "—"}</strong></div>
          </div>
          {doc.description && <p className="doc-desc">{doc.description}</p>}

          <div className="doc-files-layout">
            <ul className="file-list">
              {doc.files.length === 0 && <li className="no-data">{t("archive.noFiles")}</li>}
              {doc.files.map((f) => {
                const Icon = fileIcon(f.content_type);
                return (
                  <li key={f.id} className={preview?.file.id === f.id ? "active" : ""}>
                    <Icon size={20} />
                    <div className="fl-text">
                      <span className="fl-name">{f.file_name}</span>
                      <span className="fl-meta">
                        {humanSize(f.size_bytes, i18n.language)} · {actorName(f.uploaded_by)} · <code className="hash" title={f.sha256}>{f.sha256.slice(0, 10)}…</code>
                      </span>
                    </div>
                    {PREVIEWABLE.has(f.content_type) && (
                      <button type="button" className="btn-icon btn-view" title={t("archive.preview")} onClick={() => open(f)} disabled={busy}>
                        <Eye size={15} />
                      </button>
                    )}
                    <button type="button" className="btn-icon btn-export" title={t("archive.download")} onClick={() => save(f)} disabled={busy}>
                      <Download size={15} />
                    </button>
                  </li>
                );
              })}
            </ul>
            <div className="file-preview">
              {preview ? (
                preview.file.content_type === "application/pdf" ? (
                  <iframe title={preview.file.file_name} src={preview.url} />
                ) : (
                  <img src={preview.url} alt={preview.file.file_name} />
                )
              ) : (
                <div className="preview-empty">
                  <Eye size={28} strokeWidth={1.25} />
                  <span>{t("archive.previewHint")}</span>
                </div>
              )}
            </div>
          </div>

          {can("archive.write") && !doc.voided_at && (
            <>
              <Dropzone onFiles={add} />
              <PendingList items={pending} busy={busy} onRemove={(i) => setPending((p) => p.filter((_, j) => j !== i))} />
            </>
          )}

          <div className="form-actions">
            {pending.some((p) => p.progress === 0 && !p.error) && (
              <button type="button" className="btn btn-primary" onClick={uploadMore} disabled={busy}>
                <UploadCloud size={16} /> {t("archive.uploadFiles")}
              </button>
            )}
            <button type="button" className="btn btn-secondary" onClick={printCard} disabled={busy}>
              <Printer size={16} /> {t("archive.printCard")}
            </button>
            {can("archive.write") && !doc.voided_at && <EditButton onClick={() => setEditing(true)} />}
            {can("archive.void") && !doc.voided_at && !voiding && (
              <button type="button" className="btn btn-danger" onClick={() => setVoiding(true)}>
                <Ban size={16} /> {t("archive.void")}
              </button>
            )}
          </div>
          {editing && (
            <EditRecord
              title={t("archive.edit")}
              path={`/archive/documents/${doc.id}`}
              initial={doc}
              note={t("archive.editNote")}
              onClose={() => setEditing(false)}
              onSaved={() => (reload(), onChanged())}
              fields={[
                { key: "title", label: t("archive.docTitle"), required: true },
                { key: "reference_no", label: t("archive.referenceNo"), dir: "ltr" },
                { key: "document_date", label: t("archive.docDate"), type: "date", required: true },
                { key: "party", label: t("archive.party") },
                { key: "description", label: t("common.description"), type: "textarea" },
                { key: "tags", label: t("archive.tags"), type: "tags" },
              ]}
            />
          )}
          {voiding && (
            <div className="form-row inline-action">
              <Field label={t("archive.voidReason")}>
                <input autoFocus value={reason} onChange={(e) => setReason(e.target.value)} />
              </Field>
              <button type="button" className="btn btn-danger" disabled={busy || !reason.trim()} onClick={voidDoc}>
                {t("archive.confirmVoid")}
              </button>
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}

function saveBlob(blob: Blob, name: string) {
  // Forced octet-stream: the browser saves the file instead of rendering it.
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([blob], { type: "application/octet-stream" }));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

type T = (k: string, o?: Record<string, unknown>) => string;

/** Filing card (بطاقة أرشفة) printed and kept with the paper original. */
function archiveCardHtml(doc: DocumentDetail, org: Organization, t: T, lang: string): string {
  const row = (k: string, v: string) => `<tr><th style="width:32%">${esc(k)}</th><td>${esc(v)}</td></tr>`;
  const files = doc.files.length
    ? `<table class="t"><thead><tr><th>#</th><th>${esc(t("archive.fileName"))}</th><th class="n">${esc(t("archive.size"))}</th><th>SHA-256</th></tr></thead><tbody>${doc.files
        .map((f, i) => `<tr><td>${i + 1}</td><td>${esc(f.file_name)}</td><td class="n">${esc(humanSize(f.size_bytes, lang))}</td><td class="ltr" style="font-size:7pt">${esc(f.sha256)}</td></tr>`)
        .join("")}</tbody></table>`
    : `<div class="empty">${esc(t("archive.noFiles"))}</div>`;
  const body = `<h1 class="title">${esc(t("archive.cardTitle"))}</h1><div class="subtitle">${esc(doc.doc_no)}</div>
<table class="t" style="margin-bottom:6mm"><tbody>
${row(t("archive.docTitle"), doc.title)}${row(t("archive.category"), t(`docCategories.${doc.category}`))}
${row(t("archive.docDate"), `${gregorian(doc.document_date, lang)} — ${hijri(doc.document_date, lang)}`)}
${row(t("archive.referenceNo"), doc.reference_no ?? "—")}${row(t("archive.party"), doc.party ?? "—")}
${row(t("archive.tags"), doc.tags.join("، ") || "—")}${row(t("archive.filedBy"), actorName(doc.created_by))}
${doc.voided_at ? row(t("archive.voided"), doc.void_reason ?? "") : ""}
</tbody></table>${doc.description ? `<h2 class="sec">${esc(t("common.description"))}</h2><p>${esc(doc.description)}</p>` : ""}
<h2 class="sec">${esc(t("archive.files"))}</h2>${files}
<div class="sign"><div><span>${esc(t("archive.archivist"))}</span></div><div><span>${esc(t("reports.reviewedBy"))}</span></div><div><span>${esc(t("letters.stamp"))}</span></div></div>`;
  return paperHtml({ title: doc.doc_no, lang, org, body, footer: [doc.doc_no, `${t("reports.printedOn")} ${gregorian(new Date(), lang)}`] });
}

