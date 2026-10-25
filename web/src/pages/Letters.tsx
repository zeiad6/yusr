import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, BookMarked, FileDown, FileText, Lock, Plus, Printer, Save, Search, Send } from "lucide-react";
import { api, errorMessage } from "../api/client";
import { LETTER_KINDS, type ArchiveDocument, type Letter, type LetterDraft, type LetterKind, type Organization } from "../api/types";
import ConfirmModal from "../components/ConfirmModal";
import PaperFrame from "../components/PaperFrame";
import { toastError, toastInfo, toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, Loading, PageHeader, Tabs, useLoad, useSubmit } from "../components/ui";
import { useAuthStore } from "../store/auth";
import { gregorian, hijri, hijriText } from "../utils/dates";
import { today } from "../utils/money";
import { downloadWord, esc, paperHtml, paragraphs, printHtml } from "../utils/paper";
import { uploadFile } from "../utils/upload";
import { actorName } from "../utils/actor";

/** Kinds printed as a framed telegram (برقية) rather than a plain letter. */
const TELEGRAM: LetterKind[] = ["congratulation", "condolence", "thanks"];

interface Template {
  subject: string;
  body: string;
  closing: string;
  recipient?: string;
}

/** Arabic starting texts; `{org}` becomes the organisation's name. The "…" gaps are for the writer. */
const TEMPLATES: Record<LetterKind, Template> = {
  official: {
    subject: "",
    body: "إشارة إلى …، نفيدكم بأن …\n\nوعليه نأمل التكرم بـ …",
    closing: "وتقبلوا فائق الاحترام والتقدير،،،",
  },
  congratulation: {
    subject: "تهنئة بمناسبة …",
    body: "يسر {org} أن تتقدم إليكم بأصدق التهاني وأطيب التبريكات بمناسبة …، سائلين المولى عز وجل أن يجعله فاتحة خير وبركة، وأن يديم عليكم التوفيق والسداد.",
    closing: "وتقبلوا أطيب تحياتنا،،،",
  },
  condolence: {
    subject: "تعزية في وفاة …",
    body:
      "ببالغ الحزن والأسى تلقينا نبأ وفاة … رحمه الله، وإننا في {org} نتقدم إليكم بخالص العزاء وصادق المواساة، سائلين الله العلي القدير أن يتغمد الفقيد بواسع رحمته ومغفرته، وأن يسكنه فسيح جناته، وأن يلهمكم وذويه الصبر والسلوان.\n\n«إنا لله وإنا إليه راجعون»",
    closing: "وتقبلوا خالص تعازينا،،،",
  },
  thanks: {
    subject: "شكر وتقدير",
    body: "يطيب لـ{org} أن تتقدم إليكم بجزيل الشكر وعظيم الامتنان على …، مما كان له أبلغ الأثر في خدمة المستفيدين، سائلين الله أن يجزيكم خير الجزاء، وأن يجعل ذلك في موازين حسناتكم.",
    closing: "وتقبلوا فائق الشكر والتقدير،،،",
  },
  invitation: {
    subject: "دعوة لحضور …",
    body: "يسر {org} دعوتكم لحضور …، وذلك بمشيئة الله تعالى يوم … الموافق … في تمام الساعة … بمقر …\n\nونتطلع إلى تشريفكم ومشاركتكم.",
    closing: "وتقبلوا خالص التحية والتقدير،،،",
  },
  circular: {
    subject: "تعميم بشأن …",
    body: "تعمم {org} على جميع … بأنه …\n\nوعلى الجميع الاطلاع والعمل بموجبه.",
    closing: "والله الموفق،،،",
    recipient: "جميع منسوبي الجمعية",
  },
};

