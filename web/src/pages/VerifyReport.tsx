import { useEffect, useState, type FormEvent } from "react";
import { useSearchParams } from "react-router-dom";
import { ShieldCheck, ShieldAlert, Search } from "lucide-react";
import { api, errorMessage } from "../api/client";
import { Field, PageHeader } from "../components/ui";

interface VerificationResponse {
  valid: boolean;
  status: string;
  report_number?: string;
  title?: string;
  audit_head_hash?: string;
  published_at?: string;
  audit_sequence?: number;
  total_donations?: string;
  total_aid?: string;
  active_cases?: number;
}

export default function VerifyReport() {
  const [params] = useSearchParams();
  const [tokenInput, setTokenInput] = useState(params.get("token") ?? "");
  const [verifying, setVerifying] = useState(false);
  const [result, setResult] = useState<VerificationResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const performVerify = async (val: string) => {
    if (!val.trim()) return;
    setVerifying(true);
    setError(null);
    try {
      const res = await api.post<VerificationResponse>("/reports/verify", {
        token: val.trim().length <= 32 ? val.trim() : undefined,
        audit_head_hash: val.trim().length === 64 ? val.trim() : undefined,
      });
      setResult(res);
    } catch (e) {
      setError(errorMessage(e));
      setResult(null);
    } finally {
      setVerifying(false);
    }
  };

  useEffect(() => {
    const qToken = params.get("token");
    if (qToken) {
      performVerify(qToken);
    }
  }, [params]);

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    performVerify(tokenInput);
  };

  return (
    <div className="page" style={{ maxWidth: "800px", margin: "0 auto", padding: "24px" }}>
      <PageHeader title="بوابة التحقق الجنائي من تقارير الشفافية (Cryptographic Verification)" />

      <p className="settings-note">
        تتيح هذه البوابة للمتبرعين والمدققين التحقق الفوري من صحة وموثوقية وثائق الشفافية المالية الصادرة عن الجمعية
        ومطابقتها مع بصمة سلسلة التدقيق الجنائي غير القابلة للتعديل (SHA-256 Cryptographic Hash Chain).
      </p>

      <form className="chart-card" onSubmit={handleSubmit} style={{ marginBottom: "24px" }}>
        <div style={{ display: "flex", gap: "10px", alignItems: "flex-end" }}>
          <div style={{ flex: 1 }}>
            <Field label="رمز التحقق العام (Verification Token) أو تجزئة الكتلة (Block Hash)">
              <input
                type="text"
                required
                placeholder="مثال: e4b29f018a... أو TR-202610-e4b29f"
                value={tokenInput}
                onChange={(e) => setTokenInput(e.target.value)}
                style={{ fontFamily: "monospace", fontSize: "14px" }}
              />
            </Field>
          </div>
          <button type="submit" className="btn btn-primary" disabled={verifying} style={{ height: "40px" }}>
            <Search size={16} /> {verifying ? "جارٍ التحقق…" : "فحص وتوثيق"}
          </button>
        </div>
      </form>

      {error && (
        <div className="chart-card" style={{ borderColor: "var(--danger)", background: "rgba(239, 68, 68, 0.05)" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "10px", color: "var(--danger)" }}>
            <ShieldAlert size={24} />
            <strong>{error}</strong>
          </div>
        </div>
      )}

      {result && (
        <div
          className="chart-card"
          style={{
            borderColor: result.valid ? "var(--success)" : "var(--danger)",
            background: result.valid ? "rgba(16, 185, 129, 0.04)" : "rgba(239, 68, 68, 0.04)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "16px" }}>
            {result.valid ? (
              <ShieldCheck size={32} style={{ color: "var(--success)" }} />
            ) : (
              <ShieldAlert size={32} style={{ color: "var(--danger)" }} />
            )}
            <div>
              <h3 style={{ margin: 0, fontSize: "18px" }}>
                {result.valid ? "وثيقة رسمية مصدقة ومطابقة لسلسلة التدقيق" : "فشل التحقق: البصمة غير مطابقة"}
              </h3>
              <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: "13px" }}>{result.status}</p>
            </div>
          </div>

          {result.valid && (
            <div style={{ display: "grid", gridTemplateColumns: "1fr", gap: "12px" }}>
              {result.report_number && (
                <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--border)", paddingBottom: "8px" }}>
                  <span style={{ color: "var(--text-secondary)" }}>رقم التقرير الرسمي:</span>
                  <strong>{result.report_number}</strong>
                </div>
              )}
              {result.title && (
                <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid var(--border)", paddingBottom: "8px" }}>
                  <span style={{ color: "var(--text-secondary)" }}>عنوان الوثيقة:</span>
                  <strong>{result.title}</strong>
                </div>
              )}
              {result.audit_head_hash && (
                <div style={{ borderBottom: "1px solid var(--border)", paddingBottom: "8px" }}>
                  <span style={{ display: "block", color: "var(--text-secondary)", marginBottom: "4px" }}>
                    بصمة التدقيق الجنائي للكتلة (SHA-256 Ledger Anchor):
                  </span>
                  <code style={{ fontSize: "12px", background: "var(--surface-sunken)", padding: "4px 8px", borderRadius: "4px", display: "block", wordBreak: "break-all" }}>
                    {result.audit_head_hash}
                  </code>
                </div>
              )}
              {result.published_at && (
                <div style={{ display: "flex", justifyContent: "space-between" }}>
                  <span style={{ color: "var(--text-secondary)" }}>تاريخ النشر والختم الرقمي:</span>
                  <span>{new Date(result.published_at).toLocaleString()}</span>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
