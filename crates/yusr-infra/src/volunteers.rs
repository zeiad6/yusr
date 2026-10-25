//! Volunteer registry and logged hours, the inputs to volunteering KPIs that regulators ask
//! for (hours, active volunteers, economic value).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yusr_domain::DomainError;

use crate::{InfraError, Result, Store, audit, parse_label, serializable};

/// A single entry covers at most one day.
const MAX_HOURS_PER_ENTRY: Decimal = Decimal::from_parts(24, 0, 0, false, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolunteerStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewVolunteer {
    pub full_name: String,
    pub national_id: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub city: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub joined_on: NaiveDate,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Volunteer {
    pub id: Uuid,
    pub volunteer_no: i64,
    pub full_name: String,
    pub national_id: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub city: Option<String>,
    pub skills: Vec<String>,
    pub status: VolunteerStatus,
    pub joined_on: NaiveDate,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub total_hours: Decimal,
    pub last_activity: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewHours {
    pub volunteer_id: Uuid,
    pub activity: String,
    pub project_id: Option<Uuid>,
    pub worked_on: NaiveDate,
    pub hours: Decimal,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HoursRow {
    pub id: i64,
    pub volunteer_id: Uuid,
    pub volunteer_name: String,
    pub activity: String,
    pub project_code: Option<String>,
    pub worked_on: NaiveDate,
    pub hours: Decimal,
    pub notes: Option<String>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HoursTotal {
    pub key: String,
    pub hours: Decimal,
    pub entries: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VolunteerSummary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub total_hours: Decimal,
    pub entries: i64,
    pub volunteers: i64,
    pub by_month: Vec<HoursTotal>,
    pub by_volunteer: Vec<HoursTotal>,
    pub by_activity: Vec<HoursTotal>,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    volunteer_no: i64,
    full_name: String,
    national_id: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    city: Option<String>,
    skills: Vec<String>,
    status: String,
    joined_on: NaiveDate,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    total_hours: Decimal,
    last_activity: Option<NaiveDate>,
}

const HOURS_SELECT: &str = "SELECT h.id, h.volunteer_id, v.full_name AS volunteer_name, h.activity, p.code AS project_code, h.worked_on, h.hours, h.notes, h.created_by
   FROM volunteer_hours h JOIN volunteers v ON v.id = h.volunteer_id LEFT JOIN projects p ON p.id = h.project_id";

const SELECT: &str = "SELECT v.id, v.volunteer_no, v.full_name, v.national_id, v.phone, v.email, v.city, v.skills, v.status::text AS status,
        v.joined_on, v.notes, v.created_at, COALESCE(SUM(h.hours), 0) AS total_hours, MAX(h.worked_on) AS last_activity
   FROM volunteers v LEFT JOIN volunteer_hours h ON h.volunteer_id = v.id";

fn volunteer(r: Row) -> Result<Volunteer> {
    Ok(Volunteer {
        status: parse_label(&r.status)?,
        id: r.id,
        volunteer_no: r.volunteer_no,
        full_name: r.full_name,
        national_id: r.national_id,
        phone: r.phone,
        email: r.email,
        city: r.city,
        skills: r.skills,
        joined_on: r.joined_on,
        notes: r.notes,
        created_at: r.created_at,
        total_hours: r.total_hours,
        last_activity: r.last_activity,
    })
}

fn opt(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl Store {
    pub async fn create_volunteer(&self, n: &NewVolunteer, actor: &str) -> Result<Volunteer> {
        if n.full_name.trim().is_empty() {
            return Err(DomainError::Invalid("full_name is required".into()).into());
        }
        let skills: Vec<String> = n.skills.iter().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
        let id = serializable!(self, |conn| {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO volunteers (full_name, national_id, phone, email, city, skills, joined_on, notes, created_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
            )
            .bind(n.full_name.trim())
            .bind(opt(&n.national_id))
            .bind(opt(&n.phone))
            .bind(opt(&n.email))
            .bind(opt(&n.city))
            .bind(&skills)
            .bind(n.joined_on)
            .bind(opt(&n.notes))
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "create", "volunteer", &id.to_string(), serde_json::json!({ "full_name": n.full_name.trim() })).await?;
            Ok(id)
        })?;
        self.volunteer(id).await
    }

    pub async fn volunteers(&self, q: Option<&str>, limit: i64) -> Result<Vec<Volunteer>> {
        let pattern = q.map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
        let rows: Vec<Row> = sqlx::query_as(&format!(
            "{SELECT} WHERE ($1::text IS NULL OR v.full_name ILIKE $1 OR v.phone ILIKE $1 OR v.city ILIKE $1 OR array_to_string(v.skills, ' ') ILIKE $1)
              GROUP BY v.id ORDER BY v.status, v.volunteer_no DESC LIMIT $2"
        ))
        .bind(pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(volunteer).collect()
    }

    pub async fn volunteer(&self, id: Uuid) -> Result<Volunteer> {
        let row: Option<Row> = sqlx::query_as(&format!("{SELECT} WHERE v.id = $1 GROUP BY v.id")).bind(id).fetch_optional(&self.pool).await?;
        volunteer(row.ok_or(InfraError::NotFound("volunteer"))?)
    }

    pub async fn set_volunteer_status(&self, id: Uuid, status: VolunteerStatus, actor: &str) -> Result<Volunteer> {
        let label = if status == VolunteerStatus::Active { "active" } else { "inactive" };
        serializable!(self, |conn| {
            let n = sqlx::query("UPDATE volunteers SET status = $2::volunteer_status WHERE id = $1").bind(id).bind(label).execute(&mut *conn).await?.rows_affected();
            if n == 0 {
                return Err(InfraError::NotFound("volunteer"));
            }
            audit::append(conn, actor, "update", "volunteer", &id.to_string(), serde_json::json!({ "status": status })).await?;
            Ok(())
        })?;
        self.volunteer(id).await
    }

    pub async fn log_hours(&self, h: &NewHours, actor: &str) -> Result<HoursRow> {
        if h.hours <= Decimal::ZERO || h.hours > MAX_HOURS_PER_ENTRY || h.hours.scale() > 2 {
            return Err(DomainError::Invalid("hours must be more than 0 and at most 24, with up to 2 decimals".into()).into());
        }
        if h.activity.trim().is_empty() {
            return Err(DomainError::Invalid("activity is required".into()).into());
        }
        if h.worked_on > Utc::now().date_naive() {
            return Err(DomainError::Invalid("hours cannot be logged for a future date".into()).into());
        }
        let id = serializable!(self, |conn| {
            let status: Option<(String,)> = sqlx::query_as("SELECT status::text FROM volunteers WHERE id = $1").bind(h.volunteer_id).fetch_optional(&mut *conn).await?;
            match status {
                None => return Err(InfraError::NotFound("volunteer")),
                Some((s,)) if s != "active" => return Err(InfraError::Conflict("volunteer_inactive", "volunteer is inactive".into())),
                Some(_) => {}
            }
            let (id,): (i64,) = sqlx::query_as(
                "INSERT INTO volunteer_hours (volunteer_id, activity, project_id, worked_on, hours, notes, created_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
            )
            .bind(h.volunteer_id)
            .bind(h.activity.trim())
            .bind(h.project_id)
            .bind(h.worked_on)
            .bind(h.hours)
            .bind(opt(&h.notes))
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(conn, actor, "log_hours", "volunteer", &h.volunteer_id.to_string(), serde_json::json!({ "hours": h.hours, "worked_on": h.worked_on })).await?;
            Ok(id)
        })?;
        let row: Option<HoursRow> = sqlx::query_as(&format!("{HOURS_SELECT} WHERE h.id = $1")).bind(id).fetch_optional(&self.pool).await?;
        row.ok_or(InfraError::NotFound("volunteer hours"))
    }

    pub async fn hours(&self, volunteer_id: Option<Uuid>, from: NaiveDate, to: NaiveDate) -> Result<Vec<HoursRow>> {
        Ok(sqlx::query_as(&format!(
            "{HOURS_SELECT} WHERE ($1::uuid IS NULL OR h.volunteer_id = $1) AND h.worked_on BETWEEN $2 AND $3
              ORDER BY h.worked_on DESC, h.id DESC LIMIT 500"
        ))
        .bind(volunteer_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn volunteer_summary(&self, from: NaiveDate, to: NaiveDate) -> Result<VolunteerSummary> {
        const W: &str = "h.worked_on BETWEEN $1 AND $2";
        let (total_hours, entries, volunteers): (Decimal, i64, i64) =
            sqlx::query_as(&format!("SELECT COALESCE(SUM(h.hours), 0), COUNT(*), COUNT(DISTINCT h.volunteer_id) FROM volunteer_hours h WHERE {W}"))
                .bind(from)
                .bind(to)
                .fetch_one(&self.pool)
                .await?;
        let group = |key: &str, join: &str, order: &str| {
            format!("SELECT {key} AS key, SUM(h.hours) AS hours, COUNT(*) AS entries FROM volunteer_hours h {join} WHERE {W} GROUP BY 1 ORDER BY {order}")
        };
        let fetch = |sql: String| async move { sqlx::query_as::<_, HoursTotal>(&sql).bind(from).bind(to).fetch_all(&self.pool).await };
        Ok(VolunteerSummary {
            from,
            to,
            total_hours,
            entries,
            volunteers,
            by_month: fetch(group("to_char(h.worked_on, 'YYYY-MM')", "", "1")).await?,
            by_volunteer: fetch(group("v.full_name", "JOIN volunteers v ON v.id = h.volunteer_id", "hours DESC")).await?,
            by_activity: fetch(group("h.activity", "", "hours DESC")).await?,
        })
    }
}
