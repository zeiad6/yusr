import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { AlertOctagon, AlertTriangle, Bot, Info, Send, Sparkles, Square, X } from "lucide-react";
import { api, errorMessage } from "../api/client";
import { useAuthStore } from "../store/auth";

type Severity = "critical" | "warning" | "info";

interface Insight {
  code: string;
  severity: Severity;
  params: Record<string, unknown>;
  link: string;
}

interface Message {
  role: "user" | "assistant";
  content: string;
}

/** Insights are re-read this often while the app is open. */
const REFRESH_MS = 5 * 60 * 1000;
const SUGGESTIONS = ["q1", "q2", "q3", "q4"] as const;

const ICON = { critical: AlertOctagon, warning: AlertTriangle, info: Info };

/** Models that reason out loud wrap it in <think>; only the answer is shown. */
export function visibleAnswer(text: string): string {
  return text.replace(/<think>[\s\S]*?(<\/think>|$)/g, "").trimStart();
}

function insightText(t: (k: string, o?: Record<string, unknown>) => string, i: Insight) {
  const p = Object.fromEntries(Object.entries(i.params).map(([k, v]) => [k, v ?? "—"]));
  return t(`advisor.insights.${i.code}`, p);
}

/**
 * The advisor's floating button and panel. It reads and advises only: the server gives the
 * model a summary of what this user may already see, and nothing it says can run an action.
 */
export default function AdvisorWidget() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const can = useAuthStore((s) => s.can);
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<"insights" | "chat">("insights");
  const [insights, setInsights] = useState<Insight[] | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [question, setQuestion] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [chatError, setChatError] = useState<string | null>(null);
  const abort = useRef<AbortController | null>(null);
  const end = useRef<HTMLDivElement>(null);
  const allowed = can("advisor.use");

  const refresh = useCallback(() => {
    api.get<Insight[]>("/advisor/insights").then(setInsights, () => setInsights([]));
  }, []);

  useEffect(() => {
    if (!allowed) return;
    refresh();
    const id = setInterval(refresh, REFRESH_MS);
    return () => clearInterval(id);
  }, [allowed, refresh]);

  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [messages]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!allowed) return null;
  const urgent = (insights ?? []).filter((i) => i.severity !== "info").length;

  const ask = async (text: string) => {
    const q = text.trim();
    if (!q || streaming) return;
    const history: Message[] = [...messages, { role: "user", content: q }];
    setMessages([...history, { role: "assistant", content: "" }]);
    setQuestion("");
    setChatError(null);
    setStreaming(true);
    const controller = new AbortController();
    abort.current = controller;
    try {
      const res = await api.raw("POST", "/advisor/chat", { messages: history });
      const reader = res.body?.getReader();
      if (!reader) throw new Error("no stream");
      const decoder = new TextDecoder();
      let answer = "";
      controller.signal.addEventListener("abort", () => reader.cancel().catch(() => undefined));
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        answer += decoder.decode(value, { stream: true });
        setMessages([...history, { role: "assistant", content: answer }]);
      }
      if (!visibleAnswer(answer).trim()) setChatError(t("advisor.emptyAnswer"));
    } catch (e) {
      if (!controller.signal.aborted) {
        setChatError(errorMessage(e));
        setMessages(history);
      }
    } finally {
      setStreaming(false);
      abort.current = null;
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    ask(question);
  };

  return (
    <>
      <button
        type="button"
        className={`advisor-fab${urgent ? " has-alerts" : ""}`}
        onClick={() => {
          if (!open) refresh();
          setOpen(!open);
        }}
        aria-expanded={open}
        aria-label={t("advisor.title")}
        title={t("advisor.title")}
      >
        <Sparkles size={22} />
        {urgent > 0 && <span className="advisor-badge">{urgent}</span>}
      </button>

      {open && (
        <section className="advisor-panel" role="dialog" aria-label={t("advisor.title")}>
          <header className="advisor-head">
            <span className="advisor-avatar">
              <Bot size={18} />
            </span>
            <div>
              <strong>{t("advisor.title")}</strong>
              <small>{t("advisor.readOnly")}</small>
            </div>
            <button type="button" className="btn-icon" onClick={() => setOpen(false)} aria-label={t("common.close")}>
              <X size={18} />
            </button>
          </header>

          <div className="advisor-tabs" role="tablist">
            <button type="button" role="tab" aria-selected={tab === "insights"} className={tab === "insights" ? "active" : ""} onClick={() => (setTab("insights"), refresh())}>
              {t("advisor.insightsTab")} {insights && insights.length > 0 && <span className="count">{insights.length}</span>}
            </button>
            <button type="button" role="tab" aria-selected={tab === "chat"} className={tab === "chat" ? "active" : ""} onClick={() => setTab("chat")}>
              {t("advisor.chatTab")}
            </button>
          </div>

          {tab === "insights" ? (
            <div className="advisor-body">
              {insights === null && <p className="advisor-empty">{t("common.loading")}</p>}
              {insights?.length === 0 && <p className="advisor-empty">{t("advisor.allClear")}</p>}
              <ul className="insight-list">
                {insights?.map((i, n) => {
                  const Icon = ICON[i.severity];
                  return (
                    <li key={`${i.code}-${n}`} className={`insight insight-${i.severity}`}>
                      <Icon size={17} />
                      <div>
                        <p>{insightText(t, i)}</p>
                        <button
                          type="button"
                          className="link-btn"
                          onClick={() => {
                            navigate(i.link);
                            setOpen(false);
                          }}
                        >
                          {t("advisor.open")}
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            </div>
          ) : (
            <div className="advisor-chat">
              <div className="advisor-body chat-log" aria-live="polite">
                {messages.length === 0 && (
                  <div className="advisor-welcome">
                    <p>{t("advisor.welcome")}</p>
                    <div className="suggestions">
                      {SUGGESTIONS.map((k) => (
                        <button key={k} type="button" className="chip" onClick={() => ask(t(`advisor.suggestions.${k}`))}>
                          {t(`advisor.suggestions.${k}`)}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
                {messages.map((m, i) => (
                  <div key={i} className={`bubble bubble-${m.role}`}>
                    {m.role === "assistant" ? visibleAnswer(m.content) || (streaming ? <span className="typing" aria-label={t("advisor.thinking")} /> : "") : m.content}
                  </div>
                ))}
                {chatError && <p className="advisor-error">{chatError}</p>}
                <div ref={end} />
              </div>
              <form className="advisor-input" onSubmit={submit}>
                <textarea
                  rows={2}
                  value={question}
                  placeholder={t("advisor.placeholder")}
                  onChange={(e) => setQuestion(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      ask(question);
                    }
                  }}
                />
                {streaming ? (
                  <button type="button" className="btn-icon" onClick={() => abort.current?.abort()} aria-label={t("advisor.stop")}>
                    <Square size={18} />
                  </button>
                ) : (
                  <button type="submit" className="btn-icon send" disabled={!question.trim()} aria-label={t("advisor.send")}>
                    <Send size={18} />
                  </button>
                )}
              </form>
              <small className="advisor-disclaimer">{t("advisor.disclaimer")}</small>
            </div>
          )}
        </section>
      )}
    </>
  );
}
