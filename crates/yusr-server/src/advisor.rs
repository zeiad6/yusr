//! The Yusr advisor: rule-based insights plus a chat backed by a local language model.
//!
//! Read-only by construction. The model receives a summary the server computes from what
//! the caller may already read, and its answer is plain text sent back to the browser; it
//! has no tools, no database access and no way to trigger an operation.
//!
//! Any OpenAI-compatible endpoint works: the bundled llama.cpp `llama-server` in the
//! desktop app, Ollama (`http://127.0.0.1:11434/v1`) or LM Studio.

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{Duration, Utc};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::AppState;
use crate::auth::{Auth, Permission};
use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdvisorSettings {
    pub enabled: bool,
    /// Base URL of an OpenAI-compatible API, ending in `/v1`.
    pub url: String,
    pub model: String,
}

impl AdvisorSettings {
    /// `YUSR_ADVISOR_URL` / `YUSR_ADVISOR_MODEL`, else a local Ollama with Qwen 3.5.
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self {
            enabled: var("YUSR_ADVISOR_ENABLED").is_none_or(|v| v != "0" && v != "false"),
            url: var("YUSR_ADVISOR_URL").unwrap_or_else(|| "http://127.0.0.1:11434/v1".into()).trim_end_matches('/').to_owned(),
            model: var("YUSR_ADVISOR_MODEL").unwrap_or_else(|| "qwen3.5:4b".into()),
        }
    }
}

/// Stored settings override the server's defaults field by field.
async fn settings(s: &AppState) -> ApiResult<AdvisorSettings> {
    let (stored,): (Value,) = sqlx::query_as("SELECT advisor FROM settings").fetch_one(&s.store.pool).await?;
    let d = &s.advisor;
    Ok(AdvisorSettings {
        enabled: stored["enabled"].as_bool().unwrap_or(d.enabled),
        url: stored["url"].as_str().filter(|u| !u.is_empty()).unwrap_or(&d.url).trim_end_matches('/').to_owned(),
        model: stored["model"].as_str().filter(|m| !m.is_empty()).unwrap_or(&d.model).to_owned(),
    })
}

fn client(timeout: std::time::Duration) -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .read_timeout(timeout)
        .build()
        .expect("HTTP client initialises")
}

fn unavailable(msg: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_GATEWAY, "advisor_unavailable", msg)
}

