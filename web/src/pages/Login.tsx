import { useState, type FormEvent } from "react";
import { APP_VERSION } from "../components/About";
import { Navigate, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { ArrowRight, Lock, User } from "lucide-react";
import { api, errorMessage } from "../api/client";
import type { LoginResponse } from "../api/types";
import { useAuthStore } from "../store/auth";

/** The desktop app opens `/login#first-run=<password>` once, after creating the administrator. */
function firstRunPassword(): string | null {
  const m = /^#first-run=(.+)$/.exec(window.location.hash);
  if (!m) return null;
  window.history.replaceState(null, "", window.location.pathname);
  return decodeURIComponent(m[1]);
}

export default function Login() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const { session, login } = useAuthStore();
  const [firstRun] = useState(firstRunPassword);
  const [username, setUsername] = useState(firstRun ? "admin" : "");
  const [password, setPassword] = useState(firstRun ?? "");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const isRtl = i18n.dir() === "rtl";

  if (session) return <Navigate to="/" replace />;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      const r = await api.post<LoginResponse>("/auth/login", { username, password });
      login({ token: r.token, username: username.trim().toLowerCase(), fullName: r.full_name, role: r.role, expiresAt: r.expires_at, permissions: r.permissions });
      navigate("/");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="login-page">
      <div className="login-shell">
        <aside className="login-aside">
          <div className="login-aside-top">
            <span className="brand-icon">
              <img src="/logo-mark-light.svg" alt="" width={18} height={18} aria-hidden />
            </span>
            <span className="brand-text">{t("app.short")}</span>
          </div>
          <div className="login-aside-body">
            <h2 className="login-aside-title">{t("app.name")}</h2>
            <p className="login-aside-text">{t("app.tagline")}</p>
          </div>
          <div className="login-aside-foot">
            <span className="version">v{APP_VERSION}</span>
          </div>
        </aside>

        <div className="login-card">
          <div className="login-header">
            <span className="login-logo">
              <img src="/logo-mark.svg" alt="" width={22} height={22} aria-hidden />
            </span>
            <h1>{t("app.name")}</h1>
            <p>{t("auth.subtitle")}</p>
          </div>

          <form onSubmit={submit} className="login-form">
            {firstRun && (
              <div className="first-run" role="status">
                <strong>{t("auth.firstRunTitle")}</strong>
                <span>{t("auth.firstRunText")}</span>
                <code dir="ltr">admin · {firstRun}</code>
              </div>
            )}
            {error && (
              <div className="error-message" role="alert">
                {error}
              </div>
            )}
            <div className="form-group">
              <label htmlFor="username">{t("auth.username")}</label>
              <div className="login-input">
                <span className="login-input-icon">
                  <User size={16} aria-hidden />
                </span>
                <input id="username" value={username} onChange={(e) => setUsername(e.target.value)} required autoComplete="username" autoFocus />
              </div>
            </div>
            <div className="form-group">
              <label htmlFor="password">{t("auth.password")}</label>
              <div className="login-input">
                <span className="login-input-icon">
                  <Lock size={16} aria-hidden />
                </span>
                <input id="password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} required autoComplete="current-password" />
              </div>
            </div>
            <div className="login-form-actions">
              <button type="submit" className="btn btn-primary" disabled={loading}>
                <span>{loading ? t("auth.loggingIn") : t("auth.login")}</span>
                {!loading && <ArrowRight size={16} style={isRtl ? { transform: "scaleX(-1)" } : undefined} aria-hidden />}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
