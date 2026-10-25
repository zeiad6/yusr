import { useTranslation } from "react-i18next";
import { localName, useRefs } from "../store/refs";

export function FundSelect({ value, onChange, required = true }: { value: string; onChange: (id: string) => void; required?: boolean }) {
  const { i18n } = useTranslation();
  const funds = useRefs((s) => s.funds);
  return (
    <select required={required} value={value} onChange={(e) => onChange(e.target.value)}>
      {!required && <option value="">—</option>}
      {funds.map((f) => (
        <option key={f.id} value={f.id}>
          {f.code} · {localName(f, i18n.language)}
        </option>
      ))}
    </select>
  );
}

export function FormActions({ busy, onClose, label }: { busy: boolean; onClose: () => void; label?: string }) {
  const { t } = useTranslation();
  return (
    <div className="form-actions">
      <button type="submit" className="btn btn-primary" disabled={busy} data-shortcut="save">
        {label ?? t("common.save")}
      </button>
      <button type="button" className="btn btn-secondary" onClick={onClose}>
        {t("common.cancel")}
      </button>
    </div>
  );
}