const LETTER_CSS = `
.lt-meta{display:flex;justify-content:flex-end;margin:-2mm 0 6mm}
.lt-meta table{border-collapse:collapse;font-size:9.5pt}
.lt-meta td{padding:.6mm 2.5mm;border-bottom:.5pt dotted #b9b2a3}.lt-meta td:first-child{color:#5c6968;font-weight:600}
.lt-to{font-size:12pt;font-weight:700;margin:0 0 4mm;display:flex;justify-content:space-between;gap:8mm}
.lt-to small{display:block;font-weight:400;font-size:10pt;color:#3b4847}
.greet{margin:0 0 4mm}
.lt-subject{text-align:center;font-weight:700;font-size:11.5pt;margin:0 0 5mm}
.lt-subject span{border-bottom:1pt solid #a98336;padding-bottom:.8mm}
.lt-body p{margin:0 0 3.5mm;text-align:justify;text-indent:8mm;font-size:11.5pt;line-height:1.95}
.lt-closing{text-align:center;font-weight:600;margin:6mm 0 0}
.lt-sign{display:flex;justify-content:flex-end;margin-top:10mm}
.lt-sign>div{text-align:center;min-width:62mm}
.lt-sign .role{color:#3b4847}.lt-sign .who{font-weight:700;font-size:11.5pt;margin-top:12mm}
.stamp{width:28mm;height:28mm;border:.8pt dashed #a98336;border-radius:50%;margin:4mm auto 0;display:flex;align-items:center;justify-content:center;font-size:8pt;color:#a98336}
.stamp-img{display:block;max-width:34mm;max-height:34mm;margin:3mm auto 0;object-fit:contain;opacity:.92}
.lt-cc{margin-top:8mm;font-size:9pt;color:#3b4847}.lt-cc b{display:block}
.tg{border:3pt double #a98336;padding:7mm 8mm 8mm;border-radius:1.5mm;position:relative;margin-top:2mm}
.tg-band{position:absolute;top:-4.2mm;inset-inline-start:50%;transform:translateX(50%);background:#fff;padding:0 6mm;font-weight:700;font-size:12pt;color:#0f766e;letter-spacing:.5pt}
[dir=ltr] .tg-band{transform:translateX(-50%)}
.tg.condolence{border-color:#57534e}.tg.condolence .tg-band{color:#3f3b37}
.watermark{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;pointer-events:none;font-size:72pt;font-weight:700;color:rgba(169,131,54,.11);transform:rotate(-28deg)}
`;

function fillTemplate(text: string, org: Organization | null) {
  return text.split("{org}").join(org?.name_ar || "الجمعية");
}

/** The letter as printed; letters are written in Arabic, so labels stay Arabic. */
export function letterHtml(d: LetterDraft, refNo: string | null, hijriDate: string | null, org: Organization): string {
  const date = `${hijriDate ? hijriText(hijriDate, "ar") : hijri(d.letter_date, "ar")} — ${gregorian(d.letter_date, "ar")}`;
  const meta = `<div class="lt-meta"><table>
<tr><td>الرقم</td><td>${esc(refNo ?? "—")}</td></tr>
<tr><td>التاريخ</td><td>${esc(date)}</td></tr>
<tr><td>المرفقات</td><td>${esc(d.attachments || "—")}</td></tr></table></div>`;
  const to = `<div class="lt-to"><div>${esc(d.recipient_title ? `${d.recipient_title} / ` : "")}${esc(d.recipient_name)}${d.recipient_org ? `<small>${esc(d.recipient_org)}</small>` : ""}</div>${d.kind === "circular" ? "" : "<div>المحترم</div>"}</div>`;
  const content = `${to}<p class="greet">السلام عليكم ورحمة الله وبركاته، وبعد:</p>
<div class="lt-subject"><span>الموضوع: ${esc(d.subject)}</span></div>
<div class="lt-body">${paragraphs(d.body)}</div>
${d.closing ? `<p class="lt-closing">${esc(d.closing)}</p>` : ""}
<div class="lt-sign"><div><div class="role">${esc(d.signatory_title ?? "")}</div><div class="who">${esc(d.signatory_name)}</div>${org.stamp_data_url ? `<img class="stamp-img" src="${esc(org.stamp_data_url)}" alt="">` : `<div class="stamp">الختم</div>`}</div></div>`;
  const kindTitle: Record<LetterKind, string> = {
    official: "خطاب رسمي",
    congratulation: "برقية تهنئة",
    condolence: "برقية تعزية",
    thanks: "برقية شكر وتقدير",
    invitation: "دعوة",
    circular: "تعميم",
  };
  const framed = TELEGRAM.includes(d.kind) ? `<div class="tg ${d.kind}"><div class="tg-band">${kindTitle[d.kind]}</div>${content}</div>` : content;
  const cc = d.cc
    ? `<div class="lt-cc"><b>صورة مع التحية لـ:</b>${d.cc
        .split("\n")
        .filter((l) => l.trim())
        .map((l) => `- ${esc(l.trim())}`)
        .join("<br>")}</div>`
    : "";
  const watermark = refNo ? "" : `<div class="watermark">مسودة</div>`;
  return paperHtml({
    title: refNo ? `${kindTitle[d.kind]} ${refNo}` : d.subject || kindTitle[d.kind],
    lang: "ar",
    org,
    body: `${watermark}${meta}${framed}${cc}`,
    extraCss: LETTER_CSS,
    footer: [org.address || org.name_ar, refNo ? `رقم الصادر ${refNo}` : "مسودة — غير صادرة"],
  });
}

