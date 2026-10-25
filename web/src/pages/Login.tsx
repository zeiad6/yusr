import { useEffect, useState, type FormEvent } from "react";
import { APP_VERSION } from "../components/About";
import { Navigate, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { ArrowRight, Award, Check, Copy, Database, KeyRound, Lock, ShieldCheck, Sparkles, User } from "lucide-react";
import { api, errorMessage } from "../api/client";
import type { LoginResponse } from "../api/types";
import { useAuthStore } from "../store/auth";

const GOOGLE_CLIENT_ID = "07463803009-lfi5hgk8uth9h0liiof1t6da8md81pbb.apps.googleusercontent.com";

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
  const defaultUser = "admin";
  const defaultPass = firstRun || "admin12345";
  const [username, setUsername] = useState(defaultUser);
  const [password, setPassword] = useState(defaultPass);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [copiedUser, setCopiedUser] = useState(false);
  const [copiedPass, setCopiedPass] = useState(false);
  const isRtl = i18n.dir() === "rtl";

  useEffect(() => {
    // Check if redirected back from Google OAuth implicit flow
    const hash = window.location.hash;
    if (hash.includes("access_token=")) {
      const params = new URLSearchParams(hash.replace(/^#/, ""));
      const accessToken = params.get("access_token");
      if (accessToken) {
        window.history.replaceState(null, "", window.location.pathname);
        setLoading(true);
        fetch("https://www.googleapis.com/oauth2/v3/userinfo", {
          headers: { Authorization: `Bearer ${accessToken}` },
        })
          .then((res) => res.json())
          .then(async (profile) => {
            if (profile.email) {
              const r = await api.post<LoginResponse>("/auth/google", {
                email: profile.email,
                name: profile.name || profile.email,
                sub: profile.sub,
              });
              login({
                token: r.token,
                username: profile.email.split("@")[0].toLowerCase(),
                fullName: r.full_name || profile.name,
                role: r.role,
                expiresAt: r.expires_at,
                permissions: r.permissions,
              });
              navigate("/");
            }
          })
          .catch((err) => {
            setError(errorMessage(err));
          })
          .finally(() => {
            setLoading(false);
          });
      }
    }
  }, [navigate, login]);

  if (session) return <Navigate to="/" replace />;

  const copyField = (val: string, field: "user" | "pass") => {
    if (navigator?.clipboard?.writeText) {
      navigator.clipboard.writeText(val);
    }
    if (field === "user") {
      setCopiedUser(true);
      setTimeout(() => setCopiedUser(false), 2000);
    } else {
      setCopiedPass(true);
      setTimeout(() => setCopiedPass(false), 2000);
    }
  };

  const autofillCredentials = () => {
    setUsername(defaultUser);
    setPassword(defaultPass);
  };

  const handleGoogleSignIn = () => {
    if (!navigator.onLine) {
      setError(t("auth.googleOfflineNotice"));
      return;
    }
    const rootUrl = "https://accounts.google.com/o/oauth2/v2/auth";
    const options = {
      redirect_uri: window.location.origin + "/login",
      client_id: GOOGLE_CLIENT_ID,
      response_type: "token",
      scope: "openid email profile",
      prompt: "select_account",
    };
    const qs = new URLSearchParams(options).toString();
    window.location.assign(`${rootUrl}?${qs}`);
  };

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

            <div className="login-release-badges">
              <div className="release-badge-item">
                <ShieldCheck size={14} />
                <span>{t("auth.releaseTag")}</span>
              </div>
              <div className="release-badge-item">
                <Database size={14} />
                <span>{t("auth.engineSpecs")}</span>
              </div>
              <div className="release-badge-item">
                <Award size={14} />
                <span>{t("auth.specs")}</span>
              </div>
            </div>
          </div>
          <div className="login-aside-foot">
            <div className="version-row">
              <span className="version">v{APP_VERSION}</span>
              <span className="release-type">Standalone MSI</span>
            </div>
            <div className="legal-note">PolyForm Noncommercial 1.0.0 · Patent Defense</div>
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

          {/* Default Credentials Panel with Copy Buttons & Auto-fill */}
          <div className="login-creds-card t-glass-card" role="region" aria-label={t("auth.defaultCredentials")}>
            <div className="login-creds-header">
              <div className="login-creds-title">
                <KeyRound size={16} className="login-creds-icon" />
                <span>{t("auth.defaultCredentials")}</span>
              </div>
              <button
                type="button"
                onClick={autofillCredentials}
                className="btn-text-action"
                title={t("auth.autofill")}
              >
                <Sparkles size={13} />
                <span>{t("auth.autofill")}</span>
              </button>
            </div>

            <div className="login-creds-grid">
              <div className="login-cred-row">
                <span className="cred-label">{t("auth.defaultUsername")}:</span>
                <code className="cred-val" dir="ltr">{defaultUser}</code>
                <button
                  type="button"
                  onClick={() => copyField(defaultUser, "user")}
                  className={`btn-cred-copy ${copiedUser ? "copied" : ""}`}
                  aria-label={`${t("auth.copy")} ${t("auth.defaultUsername")}`}
                >
                  {copiedUser ? <Check size={13} /> : <Copy size={13} />}
                  <span>{copiedUser ? t("auth.copied") : t("auth.copy")}</span>
                </button>
              </div>

              <div className="login-cred-row">
                <span className="cred-label">{t("auth.defaultPassword")}:</span>
                <code className="cred-val" dir="ltr">{defaultPass}</code>
                <button
                  type="button"
                  onClick={() => copyField(defaultPass, "pass")}
                  className={`btn-cred-copy ${copiedPass ? "copied" : ""}`}
                  aria-label={`${t("auth.copy")} ${t("auth.defaultPassword")}`}
                >
                  {copiedPass ? <Check size={13} /> : <Copy size={13} />}
                  <span>{copiedPass ? t("auth.copied") : t("auth.copy")}</span>
                </button>
              </div>
            </div>
          </div>

          <form onSubmit={submit} className="login-form">
            {firstRun && (
              <div className="first-run" role="status">
                <strong>{t("auth.firstRunTitle")}</strong>
                <span>{t("auth.firstRunText")}</span>
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

            <div className="login-divider">
              <span>{t("auth.orContinueWith")}</span>
            </div>

            <button
              type="button"
              onClick={handleGoogleSignIn}
              className="btn btn-google"
              disabled={loading}
            >
              <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden>
                <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
                <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
                <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22.81-.63z" />
                <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z" />
              </svg>
              <span>{t("auth.googleSignIn")}</span>
            </button>
          </form>

          <div className="login-footer-meta">
            <span>{t("auth.versionDetails")}: v{APP_VERSION}</span>
            <span>100% Offline · ZATCA Phase 2</span>
          </div>
        </div>
      </div>
    </div>
  );
}
