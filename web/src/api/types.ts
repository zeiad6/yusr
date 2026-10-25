// Mirrors the JSON of crates/yusr-*/src. Decimals travel as strings (rust_decimal
// `serde-with-str`) and are sent back as strings so no precision is lost.

export type Decimal = string;
export type Uuid = string;
export type IsoDate = string;

export type Role = "admin" | "accountant" | "case_worker" | "auditor" | "viewer";
export type DonationKind = "zakat" | "sadaqah" | "kaffarah" | "waqf" | "general";
export type FundRestriction = "unrestricted" | "temporarily_restricted" | "permanently_restricted";
export type AccountType = "asset" | "liability" | "net_assets" | "revenue" | "expense";
export type Side = "debit" | "credit";
export type Asnaf =
  | "fuqara" | "masakin" | "amilin" | "muallafah" | "riqab" | "gharimin" | "fi_sabilillah" | "ibn_sabil";
export type Housing = "owned" | "rented" | "hosted" | "homeless";
export type AidKind = "cash" | "in_kind" | "voucher";
export type Priority = "critical" | "high" | "medium" | "low";

export const DONATION_KINDS: DonationKind[] = ["zakat", "sadaqah", "kaffarah", "waqf", "general"];
export const ROLES: Role[] = ["admin", "accountant", "case_worker", "auditor", "viewer"];
export const ACCOUNT_TYPES: AccountType[] = ["asset", "liability", "net_assets", "revenue", "expense"];
export const RESTRICTIONS: FundRestriction[] = ["unrestricted", "temporarily_restricted", "permanently_restricted"];
export const ASNAF: Asnaf[] = ["fuqara", "masakin", "amilin", "muallafah", "riqab", "gharimin", "fi_sabilillah", "ibn_sabil"];
export const HOUSING: Housing[] = ["owned", "rented", "hosted", "homeless"];
export const AID_KINDS: AidKind[] = ["cash", "in_kind", "voucher"];

export interface LoginResponse {
  token: string;
  expires_at: number;
  role: Role;
  full_name: string;
  permissions: string[];
}

export interface Me {
  user_id: Uuid;
  username: string;
  role: Role;
  permissions: string[];
}

export interface User {
  id: Uuid;
  username: string;
  full_name: string;
  role: Role;
  is_active: boolean;
  locked_until: string | null;
  created_at: string;
  /** null: the role's defaults. */
  permissions: string[] | null;
}

export interface Organization {
  name_ar: string;
  name_en: string;
  license_no: string;
  vat_number: string | null;
  address: string;
  tax_statement_ar: string;
  tax_statement_en: string;
  /** data:image/png|jpeg|webp;base64 — printed on receipts and letterheads. */
  logo_data_url?: string | null;
  stamp_data_url?: string | null;
  org_type?: string | null;
  registration_no?: string | null;
  phone?: string | null;
  email?: string | null;
  website?: string | null;
  /** Month 1–12 the fiscal year starts. */
  fiscal_year_start?: number;
}

export const ORG_TYPES = ["charity_association", "foundation", "waqf", "zakat_committee", "cooperative", "mosque", "other"] as const;

export interface Account {
  id: Uuid;
  code: string;
  name_ar: string;
  name_en: string;
  account_type: AccountType;
  parent_id: Uuid | null;
  is_postable: boolean;
  is_active: boolean;
  functional_class: string | null;
}

export interface NewAccount {
  code: string;
  name_ar: string;
  name_en: string;
  account_type: AccountType;
  parent_id: Uuid | null;
  is_postable: boolean;
  functional_class: string | null;
}

export interface Fund {
  id: Uuid;
  code: string;
  name_ar: string;
  name_en: string;
  restriction: FundRestriction;
  donation_kind: DonationKind | null;
}

export interface NewFund {
  code: string;
  name_ar: string;
  name_en: string;
  restriction: FundRestriction;
  donation_kind: DonationKind | null;
  purpose: string | null;
}

export interface Project {
  id: Uuid;
  code: string;
  name_ar: string;
  name_en: string;
  fund_id: Uuid;
}

export type NewProject = Omit<Project, "id">;