function blankDraft(kind: LetterKind, org: Organization | null, signatory: string): LetterDraft {
  const tpl = TEMPLATES[kind];
  return {
    kind,
    recipient_name: tpl.recipient ?? "",
    recipient_title: kind === "circular" ? null : "سعادة",
    recipient_org: null,
    subject: fillTemplate(tpl.subject, org),
    body: fillTemplate(tpl.body, org),
    closing: tpl.closing,
    signatory_name: signatory,
    signatory_title: "رئيس مجلس الإدارة",
    attachments: null,
    cc: null,
    letter_date: today(),
  };
}

export default function Letters() {
  const { t } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const [tab, setTab] = useState<"all" | "draft" | "issued">("all");
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<Letter | "new" | null>(null);
  const [org, setOrg] = useState<Organization | null>(null);
  const { data, error, reload } = useLoad(() => api.get<Letter[]>("/letters", { status: tab === "all" ? undefined : tab, q, limit: 500 }), [tab, q]);

  useEffect(() => {
    api.get<Organization>("/settings/organization").then(setOrg).catch(() => undefined);
  }, []);

  const printRegister = () => {
    if (!org) return;
    const issued = (data ?? []).filter((l) => l.status === "issued").slice().reverse();
    const rows = issued
      .map(
        (l) =>
          `<tr><td>${esc(l.ref_no)}</td><td>${esc(hijriText(l.hijri_date, "ar"))}</td><td>${esc(t(`letterKinds.${l.kind}`))}</td><td>${esc(l.subject)}</td><td>${esc(
            [l.recipient_name, l.recipient_org].filter(Boolean).join(" — "),
          )}</td><td>${esc(l.signatory_name)}</td></tr>`,
      )
      .join("");
    const body = `<h1 class="title">${esc(t("letters.register"))}</h1><div class="subtitle">${esc(t("reports.asOfLine", { date: gregorian(today(), "ar") }))}</div>
<table class="t"><thead><tr><th>${esc(t("letters.refNo"))}</th><th>${esc(t("common.date"))}</th><th>${esc(t("letters.kind"))}</th><th>${esc(t("letters.subject"))}</th><th>${esc(t("letters.recipient"))}</th><th>${esc(t("letters.signatory"))}</th></tr></thead><tbody>${rows}</tbody></table>`;
    printHtml(paperHtml({ title: t("letters.register"), lang: "ar", org, body: issued.length ? body : `${body}<div class="empty">${esc(t("common.noData"))}</div>`, footer: [org.name_ar, t("letters.register")] }));
  };

  if (editing && org) {
    return (
      <Composer
        org={org}
        letter={editing === "new" ? null : editing}
        onBack={() => {
          setEditing(null);
          reload();
        }}
      />
    );
  }

  return (
    <div className="page">
      <PageHeader title={t("letters.title")}>
        <button type="button" className="btn btn-secondary" onClick={printRegister} disabled={!org}>
          <BookMarked size={16} /> {t("letters.printRegister")}
        </button>
        {can("letters.write") && (
          <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setEditing("new")} disabled={!org}>
            <Plus size={16} /> {t("letters.new")}
          </button>
        )}
      </PageHeader>
      <Tabs
        active={tab}
        onChange={setTab}
        tabs={[
          { key: "all", label: t("common.all") },
          { key: "draft", label: t("letters.drafts") },
          { key: "issued", label: t("letters.issued") },
        ]}
      />
      <div className="toolbar">
        <div className="search-box">
          <Search size={16} />
          <input type="search" placeholder={t("letters.searchHint")} value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
      </div>
      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        searchable={false}
        rows={data ?? []}
        rowKey={(l) => l.id}
        onRowClick={setEditing}
        exportName="letters"
        empty={t("letters.empty")}
        columns={[
          { header: t("letters.refNo"), cell: (l) => l.ref_no ?? "—" },
          { header: t("letters.kind"), cell: (l) => t(`letterKinds.${l.kind}`) },
          { header: t("letters.subject"), cell: (l) => l.subject },
          { header: t("letters.recipient"), cell: (l) => l.recipient_name },
          { header: t("common.date"), cell: (l) => `${l.letter_date} · ${hijriText(l.hijri_date, "ar")}` },
          {
            header: t("common.status"),
            text: (l) => t(`letters.status.${l.status}`),
            cell: (l) => <Badge tone={l.status === "issued" ? "success" : "warning"}>{t(`letters.status.${l.status}`)}</Badge>,
          },
        ]}
      />
    </div>
  );
}

