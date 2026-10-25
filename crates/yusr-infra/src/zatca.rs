//! ZATCA invoice chain: assigns the invoice counter (ICV) and previous hash (PIH), signs,
//! and stores every invoice so the chain can be reported or audited later.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yusr_domain::zatca::invoice::{DocumentKind, Invoice, InvoiceLine, InvoiceType, Party};
use yusr_domain::zatca::sign::{Credentials, INITIAL_PIH, sign_invoice};

use crate::{InfraError, Result, Store, audit, serializable};

/// An invoice as requested by a user; the chain fields are filled in here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRequest {
    pub number: Option<String>,
    pub issued_at: NaiveDateTime,
    pub invoice_type: InvoiceType,
    pub kind: DocumentKind,
    pub billing_reference: Option<String>,
    pub note_reason: Option<String>,
    pub seller: Party,
    pub buyer: Option<Party>,
    pub currency: String,
    pub payment_means: String,
    pub lines: Vec<InvoiceLine>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StoredInvoice {
    pub id: Uuid,
    pub number: String,
    pub icv: i64,
    pub invoice_hash: String,
    pub previous_hash: String,
    pub invoice_type: String,
    pub qr: String,
    pub status: String,
}

impl Store {
    pub async fn issue_invoice(&self, req: &InvoiceRequest, creds: &Credentials, actor: &str) -> Result<(StoredInvoice, String)> {
        serializable!(self, |conn| {
            let head: Option<(i64, String)> = sqlx::query_as("SELECT icv, invoice_hash FROM zatca_invoices ORDER BY icv DESC LIMIT 1")
                .fetch_optional(&mut *conn)
                .await?;
            let (counter, previous_hash) = head.map_or((1, INITIAL_PIH.to_owned()), |(icv, h)| (icv + 1, h));
            let id = Uuid::new_v4();
            let invoice = Invoice {
                number: req.number.clone().unwrap_or_else(|| format!("INV-{counter:08}")),
                uuid: id,
                issue_date: req.issued_at.date(),
                issue_time: req.issued_at.time(),
                invoice_type: req.invoice_type,
                kind: req.kind,
                billing_reference: req.billing_reference.clone(),
                note_reason: req.note_reason.clone(),
                counter: counter as u64,
                previous_hash: previous_hash.clone(),
                seller: req.seller.clone(),
                buyer: req.buyer.clone(),
                currency: req.currency.clone(),
                payment_means: req.payment_means.clone(),
                lines: req.lines.clone(),
            };
            let signed = sign_invoice(&invoice, creds, req.issued_at)?;
            let type_label = match req.invoice_type {
                InvoiceType::Standard => "standard",
                InvoiceType::Simplified => "simplified",
            };
            let stored: StoredInvoice = sqlx::query_as(
                "INSERT INTO zatca_invoices (id, number, icv, invoice_hash, previous_hash, invoice_type, xml, qr)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 RETURNING id, number, icv, invoice_hash, previous_hash, invoice_type, qr, status",
            )
            .bind(id)
            .bind(&invoice.number)
            .bind(counter)
            .bind(&signed.invoice_hash)
            .bind(&previous_hash)
            .bind(type_label)
            .bind(&signed.xml)
            .bind(&signed.qr)
            .fetch_one(&mut *conn)
            .await?;
            audit::append(
                conn,
                actor,
                "issue",
                "zatca_invoice",
                &id.to_string(),
                serde_json::json!({ "number": invoice.number, "icv": counter, "hash": signed.invoice_hash }),
            )
            .await?;
            Ok((stored, signed.xml))
        })
    }

    /// Newest first, by invoice counter (ICV).
    pub async fn invoices(&self, limit: i64) -> Result<Vec<StoredInvoice>> {
        Ok(sqlx::query_as(
            "SELECT id, number, icv, invoice_hash, previous_hash, invoice_type, qr, status FROM zatca_invoices ORDER BY icv DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn invoice_xml(&self, id: Uuid) -> Result<String> {
        let row: Option<(String,)> = sqlx::query_as("SELECT xml FROM zatca_invoices WHERE id = $1").bind(id).fetch_optional(&self.pool).await?;
        row.map(|(x,)| x).ok_or(InfraError::NotFound("invoice"))
    }
}