export interface BudgetUsage {
  id: Uuid;
  fund_code: string;
  project_code: string | null;
  period_start: IsoDate;
  period_end: IsoDate;
  approved: Decimal;
  committed: Decimal;
  spent: Decimal;
}

export interface NewBudget {
  fund_id: Uuid;
  project_id: Uuid | null;
  period_start: IsoDate;
  period_end: IsoDate;
  approved: Decimal;
  committed: Decimal;
}

export interface LineDraft {
  account_id: Uuid;
  fund_id: Uuid;
  project_id: Uuid | null;
  side: Side;
  amount: Decimal;
  currency: string;
  memo: string | null;
}

export interface EntryDraft {
  entry_date: IsoDate;
  description: string;
  reference: string | null;
  lines: LineDraft[];
}

export interface PostedLine extends LineDraft {
  fx_rate: Decimal;
  base_amount: Decimal;
}

export interface PostedEntry {
  id: Uuid;
  entry_no: number;
  entry_date: IsoDate;
  hijri_date: string;
  lines: PostedLine[];
  audit_hash: string;
  budget_warnings: Uuid[];
}

export interface EntryView {
  id: Uuid;
  entry_no: number;
  entry_date: IsoDate;
  hijri_date: string;
  description: string;
  reference: string | null;
  status: string;
  source: string;
  created_by: string;
  reverses_id: Uuid | null;
  lines: PostedLine[];
}

export interface TrialBalanceRow {
  account_id: Uuid;
  code: string;
  name_ar: string;
  name_en: string;
  account_type: AccountType;
  debit: Decimal;
  credit: Decimal;
}

export interface FundBalance {
  fund_id: Uuid;
  code: string;
  name_ar: string;
  name_en: string;
  restriction: FundRestriction;
  net_assets: Decimal;
  revenue: Decimal;
  expenses: Decimal;
}

export interface Dashboard {
  as_of: IsoDate;
  hijri_as_of: string;
  base_currency: string;
  cash_position: Decimal;
  donations_this_month: { kind: DonationKind; total: Decimal; count: number }[];
  aid_this_month: Decimal;
  active_cases: number;
  critical_cases: number;
  funds: FundBalance[];
  cashflow: { month: string; inflow: Decimal; outflow: Decimal }[];
  functional_expenses_ytd: { class: string; total: Decimal }[];
  budgets: BudgetUsage[];
}

export interface Donor {
  id: Uuid;
  is_organization: boolean;
  full_name: string;
  national_id: string | null;
  phone: string | null;
  email: string | null;
  country: string | null;
  preferred_language: string;
  notes: string | null;
  created_at: string;
}

export type NewDonor = Omit<Donor, "id" | "created_at">;

export interface NewDonation {
  donor_id: Uuid;
  donation_date: IsoDate;
  kind: DonationKind | null;
  donor_designated: boolean;
  fund_id: Uuid | null;
  project_id: Uuid | null;
  amount: Decimal;
  currency: string;
  payment_method: string;
  payment_ref: string | null;
  note: string | null;
}

export interface RecordedDonation {
  donation_id: Uuid;
  receipt_no: string;
  kind: DonationKind;
  fund_id: Uuid;
  entry: PostedEntry;
}

export interface DonationRow {
  id: Uuid;
  receipt_no: string;
  donor_id: Uuid;
  donor_name: string;
  donation_date: IsoDate;
  kind: DonationKind;
  fund_code: string;
  amount: Decimal;
  currency: string;
  base_amount: Decimal;
  payment_method: string;
  payment_ref: string | null;
  note: string | null;
}

export interface CaseProfile {
  household_size: number;
  monthly_income: Decimal;
  poverty_line_per_person: Decimal;
  dependents_under_18: number;
  members_with_disability: number;
  members_with_chronic_illness: number;
  outstanding_debt: Decimal;
  housing: Housing;
  monthly_rent: Decimal;
  head_of_household_employed: boolean;
  female_or_orphan_headed: boolean;
}

export interface NewBeneficiary {
  full_name: string;
  national_id: string | null;
  phone: string | null;
  city: string | null;
  category: Asnaf;
  profile: CaseProfile;
}