function Composer({ org, letter, onBack }: { org: Organization; letter: Letter | null; onBack: () => void }) {
  const { t, i18n } = useTranslation();
  const can = useAuthStore((s) => s.can);
  const user = useAuthStore((s) => s.session?.fullName ?? "");
  const [saved, setSaved] = useState<Letter | null>(letter);
  const [draft, setDraft] = useState<LetterDraft>(() => (letter ? { ...letter } : blankDraft("congratulation", org, user)));
  const [archiveCopy, setArchiveCopy] = useState(true);
  const [confirmIssue, setConfirmIssue] = useState(false);
  const { busy, run } = useSubmit();
  const readOnly = saved?.status === "issued" || !can("letters.write");
  const html = useMemo(() => letterHtml(draft, saved?.ref_no ?? null, saved?.ref_no ? saved.hijri_date : null, org), [draft, saved, org]);
  const set = (patch: Partial<LetterDraft>) => setDraft((d) => ({ ...d, ...patch }));
  const opt = (v: string) => v || null;

  const pickKind = (kind: LetterKind) => {
    // Untouched template text follows the kind; anything the writer typed is kept.
    const before = TEMPLATES[draft.kind];
    const after = TEMPLATES[kind];
    setDraft((d) => ({
      ...d,
      kind,
      subject: d.subject === fillTemplate(before.subject, org) ? fillTemplate(after.subject, org) : d.subject,
      body: d.body === fillTemplate(before.body, org) ? fillTemplate(after.body, org) : d.body,
      closing: d.closing === before.closing ? after.closing : d.closing,
      recipient_name: d.recipient_name === (before.recipient ?? "") ? (after.recipient ?? "") : d.recipient_name,
    }));
  };

  const persist = async (): Promise<Letter> => {
    const l = saved ? await api.put<Letter>(`/letters/${saved.id}`, draft) : await api.post<Letter>("/letters", draft);
    setSaved(l);
    return l;
  };

  const save = () =>
    run(async () => {
      await persist();
      toastSuccess(t("letters.savedDraft"));
    });

  const issue = () =>
    run(async () => {
      const current = await persist();
      let issued = await api.post<Letter>(`/letters/${current.id}/issue`, {});
      if (archiveCopy) {
        try {
          const doc = await api.post<ArchiveDocument>("/archive/documents", {
            title: issued.subject,
            category: "outgoing",
            reference_no: issued.ref_no,
            document_date: issued.letter_date,
            party: [issued.recipient_name, issued.recipient_org].filter(Boolean).join(" — "),
            description: t(`letterKinds.${issued.kind}`),
            tags: [t(`letterKinds.${issued.kind}`)],
          });
          const file = new File([letterHtml(issued, issued.ref_no, issued.hijri_date, org)], `${issued.ref_no?.replace("/", "-")}.html`, { type: "text/html" });
          await uploadFile(`/archive/documents/${doc.id}/files`, file, () => undefined);
          issued = await api.post<Letter>(`/letters/${issued.id}/archive`, { document_id: doc.id });
        } catch (e) {
          toastError(`${t("letters.archiveFailed")}: ${errorMessage(e)}`);
        }
      }
      setSaved(issued);
      setDraft({ ...issued });
      toastSuccess(t("letters.issuedOk", { no: issued.ref_no }));
    });

  const fileName = (saved?.ref_no ?? draft.subject ?? "letter").replace(/[\\/]/g, "-");

  return (
    <div className="page composer">
      <PageHeader title={saved?.ref_no ? `${t(`letterKinds.${draft.kind}`)} · ${saved.ref_no}` : t("letters.compose")}>
        <button type="button" className="btn btn-secondary" onClick={onBack}>
          <ArrowRight size={16} style={i18n.dir() === "rtl" ? undefined : { transform: "scaleX(-1)" }} /> {t("letters.back")}
        </button>
        <button type="button" className="btn btn-secondary" data-shortcut="print" onClick={() => printHtml(html)}>
          <Printer size={16} /> {t("reports.print")}
        </button>
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => {
            toastInfo(t("reports.pdfHint"));
            printHtml(html);
          }}
        >
          <FileDown size={16} /> PDF
        </button>
        <button type="button" className="btn btn-secondary" onClick={() => downloadWord(html, fileName)}>
          <FileText size={16} /> Word
        </button>
      </PageHeader>

      <div className="composer-layout">
        <div className="composer-form">
          {readOnly ? (
            <div className="settings-note issued-note">
              <Lock size={15} /> {saved?.status === "issued" ? t("letters.issuedNote", { no: saved.ref_no, by: actorName(saved.issued_by) }) : t("letters.readOnly")}
            </div>
          ) : (
            <div className="kind-picker" role="radiogroup" aria-label={t("letters.kind")}>
              {LETTER_KINDS.map((k) => (
                <button key={k} type="button" role="radio" aria-checked={draft.kind === k} className={`kind-card kind-${k}${draft.kind === k ? " active" : ""}`} onClick={() => pickKind(k)}>
                  {t(`letterKinds.${k}`)}
                </button>
              ))}
            </div>
          )}
          <fieldset disabled={readOnly}>
            <div className="form-row">
              <Field label={t("letters.recipientTitle")}>
                <input list="honorifics" value={draft.recipient_title ?? ""} onChange={(e) => set({ recipient_title: opt(e.target.value) })} />
                <datalist id="honorifics">
                  {["سعادة", "معالي", "فضيلة الشيخ", "الأستاذ", "الأستاذة", "الدكتور", "صاحب السمو"].map((h) => (
                    <option key={h} value={h} />
                  ))}
                </datalist>
              </Field>
              <Field label={t("letters.recipient")}>
                <input required value={draft.recipient_name} onChange={(e) => set({ recipient_name: e.target.value })} />
              </Field>
            </div>
            <div className="form-row">
              <Field label={t("letters.recipientOrg")}>
                <input value={draft.recipient_org ?? ""} onChange={(e) => set({ recipient_org: opt(e.target.value) })} />
              </Field>
              <Field label={t("common.date")} hint={hijri(draft.letter_date, "ar")}>
                <input type="date" required value={draft.letter_date} onChange={(e) => e.target.value && set({ letter_date: e.target.value })} />
              </Field>
            </div>
            <Field label={t("letters.subject")}>
              <input required value={draft.subject} onChange={(e) => set({ subject: e.target.value })} />
            </Field>
            <Field label={t("letters.body")} hint={t("letters.bodyHint")}>
              <textarea rows={9} required value={draft.body} onChange={(e) => set({ body: e.target.value })} />
            </Field>
            <Field label={t("letters.closing")}>
              <input value={draft.closing ?? ""} onChange={(e) => set({ closing: opt(e.target.value) })} />
            </Field>
            <div className="form-row">
              <Field label={t("letters.signatoryTitle")}>
                <input value={draft.signatory_title ?? ""} onChange={(e) => set({ signatory_title: opt(e.target.value) })} />
              </Field>
              <Field label={t("letters.signatory")}>
                <input required value={draft.signatory_name} onChange={(e) => set({ signatory_name: e.target.value })} />
              </Field>
            </div>
            <div className="form-row">
              <Field label={t("letters.attachments")}>
                <input value={draft.attachments ?? ""} onChange={(e) => set({ attachments: opt(e.target.value) })} />
              </Field>
              <Field label={t("letters.cc")} hint={t("letters.ccHint")}>
                <textarea rows={2} value={draft.cc ?? ""} onChange={(e) => set({ cc: opt(e.target.value) })} />
              </Field>
            </div>
          </fieldset>
          {!readOnly && (
            <>
              <label className="checkbox-label">
                <input type="checkbox" checked={archiveCopy} onChange={(e) => setArchiveCopy(e.target.checked)} />
                {t("letters.archiveCopy")}
              </label>
              <div className="form-actions">
                <button type="button" className="btn btn-secondary" data-shortcut="save" disabled={busy} onClick={save}>
                  <Save size={16} /> {t("letters.saveDraft")}
                </button>
                <button
                  type="button"
                  className="btn btn-primary"
                  hidden={!can("letters.issue")}
                  disabled={busy || !draft.recipient_name.trim() || !draft.subject.trim() || !draft.body.trim() || !draft.signatory_name.trim()}
                  onClick={() => setConfirmIssue(true)}
                >
                  <Send size={16} /> {t("letters.issue")}
                </button>
              </div>
            </>
          )}
        </div>
        <div className="paper-stage">{html ? <PaperFrame title={t("letters.preview")} html={html} /> : <Loading />}</div>
      </div>

      <ConfirmModal
        isOpen={confirmIssue}
        title={t("letters.issue")}
        message={t("letters.issueConfirm")}
        confirmText={t("letters.issue")}
        cancelText={t("common.cancel")}
        variant="primary"
        onClose={() => setConfirmIssue(false)}
        onConfirm={issue}
      />
    </div>
  );
}
