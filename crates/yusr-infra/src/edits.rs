//! Corrections to registry records (donors, cases, funds, accounts, projects, budgets,
//! volunteers, sponsorships, archive metadata, donation notes). Posted money never changes
//! here: amounts, funds and dates of ledger-backed records are corrected by a reversing entry.
//! Every edit is recorded in the audit chain with the new values.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yusr_domain::DomainError;
use yusr_domain::charity::Asnaf;

use crate::archive::Document;
use crate::aid::Beneficiary;
use crate::donations::{Donor, NewDonor};
use crate::sponsorships::Sponsorship;
use crate::volunteers::{NewVolunteer, Volunteer};
use crate::{InfraError, Result, Store, audit, serializable};

fn clean(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn required(v: &str, field: &str) -> Result<()> {
    if v.trim().is_empty() {
        return Err(DomainError::Invalid(format!("{field} is required")).into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeneficiaryEdit {
    pub full_name: String,
    pub national_id: Option<String>,
    pub phone: Option<String>,
    pub city: Option<String>,
    pub category: Asnaf,
    /// `active`, `suspended` or `closed`.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamesEdit {
    pub name_ar: String,
    pub name_en: String,
    /// Funds only; absent keeps the current purpose.
    #[serde(default)]
    pub purpose: Option<String>,
    /// Accounts only: inactive accounts stay in reports but cannot be chosen for new entries.
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetEdit {
    pub period_end: NaiveDate,
    pub approved: Decimal,
    pub committed: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsorshipEdit {
    pub beneficiary_id: Option<Uuid>,
    pub monthly_amount: Decimal,
    pub end_date: Option<NaiveDate>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEdit {
    pub title: String,
    pub reference_no: Option<String>,
    pub document_date: NaiveDate,
    pub party: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// The only donation fields that carry no accounting meaning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DonationEdit {
    pub payment_ref: Option<String>,
    pub note: Option<String>,
}

macro_rules! update_one {
    ($store:expr, $actor:expr, $entity:literal, $id:expr, $payload:expr, $query:expr) => {{
        let payload = serde_json::to_value($payload).expect("edits serialize");
        serializable!($store, |conn| {
            if $query.execute(&mut *conn).await?.rows_affected() == 0 {
                return Err(InfraError::NotFound($entity));
            }
            audit::append(conn, $actor, "update", $entity, &$id.to_string(), payload.clone()).await?;
            Ok(())
        })
    }};
}

impl Store {
    pub async fn update_donor(&self, id: Uuid, d: &NewDonor, actor: &str) -> Result<Donor> {
        required(&d.full_name, "full_name")?;
        if !matches!(d.preferred_language.as_str(), "ar" | "en") {
            return Err(DomainError::Invalid("preferred_language must be ar or en".into()).into());
        }
        update_one!(self, actor, "donor", id, d, sqlx::query(
            "UPDATE donors SET is_organization = $2, full_name = $3, national_id = $4, phone = $5, email = $6, country = $7,
                               preferred_language = $8, notes = $9 WHERE id = $1",
        )
        .bind(id)
        .bind(d.is_organization)
        .bind(d.full_name.trim())
        .bind(clean(&d.national_id))
        .bind(clean(&d.phone))
        .bind(clean(&d.email))
        .bind(clean(&d.country))
        .bind(&d.preferred_language)
        .bind(clean(&d.notes)))?;
        self.donor(id).await
    }

    /// Identity and status only; the needs profile changes through a reassessment, which keeps
    /// the score history.
    pub async fn update_beneficiary(&self, id: Uuid, b: &BeneficiaryEdit, actor: &str) -> Result<Beneficiary> {
        required(&b.full_name, "full_name")?;
        if !matches!(b.status.as_str(), "active" | "suspended" | "closed") {
            return Err(DomainError::Invalid("status must be active, suspended or closed".into()).into());
        }
        update_one!(self, actor, "beneficiary", id, b, sqlx::query(
            "UPDATE beneficiaries SET full_name = $2, national_id = $3, phone = $4, city = $5, category = $6::asnaf,
                                      status = $7::case_status, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(b.full_name.trim())
        .bind(clean(&b.national_id))
        .bind(clean(&b.phone))
        .bind(clean(&b.city))
        .bind(b.category.as_str())
        .bind(&b.status))?;
        self.beneficiary(id).await
    }

    /// Names and purpose; restriction and giving type are fixed once money may have moved.
    pub async fn update_fund(&self, id: Uuid, e: &NamesEdit, actor: &str) -> Result<()> {
        required(&e.name_ar, "name_ar")?;
        required(&e.name_en, "name_en")?;
        update_one!(self, actor, "fund", id, e, sqlx::query("UPDATE funds SET name_ar = $2, name_en = $3, purpose = COALESCE($4, purpose) WHERE id = $1")
            .bind(id)
            .bind(e.name_ar.trim())
            .bind(e.name_en.trim())
            .bind(clean(&e.purpose)))
    }

    /// Names and the active flag; code, type and parent define the books and stay fixed.
    pub async fn update_account(&self, id: Uuid, e: &NamesEdit, actor: &str) -> Result<()> {
        required(&e.name_ar, "name_ar")?;
        required(&e.name_en, "name_en")?;
        update_one!(self, actor, "account", id, e, sqlx::query(
            "UPDATE accounts SET name_ar = $2, name_en = $3, is_active = COALESCE($4, is_active) WHERE id = $1",
        )
        .bind(id)
        .bind(e.name_ar.trim())
        .bind(e.name_en.trim())
        .bind(e.is_active))
    }

    pub async fn update_project(&self, id: Uuid, e: &NamesEdit, actor: &str) -> Result<()> {
        required(&e.name_ar, "name_ar")?;
        required(&e.name_en, "name_en")?;
        update_one!(self, actor, "project", id, e, sqlx::query("UPDATE projects SET name_ar = $2, name_en = $3 WHERE id = $1")
            .bind(id)
            .bind(e.name_ar.trim())
            .bind(e.name_en.trim()))
    }

    pub async fn update_budget(&self, id: Uuid, e: &BudgetEdit, actor: &str) -> Result<()> {
        if e.approved < Decimal::ZERO || e.committed < Decimal::ZERO {
            return Err(DomainError::Invalid("budget amounts cannot be negative".into()).into());
        }
        // The CHECK (period_end >= period_start) constraint rejects an end before the start.
        update_one!(self, actor, "budget", id, e, sqlx::query("UPDATE budgets SET period_end = $2, approved = $3, committed = $4 WHERE id = $1")
            .bind(id)
            .bind(e.period_end)
            .bind(e.approved)
            .bind(e.committed))
    }

    pub async fn update_volunteer(&self, id: Uuid, n: &NewVolunteer, actor: &str) -> Result<Volunteer> {
        required(&n.full_name, "full_name")?;
        let skills: Vec<String> = n.skills.iter().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
        update_one!(self, actor, "volunteer", id, n, sqlx::query(
            "UPDATE volunteers SET full_name = $2, national_id = $3, phone = $4, email = $5, city = $6, skills = $7,
                                   joined_on = $8, notes = $9 WHERE id = $1",
        )
        .bind(id)
        .bind(n.full_name.trim())
        .bind(clean(&n.national_id))
        .bind(clean(&n.phone))
        .bind(clean(&n.email))
        .bind(clean(&n.city))
        .bind(&skills)
        .bind(n.joined_on)
        .bind(clean(&n.notes)))?;
        self.volunteer(id).await
    }

    /// A new monthly amount applies to the months still to be paid; recorded payments keep
    /// their receipts.
    pub async fn update_sponsorship(&self, id: Uuid, e: &SponsorshipEdit, actor: &str) -> Result<Sponsorship> {
        if e.monthly_amount <= Decimal::ZERO {
            return Err(DomainError::NonPositiveAmount { line: 0, amount: e.monthly_amount }.into());
        }
        let start = self.sponsorship(id).await?.sponsorship.start_date;
        if e.end_date.is_some_and(|end| end < start) {
            return Err(DomainError::Invalid("end_date is before start_date".into()).into());
        }
        update_one!(self, actor, "sponsorship", id, e, sqlx::query(
            "UPDATE sponsorships SET beneficiary_id = $2, monthly_amount = $3, end_date = $4, notes = $5
              WHERE id = $1",
        )
        .bind(id)
        .bind(e.beneficiary_id)
        .bind(e.monthly_amount)
        .bind(e.end_date)
        .bind(clean(&e.notes)))?;
        Ok(self.sponsorship(id).await?.sponsorship)
    }

    /// Metadata only. The category is fixed: moving a document in or out of a confidential
    /// category would change who can see it.
    pub async fn update_document(&self, id: Uuid, e: &DocumentEdit, actor: &str) -> Result<Document> {
        required(&e.title, "title")?;
        let tags: Vec<String> = e.tags.iter().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect();
        update_one!(self, actor, "document", id, e, sqlx::query(
            "UPDATE documents SET title = $2, reference_no = $3, document_date = $4, party = $5, description = $6, tags = $7
              WHERE id = $1 AND voided_at IS NULL",
        )
        .bind(id)
        .bind(e.title.trim())
        .bind(clean(&e.reference_no))
        .bind(e.document_date)
        .bind(clean(&e.party))
        .bind(clean(&e.description))
        .bind(&tags))?;
        Ok(self.document(id).await?.document)
    }

    pub async fn update_donation(&self, id: Uuid, e: &DonationEdit, actor: &str) -> Result<()> {
        update_one!(self, actor, "donation", id, e, sqlx::query("UPDATE donations SET payment_ref = $2, note = $3 WHERE id = $1")
            .bind(id)
            .bind(clean(&e.payment_ref))
            .bind(clean(&e.note)))
    }
}