export interface Beneficiary {
  id: Uuid;
  file_no: number;
  full_name: string;
  national_id: string | null;
  phone: string | null;
  city: string | null;
  category: Asnaf;
  profile: CaseProfile;
  score: number;
  priority: Priority;
  status: string;
  created_at: string;
}

export interface NewDistribution {
  beneficiary_id: Uuid;
  kind: AidKind;
  fund_id: Uuid;
  project_id: Uuid | null;
  amount: Decimal;
  currency: string;
  description: string;
  items: unknown | null;
  distributed_on: IsoDate;
  paid_from_bank: boolean;
}

export interface Distribution {
  id: Uuid;
  voucher_code: string | null;
  entry: PostedEntry;
}

export interface ZakatInput {
  cash_and_bank: Decimal;
  gold_grams: Decimal;
  silver_grams: Decimal;
  gold_price_per_gram: Decimal;
  silver_price_per_gram: Decimal;
  trade_goods_market_value: Decimal;
  collectible_receivables: Decimal;
  trading_investments: Decimal;
  zakatable_share_of_long_term_investments: Decimal;
  current_liabilities: Decimal;
  nisab_basis: "gold" | "silver" | "lower_of_gold_and_silver";
  year_basis: "lunar" | "solar";
  hawl_start: IsoDate | null;
  assessment_date: IsoDate;
}

export interface ZakatAssessment {
  gross_zakatable_assets: Decimal;
  deductible_liabilities: Decimal;
  net_zakatable_assets: Decimal;
  nisab_value: Decimal;
  rate: Decimal;
  hawl_complete: boolean;
  hawl_due_hijri: { year: number; month: number; day: number } | null;
  zakat_due: Decimal;
  exemption_reason: string | null;
}

export interface AuditRecord {
  seq: number;
  occurred_at: string;
  actor: string;
  action: string;
  entity: string;
  entity_id: string;
  payload: unknown;
  prev_hash: string;
  hash: string;
}

export interface VerifyReport {
  records_checked: number;
  head_hash: string | null;
  ok: boolean;
  error: string | null;
}

export interface Address {
  street: string;
  building_number: string;
  district: string;
  city: string;
  postal_zone: string;
  country: string;
}

export interface Party {
  registration_name: string;
  vat_number: string | null;
  other_id: [string, string] | null;
  address: Address;
}

export type VatCategory = "S" | "Z" | "E" | "O";

export interface InvoiceLine {
  name: string;
  quantity: Decimal;
  unit_price: Decimal;
  vat_category: VatCategory;
  vat_percent: Decimal;
  exemption: [string, string] | null;
}

export interface InvoiceRequest {
  number: string | null;
  issued_at: string;
  invoice_type: "standard" | "simplified";
  kind: "invoice" | "debit_note" | "credit_note";
  billing_reference: string | null;
  note_reason: string | null;
  seller: Party;
  buyer: Party | null;
  currency: string;
  payment_means: string;
  lines: InvoiceLine[];
}

export interface StoredInvoice {
  id: Uuid;
  number: string;
  icv: number;
  invoice_hash: string;
  previous_hash: string;
  invoice_type: string;
  qr: string;
  status: string;
}

export type DocumentCategory = "incoming" | "outgoing" | "contract" | "financial" | "report" | "beneficiary" | "donor" | "hr" | "legal" | "other";
export const DOCUMENT_CATEGORIES: DocumentCategory[] = ["incoming", "outgoing", "contract", "financial", "report", "beneficiary", "donor", "hr", "legal", "other"];

export interface NewDocument {
  title: string;
  category: DocumentCategory;
  reference_no: string | null;
  document_date: IsoDate;
  party: string | null;
  description: string | null;
  tags: string[];
}

export interface ArchiveDocument extends NewDocument {
  id: Uuid;
  doc_no: string;
  voided_at: string | null;
  void_reason: string | null;
  created_by: string;
  created_at: string;
  file_count: number;
  total_bytes: number;
}

export interface DocumentFile {
  id: Uuid;
  document_id: Uuid;
  file_name: string;
  content_type: string;
  size_bytes: number;
  sha256: string;
  uploaded_by: string;
  uploaded_at: string;
}

