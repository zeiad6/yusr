import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Pencil } from "lucide-react";
import { api } from "../api/client";
import Modal from "./Modal";
import { toastSuccess } from "./Toast";
import { Field, useSubmit } from "./ui";
import { FormActions } from "./forms";

export type FieldSpec = {
  key: string;
  label: string;
  type?: "text" | "textarea" | "date" | "number" | "select" | "checkbox" | "tags" | "email" | "tel";
  options?: { value: string; label: string }[];
  required?: boolean;
  dir?: "ltr" | "rtl";
  hint?: string;
  /** Number inputs: smallest step, e.g. "0.01" for money. */
  step?: string;
};

type Values = Record<string, unknown>;

/**
 * One form for correcting any registry record. Empty text becomes `null`, tags are a
 * comma-separated list, and the whole object is sent with PUT (or PATCH) so the server
 * validates and audits it as one change.
 */
export default function EditRecord({
  title,
  path,
  method = "put",
  fields,
  initial,
  note,
  onClose,
  onSaved,
}: {
  title: string;
  path: string;
  method?: "put" | "patch";
  fields: FieldSpec[];
  initial: object;
  /** Shown above the form, e.g. what cannot be changed here and why. */
  note?: string;
  onClose: () => void;
  onSaved: (result: unknown) => void;
}) {
  const { t } = useTranslation();
  const [values, setValues] = useState<Values>(() => ({ ...(initial as Values) }));
  const { busy, run } = useSubmit();
  const set = (k: string, v: unknown) => setValues((cur) => ({ ...cur, [k]: v }));

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      const body: Values = { ...values };
      for (const f of fields) {
        const v = body[f.key];
        if (f.type === "tags") body[f.key] = String(v ?? "").split(/[,،]/).map((s) => s.trim()).filter(Boolean);
        else if (typeof v === "string" && f.type !== "select" && !f.required) body[f.key] = v.trim() || null;
      }
      const r = method === "put" ? await api.put(path, body) : await api.patch(path, body);
      toastSuccess(t("common.saved"));
      onSaved(r);
      onClose();
    });
  };

  const input = (f: FieldSpec) => {
    const v = values[f.key];
    switch (f.type) {
      case "textarea":
        return <textarea rows={3} required={f.required} value={String(v ?? "")} onChange={(e) => set(f.key, e.target.value)} />;
      case "select":
        return (
          <select required={f.required} value={String(v ?? "")} onChange={(e) => set(f.key, e.target.value || null)}>
            {!f.required && <option value="">—</option>}
            {f.options?.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        );
      case "checkbox":
        return (
          <label className="checkbox-label">
            <input type="checkbox" checked={Boolean(v)} onChange={(e) => set(f.key, e.target.checked)} />
            {f.label}
          </label>
        );
      case "tags":
        return <input value={Array.isArray(v) ? v.join("، ") : String(v ?? "")} onChange={(e) => set(f.key, e.target.value)} />;
      default:
        return (
          <input
            type={f.type ?? "text"}
            dir={f.dir}
            step={f.step}
            required={f.required}
            value={String(v ?? "")}
            onChange={(e) => set(f.key, e.target.value)}
          />
        );
    }
  };

  return (
    <Modal isOpen onClose={onClose} title={title}>
      <form onSubmit={submit} className="edit-record">
        {note && <p className="settings-note">{note}</p>}
        <div className="edit-grid">
          {fields.map((f) =>
            f.type === "checkbox" ? (
              <div key={f.key} className="form-group">
                {input(f)}
              </div>
            ) : (
              <Field key={f.key} label={f.label} hint={f.hint}>
                {input(f)}
              </Field>
            ),
          )}
        </div>
        <FormActions busy={busy} onClose={onClose} />
      </form>
    </Modal>
  );
}

export function EditButton({ onClick, label }: { onClick: () => void; label?: string }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      className="btn btn-sm btn-ghost"
      title={label ?? t("common.edit")}
      aria-label={label ?? t("common.edit")}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      <Pencil size={14} /> {label ?? t("common.edit")}
    </button>
  );
}