/// Models the endpoint serves, or why it cannot be reached.
async fn probe(cfg: &AdvisorSettings) -> Result<Vec<String>, String> {
    let res = client(std::time::Duration::from_secs(4)).get(format!("{}/models", cfg.url)).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status()));
    }
    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(body["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect())
}

pub async fn status(State(s): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require(Permission::AdvisorUse)?;
    let cfg = settings(&s).await?;
    let probe = if cfg.enabled { probe(&cfg).await } else { Err("disabled".into()) };
    let manage = a.can(Permission::AdvisorManage);
    Ok(Json(json!({
        "enabled": cfg.enabled,
        "model": cfg.model,
        "url": manage.then_some(&cfg.url),
        "reachable": probe.is_ok(),
        "models": probe.as_ref().ok(),
        "error": probe.err(),
    })))
}

pub async fn update_settings(State(s): State<AppState>, a: Auth, Json(cfg): Json<AdvisorSettings>) -> ApiResult<StatusCode> {
    a.require(Permission::AdvisorManage)?;
    if !(cfg.url.starts_with("http://") || cfg.url.starts_with("https://")) || cfg.model.trim().is_empty() {
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_settings", "url must start with http:// or https:// and a model is required"));
    }
    let value = serde_json::to_value(&cfg).expect("settings serialize");
    let actor = a.actor();
    yusr_infra::serializable!(s.store, |conn| {
        sqlx::query("UPDATE settings SET advisor = $1, updated_at = now()").bind(&value).execute(&mut *conn).await?;
        yusr_infra::audit::append(conn, &actor, "update", "settings", "advisor", value.clone()).await?;
        Ok(())
    })?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- insights -----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

/// One finding. `code` selects the translated text in the UI; `params` fill it in.
#[derive(Debug, Clone, Serialize)]
pub struct Insight {
    pub code: &'static str,
    pub severity: Severity,
    pub params: Value,
    /// The page where the user can act on it.
    pub link: &'static str,
}

fn insight(code: &'static str, severity: Severity, link: &'static str, params: Value) -> Insight {
    Insight { code, severity, params, link }
}

/// Usage at or above this share of an approved budget is flagged before it runs out.
const BUDGET_WARNING_SHARE: Decimal = Decimal::from_parts(9, 0, 0, false, 1);
const BACKUP_MAX_AGE_DAYS: i64 = 7;
const NEGLECTED_CASE_DAYS: i64 = 90;
const STALE_DRAFT_DAYS: i64 = 14;

pub async fn compute(s: &AppState, a: &Auth) -> ApiResult<Vec<Insight>> {
    let pool = &s.store.pool;
    let today = Utc::now().date_naive();
    let mut out = Vec::new();
    if a.can(Permission::AuditRead) {
        let report = yusr_infra::audit::verify(pool).await?;
        if !report.ok {
            out.push(insight("audit_chain_broken", Severity::Critical, "/audit", json!({ "error": report.error })));
        }
    }
    if a.can(Permission::ReportsRead) {
        for f in s.store.fund_balances(today).await? {
            let balance = f.net_assets + f.revenue - f.expenses;
            if balance < Decimal::ZERO {
                out.push(insight("fund_overdrawn", Severity::Critical, "/funds", json!({ "fund": f.name_ar, "fund_en": f.name_en, "balance": balance })));
            }
        }
        for b in s.store.budget_usage(today).await? {
            if b.approved <= Decimal::ZERO {
                continue;
            }
            let used = b.spent + b.committed;
            let share = used / b.approved;
            if share >= Decimal::ONE {
                out.push(insight("budget_exceeded", Severity::Critical, "/funds", json!({ "fund": b.fund_code, "project": b.project_code, "used": used, "approved": b.approved })));
            } else if share >= BUDGET_WARNING_SHARE {
                let percent = (share * Decimal::ONE_HUNDRED).round();
                out.push(insight("budget_nearly_used", Severity::Warning, "/funds", json!({ "fund": b.fund_code, "project": b.project_code, "percent": percent })));
            }
        }
        let zakat: Option<(Decimal,)> = sqlx::query_as(
            "SELECT COALESCE(SUM(l.signed_base) FILTER (WHERE a.account_type = 'revenue'), 0) * -1
                  - COALESCE(SUM(l.signed_base) FILTER (WHERE a.account_type = 'expense'), 0)
               FROM journal_lines l JOIN accounts a ON a.id = l.account_id JOIN funds f ON f.id = l.fund_id
              WHERE f.donation_kind = 'zakat'",
        )
        .fetch_optional(pool)
        .await?;
        if let Some((held,)) = zakat.filter(|(h,)| *h > Decimal::ZERO) {
            out.push(insight("zakat_undistributed", Severity::Info, "/beneficiaries", json!({ "amount": held, "currency": s.store.base_currency })));
        }
    }
    if a.can(Permission::SponsorshipsRead) {
        let behind = s.store.sponsorships(Some(yusr_infra::sponsorships::SponsorshipStatus::Active), None, 500).await?;
        let late: Vec<_> = behind.iter().filter(|sp| sp.arrears_months >= 2).collect();
        if !late.is_empty() {
            let months: i64 = late.iter().map(|sp| sp.arrears_months).sum();
            out.push(insight("sponsorship_arrears", Severity::Warning, "/sponsorships", json!({ "count": late.len(), "months": months })));
        }
    }
    if a.can(Permission::CasesRead) {
        let (neglected,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM beneficiaries b
              WHERE b.status = 'active' AND b.priority IN ('critical', 'high')
                AND NOT EXISTS (SELECT 1 FROM aid_distributions d WHERE d.beneficiary_id = b.id AND d.distributed_on >= $1)",
        )
        .bind(today - Duration::days(NEGLECTED_CASE_DAYS))
        .fetch_one(pool)
        .await?;
        if neglected > 0 {
            out.push(insight("urgent_cases_without_aid", Severity::Warning, "/beneficiaries", json!({ "count": neglected, "days": NEGLECTED_CASE_DAYS })));
        }
    }
    if a.can(Permission::BackupCreate) {
        let (last,): (Option<chrono::DateTime<Utc>>,) =
            sqlx::query_as("SELECT MAX(occurred_at) FROM audit_log WHERE action = 'backup' AND entity = 'database'").fetch_one(pool).await?;
        if last.is_none_or(|t| Utc::now() - t > Duration::days(BACKUP_MAX_AGE_DAYS)) {
            out.push(insight("backup_overdue", Severity::Warning, "/settings", json!({ "days": BACKUP_MAX_AGE_DAYS, "last": last })));
        }
    }
    if a.can(Permission::UsersManage) {
        let (locked,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE locked_until > now()").fetch_one(pool).await?;
        if locked > 0 {
            out.push(insight("accounts_locked", Severity::Info, "/users", json!({ "count": locked })));
        }
    }
    if a.can(Permission::LettersRead) {
        let (drafts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM letters WHERE status = 'draft' AND letter_date < $1")
            .bind(today - Duration::days(STALE_DRAFT_DAYS))
            .fetch_one(pool)
            .await?;
        if drafts > 0 {
            out.push(insight("stale_letter_drafts", Severity::Info, "/letters", json!({ "count": drafts, "days": STALE_DRAFT_DAYS })));
        }
    }
    out.sort_by_key(|i| match i.severity {
        Severity::Critical => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    });
    Ok(out)
}

pub async fn insights(State(s): State<AppState>, a: Auth) -> ApiResult<Json<Vec<Insight>>> {
    a.require(Permission::AdvisorUse)?;
    Ok(Json(compute(&s, &a).await?))
}

// ---- chat ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
}

const MAX_HISTORY: usize = 12;
const MAX_MESSAGE_CHARS: usize = 4000;
/// A local model serves one or two conversations at a time; more would only queue in it.
const CHAT_SLOTS: usize = 2;
static CHATS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(CHAT_SLOTS);

const SYSTEM_PROMPT: &str = "أنت «مستشار يُسر»، خبير ومستشار مالي ومحاسبي وإداري معتمد متخصص في محاسبة المنظمات غير الربحية، الجمعيات الخيرية، الأوقاف، وصناديق الزكاة داخل نظام «يُسر».

قواعدك الأساسية:
1. أنت للاطلاع والتحليل والاستشارة المهنية فقط: لا تستطيع تنفيذ أي عملية ولا تعديل قواعد البيانات مباشرة. إذا طُلب منك تسجيل قيد أو توزيع مساعدة، اشرح للمستخدم خطوات ذلك المحددة عبر شاشات النظام.
2. اعتمد حصرياً على «لقطة النظام» والبيانات المحاسبية الواردة. لا تخترع أرقاماً ولا تخلط بين الصناديق. إذا نقصت البيانات، فحدد التقرير اللازم من قائمة التقارير (/reports).
3. المعايير المحاسبية المعتمدة:
   - محاسبة الصناديق (Fund Accounting): الفصل التام بين أصول وأموال: غير المقيدة (عامة/تشغيلية)، مقيدة بأجل أو غرض (كفالات، إغاثة، بناء)، وأصول الأوقاف الدائمة (حبس الأصل وتسبيل المنفعة).
   - مبدأ القيد المزدوج: (من حـ/ المدين إلى حـ/ الدائن). القيود المرحلة قطعية ومحمية بسلسلة تجزئة SHA-256، ولا تُعدل نهائياً بل يتم تصحيحها حصراً عبر قيد عكسي كامل مع الإشارة للمرجع.
   - قيود التبرعات: استلام تبرع مقيد يُقيد (مدين: البنك/الصندوق، دائن: إيرادات تبرعات مقيدة - صندوق المشروع)، والصرف منه يكون (مدين: مصروفات المشروع، دائن: البنك/الصندوق).
4. الفقه المحاسبي للزكاة:
   - النصاب: 85 غراماً من الذهب عيار 24 (أو 595 غراماً فضة). تُحسب الزكاة بنسبة 2.5% للسنة الهجرية (أو 2.577% للميلادية).
   - وعاء الزكاة: (النقدية وما في حكمها + عروض التجارة المعدة للبيع + الديون المرجوة التحصيل) مطروحاً منها (الالتزامات والديون قصيرة الأجل المستحقة خلال الحول).
   - المصارف الشرعية الثمانية: تصرف حصراً للأصناف المحددة شرعاً. يُمنع منعاً باتاً تمويل المصاريف الإدارية العامة أو شراء أصول دائمة للجمعية من أموال الزكاة دون إذن شرعي مقنن (كحصة العاملين عليها بضوابطها).
5. إدارة المخاطر والرقابة الداخلية:
   - حذر فوراً من: صناديق مكشوفة برصيد سالب، تجاوز اعتمادات الميزانية (>90% تحذير، >=100% خطر حرج)، تأخر كفالات الأيتام لشهرين فأكثر، حالات أسر مصنفة حرجة لم يصرف لها عون منذ 90 يوماً، انقطاع أو تزوير سلسلة التدقيق المشفرة، وتأخر النسخ الاحتياطي لأكثر من 7 أيام.
6. الأسلوب: إجابات مهنية، مختصرة ومرتبة في نقاط وعناوين، مع إبراز أرقام الحسابات وروابط صفحات النظام.

خريطة مسارات النظام:
- القيود اليومية والدفاتر: (/journal)
- شجرة ودليل الحسابات: (/accounts)
- الصناديق والمشاريع والموازنات: (/funds)
- سجل المتبرعين: (/donors) وإيصالات التبرع: (/donations)
- كفالات الأيتام والأسر: (/sponsorships)
- المستفيدون وصرف المساعدات والقسائم: (/beneficiaries)
- التطوع وساعات الخدمة: (/volunteers)
- حاسبة الزكاة الشرعية: (/zakat)
- الفواتير الإلكترونية ZATCA: (/invoices)
- الأرشيف والوثائق المشفرة: (/archive)
- المراسلات الرسمية والبرقيات: (/letters)
- التقارير والقوائم المالية A4: (/reports)
- سجل التدقيق الجنائي غير القابل للتعديل: (/audit)
- المستخدمون والصلاحيات (41 صلاحية): (/users)
- إعدادات المؤسسة والنسخ الاحتياطي: (/settings)";

/// What the advisor may know: aggregates the caller can already see, never personal data.
async fn snapshot(s: &AppState, a: &Auth) -> ApiResult<Value> {
    let today = Utc::now().date_naive();
    let org = s.store.organization().await?;
    let mut snap = json!({
        "today": today,
        "organization": { "name": org.name_ar, "type": org.org_type, "fiscal_year_start_month": org.fiscal_year_start },
        "base_currency": s.store.base_currency,
        "user": { "role": a.role, "permissions": a.permissions.names() },
        "insights": compute(s, a).await?,
    });
    if a.can(Permission::ReportsRead) {
        let d = s.store.dashboard(today).await?;
        snap["dashboard"] = json!({
            "cash_position": d.cash_position,
            "donations_this_month": d.donations_this_month,
            "aid_this_month": d.aid_this_month,
            "active_cases": d.active_cases,
            "critical_cases": d.critical_cases,
            "cashflow_by_month": d.cashflow,
            "functional_expenses_ytd": d.functional_expenses_ytd,
        });
        snap["funds"] = json!(d.funds.iter().map(|f| json!({
            "code": f.code, "name": f.name_ar, "restriction": f.restriction,
            "balance": f.net_assets + f.revenue - f.expenses, "revenue": f.revenue, "expenses": f.expenses,
        })).collect::<Vec<_>>());
        snap["budgets"] = json!(d.budgets.iter().map(|b| json!({
            "fund": b.fund_code, "project": b.project_code, "approved": b.approved, "committed": b.committed, "spent": b.spent,
            "period": [b.period_start, b.period_end],
        })).collect::<Vec<_>>());
    }
    let count = |sql: &'static str| async move { sqlx::query_as::<_, (i64,)>(sql).fetch_one(&s.store.pool).await.map(|r| r.0) };
    if a.can(Permission::DonorsRead) {
        snap["donors"] = json!(count("SELECT COUNT(*) FROM donors").await?);
    }
    if a.can(Permission::SponsorshipsRead) {
        snap["active_sponsorships"] = json!(count("SELECT COUNT(*) FROM sponsorships WHERE status = 'active'").await?);
    }
    if a.can(Permission::VolunteersRead) {
        snap["active_volunteers"] = json!(count("SELECT COUNT(*) FROM volunteers WHERE status = 'active'").await?);
    }
    if a.can(Permission::ArchiveRead) {
        snap["archived_documents"] = json!(count("SELECT COUNT(*) FROM documents WHERE voided_at IS NULL").await?);
    }
    Ok(snap)
}

/// The answer streams back as plain UTF-8 text, piece by piece.
pub async fn chat(State(s): State<AppState>, a: Auth, Json(req): Json<ChatRequest>) -> ApiResult<Response> {
    a.require(Permission::AdvisorUse)?;
    let cfg = settings(&s).await?;
    if !cfg.enabled {
        return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "advisor_disabled", "the advisor is turned off in settings"));
    }
    let history: Vec<ChatMessage> = req
        .messages
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| ChatMessage { role: m.role, content: m.content.chars().take(MAX_MESSAGE_CHARS).collect() })
        .collect();
    if history.last().is_none_or(|m| m.role != "user" || m.content.trim().is_empty()) {
        return Err(ApiError::bad_request("the last message must be a non-empty user question"));
    }
    let start = history.len().saturating_sub(MAX_HISTORY);
    let slot = CHATS.try_acquire().map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "advisor_busy", "the advisor is answering another question"))?;
    let snap = snapshot(&s, &a).await?;
    let mut messages = vec![json!({ "role": "system", "content": format!("{SYSTEM_PROMPT}\n\nلقطة النظام (JSON):\n{snap}") })];
    messages.extend(history[start..].iter().map(|m| json!(m)));
    let body = json!({
        "model": cfg.model,
        "messages": messages,
        "stream": true,
        "temperature": 0.3,
        "max_tokens": 1200,
        // llama-server: answer directly instead of emitting a long hidden reasoning pass.
        "chat_template_kwargs": { "enable_thinking": false },
    });
    let res = client(std::time::Duration::from_secs(180))
        .post(format!("{}/chat/completions", cfg.url))
        .json(&body)
        .send()
        .await
        .map_err(|e| unavailable(format!("cannot reach the local model at {}: {e}", cfg.url)))?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        return Err(unavailable(format!("the model server answered {status}: {}", detail.chars().take(300).collect::<String>())));
    }
    let mut upstream = res.bytes_stream();
    let stream = async_stream(move |tx| async move {
        let _slot = slot;
        let mut buf = Vec::new();
        while let Some(chunk) = upstream.next().await {
            let Ok(chunk) = chunk else {
                let _ = tx.send("\n\n[انقطع الاتصال بالنموذج]".to_owned()).await;
                return;
            };
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let Some(data) = line.trim().strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" {
                    return;
                }
                if let Some(piece) = serde_json::from_str::<Value>(data).ok().and_then(|v| v["choices"][0]["delta"]["content"].as_str().map(str::to_owned)) {
                    if !piece.is_empty() && tx.send(piece).await.is_err() {
                        return; // the browser went away
                    }
                }
            }
        }
    });
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], Body::from_stream(stream)).into_response())
}

/// A stream fed by a spawned task through a small channel.
fn async_stream<F, Fut>(producer: F) -> impl futures_util::Stream<Item = Result<String, std::convert::Infallible>>
where
    F: FnOnce(tokio::sync::mpsc::Sender<String>) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(32);
    tokio::spawn(producer(tx));
    futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx).map(|o| o.map(Ok)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_warning_share_is_ninety_percent() {
        assert_eq!(BUDGET_WARNING_SHARE, Decimal::new(9, 1));
    }

    #[test]
    fn prompt_forbids_actions() {
        assert!(SYSTEM_PROMPT.contains("لا تستطيع تنفيذ أي عملية"));
    }
}