export interface DocumentDetail extends ArchiveDocument {
  files: DocumentFile[];
}

export type LetterKind = "official" | "congratulation" | "condolence" | "thanks" | "invitation" | "circular";
export const LETTER_KINDS: LetterKind[] = ["official", "congratulation", "condolence", "thanks", "invitation", "circular"];

export interface LetterDraft {
  kind: LetterKind;
  recipient_name: string;
  recipient_title: string | null;
  recipient_org: string | null;
  subject: string;
  body: string;
  closing: string | null;
  signatory_name: string;
  signatory_title: string | null;
  attachments: string | null;
  cc: string | null;
  letter_date: IsoDate;
}

export interface Letter extends LetterDraft {
  id: Uuid;
  ref_no: string | null;
  hijri_date: string;
  status: "draft" | "issued";
  issued_at: string | null;
  issued_by: string | null;
  archive_id: Uuid | null;
  created_by: string;
  created_at: string;
}

export interface ActivityRow {
  code: string;
  name_ar: string;
  name_en: string;
  account_type: "revenue" | "expense";
  restriction: FundRestriction;
  amount: Decimal;
}

export interface GroupTotal {
  key: string;
  name_ar: string | null;
  name_en: string | null;
  total: Decimal;
  count: number;
}

export interface DonationSummary {
  from: IsoDate;
  to: IsoDate;
  total: Decimal;
  count: number;
  donors: number;
  by_kind: GroupTotal[];
  by_fund: GroupTotal[];
  by_method: GroupTotal[];
  by_month: GroupTotal[];
  top_donors: GroupTotal[];
}

export interface AidSummary {
  from: IsoDate;
  to: IsoDate;
  total: Decimal;
  count: number;
  beneficiaries: number;
  by_kind: GroupTotal[];
  by_category: GroupTotal[];
  by_fund: GroupTotal[];
  by_month: GroupTotal[];
}

export type SponsorshipProgram = "orphan" | "family" | "student" | "patient" | "elderly" | "other";
export const SPONSORSHIP_PROGRAMS: SponsorshipProgram[] = ["orphan", "family", "student", "patient", "elderly", "other"];
export type SponsorshipStatus = "active" | "paused" | "ended";

export interface NewSponsorship {
  donor_id: Uuid;
  beneficiary_id: Uuid | null;
  program: SponsorshipProgram;
  kind: DonationKind;
  fund_id: Uuid | null;
  monthly_amount: Decimal;
  currency: string;
  start_date: IsoDate;
  end_date: IsoDate | null;
  notes: string | null;
}

export interface Sponsorship extends NewSponsorship {
  id: Uuid;
  sponsorship_no: number;
  donor_name: string;
  beneficiary_name: string | null;
  status: SponsorshipStatus;
  created_at: string;
  months_due: number;
  months_paid: number;
  amount_paid: Decimal;
  arrears_months: number;
  last_payment: IsoDate | null;
}

export interface SponsorshipDetail extends Sponsorship {
  payments: { donation_id: Uuid; receipt_no: string; donation_date: IsoDate; months: number; amount: Decimal; currency: string }[];
}

export interface Volunteer {
  id: Uuid;
  volunteer_no: number;
  full_name: string;
  national_id: string | null;
  phone: string | null;
  email: string | null;
  city: string | null;
  skills: string[];
  status: "active" | "inactive";
  joined_on: IsoDate;
  notes: string | null;
  created_at: string;
  total_hours: Decimal;
  last_activity: IsoDate | null;
}

export interface HoursRow {
  id: number;
  volunteer_id: Uuid;
  volunteer_name: string;
  activity: string;
  project_code: string | null;
  worked_on: IsoDate;
  hours: Decimal;
  notes: string | null;
  created_by: string;
}

export interface VolunteerSummary {
  from: IsoDate;
  to: IsoDate;
  total_hours: Decimal;
  entries: number;
  volunteers: number;
  by_month: { key: string; hours: Decimal; entries: number }[];
  by_volunteer: { key: string; hours: Decimal; entries: number }[];
  by_activity: { key: string; hours: Decimal; entries: number }[];
}
