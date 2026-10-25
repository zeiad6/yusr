-- Yusr core schema. Money is NUMERIC (never float). Ledger and audit tables are
-- write-once: triggers below reject UPDATE/DELETE/TRUNCATE, and a deferred constraint
-- trigger re-checks that every fund in every entry balances at COMMIT.

CREATE TYPE account_type     AS ENUM ('asset', 'liability', 'net_assets', 'revenue', 'expense');
CREATE TYPE functional_class AS ENUM ('program', 'management_general', 'fundraising');
CREATE TYPE fund_restriction AS ENUM ('unrestricted', 'temporarily_restricted', 'permanently_restricted');
CREATE TYPE donation_kind    AS ENUM ('zakat', 'sadaqah', 'kaffarah', 'waqf', 'general');
CREATE TYPE entry_side       AS ENUM ('debit', 'credit');
CREATE TYPE entry_status     AS ENUM ('posted', 'reversed');
CREATE TYPE asnaf            AS ENUM ('fuqara', 'masakin', 'amilin', 'muallafah', 'riqab', 'gharimin', 'fi_sabilillah', 'ibn_sabil');
CREATE TYPE aid_kind         AS ENUM ('cash', 'in_kind', 'voucher');
CREATE TYPE voucher_status   AS ENUM ('issued', 'redeemed', 'void');
CREATE TYPE case_status      AS ENUM ('active', 'suspended', 'closed');
CREATE TYPE user_role        AS ENUM ('admin', 'accountant', 'case_worker', 'auditor', 'viewer');
CREATE TYPE payment_status   AS ENUM ('pending', 'paid', 'failed', 'refunded');

CREATE DOMAIN currency_code AS CHAR(3) CHECK (VALUE ~ '^[A-Z]{3}$');
CREATE DOMAIN money_amount  AS NUMERIC(20, 4);

-- Organisation-wide settings: a single row.
CREATE TABLE settings (
    id                  BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    base_currency       currency_code NOT NULL DEFAULT 'SAR',
    organization        JSONB NOT NULL,
    voucher_secret      BYTEA NOT NULL CHECK (length(voucher_secret) >= 32),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE exchange_rates (
    currency    currency_code NOT NULL,
    rate_date   DATE NOT NULL,
    -- Base-currency units per one unit of `currency`.
    rate        NUMERIC(20, 10) NOT NULL CHECK (rate > 0),
    source      TEXT NOT NULL DEFAULT 'manual',
    PRIMARY KEY (currency, rate_date)
);

CREATE TABLE accounts (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code             TEXT NOT NULL UNIQUE CHECK (code ~ '^[0-9]{1,10}$'),
    name_ar          TEXT NOT NULL,
    name_en          TEXT NOT NULL,
    account_type     account_type NOT NULL,
    parent_id        UUID REFERENCES accounts (id),
    -- Header accounts group children and cannot be posted to.
    is_postable      BOOLEAN NOT NULL DEFAULT TRUE,
    is_active        BOOLEAN NOT NULL DEFAULT TRUE,
    -- Functional expense classification (FASB ASC 958-720) for the expense statement.
    functional_class functional_class,
    CHECK (functional_class IS NULL OR account_type = 'expense')
);

CREATE TABLE funds (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code          TEXT NOT NULL UNIQUE,
    name_ar       TEXT NOT NULL,
    name_en       TEXT NOT NULL,
    restriction   fund_restriction NOT NULL,
    -- Set for funds that hold one Islamic giving type (Zakat, Kaffarah, Waqf).
    donation_kind donation_kind,
    purpose       TEXT,
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE projects (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code       TEXT NOT NULL UNIQUE,
    name_ar    TEXT NOT NULL,
    name_en    TEXT NOT NULL,
    fund_id    UUID NOT NULL REFERENCES funds (id),
    is_active  BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE budgets (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fund_id       UUID NOT NULL REFERENCES funds (id),
    project_id    UUID REFERENCES projects (id),
    period_start  DATE NOT NULL,
    period_end    DATE NOT NULL,
    approved      money_amount NOT NULL CHECK (approved >= 0),
    committed     money_amount NOT NULL DEFAULT 0 CHECK (committed >= 0),
    CHECK (period_end >= period_start)
);
-- One budget per fund/project and period start; NULL project means the whole fund.
CREATE UNIQUE INDEX budgets_scope_uq ON budgets (fund_id, COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid), period_start);

CREATE TABLE users (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username       TEXT NOT NULL UNIQUE CHECK (username ~ '^[a-z0-9._-]{3,64}$'),
    full_name      TEXT NOT NULL,
    password_hash  TEXT NOT NULL,
    role           user_role NOT NULL,
    is_active      BOOLEAN NOT NULL DEFAULT TRUE,
    failed_logins  INTEGER NOT NULL DEFAULT 0,
    locked_until   TIMESTAMPTZ,
    -- Bumped on password change or logout-everywhere; tokens carry it and die with it.
    token_version  INTEGER NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE journal_entries (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entry_no     BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
    entry_date   DATE NOT NULL,
    -- Umm al-Qura date of entry_date, stored for reports and validated by the application.
    hijri_date   TEXT NOT NULL CHECK (hijri_date ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'),
    description  TEXT NOT NULL CHECK (btrim(description) <> ''),
    reference    TEXT,
    status       entry_status NOT NULL DEFAULT 'posted',
    reverses_id  UUID UNIQUE REFERENCES journal_entries (id),
    source       TEXT NOT NULL DEFAULT 'manual',
    created_by   TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX journal_entries_date_idx ON journal_entries (entry_date);

CREATE TABLE journal_lines (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    entry_id     UUID NOT NULL REFERENCES journal_entries (id),
    line_no      INTEGER NOT NULL,
    account_id   UUID NOT NULL REFERENCES accounts (id),
    fund_id      UUID NOT NULL REFERENCES funds (id),
    project_id   UUID REFERENCES projects (id),
    side         entry_side NOT NULL,
    amount       money_amount NOT NULL CHECK (amount > 0),
    currency     currency_code NOT NULL,
    fx_rate      NUMERIC(20, 10) NOT NULL CHECK (fx_rate > 0),
    base_amount  money_amount NOT NULL CHECK (base_amount > 0),
    signed_base  money_amount GENERATED ALWAYS AS (CASE side WHEN 'debit' THEN base_amount ELSE -base_amount END) STORED,
    memo         TEXT,
    UNIQUE (entry_id, line_no)
);
CREATE INDEX journal_lines_account_idx ON journal_lines (account_id);
CREATE INDEX journal_lines_fund_idx ON journal_lines (fund_id, account_id);
CREATE INDEX journal_lines_project_idx ON journal_lines (project_id) WHERE project_id IS NOT NULL;

-- Every fund inside an entry must balance; checked once per entry at COMMIT so the
-- application can insert lines one by one.
CREATE FUNCTION check_entry_balanced() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    bad RECORD;
BEGIN
    SELECT fund_id, SUM(signed_base) AS diff INTO bad
      FROM journal_lines WHERE entry_id = NEW.entry_id
     GROUP BY fund_id HAVING SUM(signed_base) <> 0 LIMIT 1;
    IF FOUND THEN
        RAISE EXCEPTION 'entry % fund % out of balance by %', NEW.entry_id, bad.fund_id, bad.diff
            USING ERRCODE = 'check_violation';
    END IF;
    IF (SELECT COUNT(*) FROM journal_lines WHERE entry_id = NEW.entry_id) < 2 THEN
        RAISE EXCEPTION 'entry % has fewer than two lines', NEW.entry_id USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END $$;

CREATE CONSTRAINT TRIGGER journal_lines_balanced
    AFTER INSERT ON journal_lines DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION check_entry_balanced();

CREATE FUNCTION reject_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is write-once: % is not allowed', TG_TABLE_NAME, TG_OP
        USING ERRCODE = 'insufficient_privilege';
END $$;

CREATE TRIGGER journal_lines_worm BEFORE UPDATE OR DELETE ON journal_lines
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER journal_lines_no_truncate BEFORE TRUNCATE ON journal_lines
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

-- Entries are immutable except for the single transition posted -> reversed.
CREATE FUNCTION guard_entry_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'journal entries cannot be deleted; post a reversal' USING ERRCODE = 'insufficient_privilege';
    END IF;
    IF OLD.status = 'posted' AND NEW.status = 'reversed'
       AND (NEW.entry_date, NEW.hijri_date, NEW.description, NEW.reference, NEW.reverses_id, NEW.source, NEW.created_by, NEW.created_at)
           IS NOT DISTINCT FROM
           (OLD.entry_date, OLD.hijri_date, OLD.description, OLD.reference, OLD.reverses_id, OLD.source, OLD.created_by, OLD.created_at) THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'journal entries are immutable; post a reversal' USING ERRCODE = 'insufficient_privilege';
END $$;

CREATE TRIGGER journal_entries_guard BEFORE UPDATE OR DELETE ON journal_entries
    FOR EACH ROW EXECUTE FUNCTION guard_entry_update();
CREATE TRIGGER journal_entries_no_truncate BEFORE TRUNCATE ON journal_entries
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

-- Hash-chained audit log (see yusr_domain::audit for the hash definition).
CREATE TABLE audit_log (
    seq          BIGINT PRIMARY KEY CHECK (seq > 0),
    occurred_at  TIMESTAMPTZ NOT NULL,
    actor        TEXT NOT NULL,
    action       TEXT NOT NULL,
    entity       TEXT NOT NULL,
    entity_id    TEXT NOT NULL,
    payload      JSONB NOT NULL,
    prev_hash    BYTEA NOT NULL CHECK (length(prev_hash) = 32),
    hash         BYTEA NOT NULL UNIQUE CHECK (length(hash) = 32)
);
CREATE INDEX audit_log_entity_idx ON audit_log (entity, entity_id);

-- The chain must link: each row's prev_hash is the previous row's hash.
CREATE FUNCTION check_audit_link() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    prev BYTEA;
BEGIN
    IF NEW.seq = 1 THEN
        prev := '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea;
    ELSE
        SELECT hash INTO prev FROM audit_log WHERE seq = NEW.seq - 1;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'audit_log gap before seq %', NEW.seq USING ERRCODE = 'check_violation';
        END IF;
    END IF;
    IF NEW.prev_hash <> prev THEN
        RAISE EXCEPTION 'audit_log seq % does not link to its predecessor', NEW.seq USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER audit_log_link BEFORE INSERT ON audit_log FOR EACH ROW EXECUTE FUNCTION check_audit_link();
CREATE TRIGGER audit_log_worm BEFORE UPDATE OR DELETE ON audit_log FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER audit_log_no_truncate BEFORE TRUNCATE ON audit_log FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TABLE donors (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    is_organization    BOOLEAN NOT NULL DEFAULT FALSE,
    full_name          TEXT NOT NULL CHECK (btrim(full_name) <> ''),
    national_id        TEXT UNIQUE,
    phone              TEXT,
    email              TEXT,
    country            CHAR(2),
    preferred_language TEXT NOT NULL DEFAULT 'ar' CHECK (preferred_language IN ('ar', 'en')),
    notes              TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX donors_name_idx ON donors (lower(full_name));

-- Gap-free receipt numbering per Gregorian year (tax authorities expect no gaps).
CREATE TABLE receipt_counters (
    year      INTEGER PRIMARY KEY,
    last_no   INTEGER NOT NULL
);

CREATE TABLE donations (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    receipt_no        TEXT NOT NULL UNIQUE,
    donor_id          UUID NOT NULL REFERENCES donors (id),
    donation_date     DATE NOT NULL,
    kind              donation_kind NOT NULL,
    donor_designated  BOOLEAN NOT NULL DEFAULT FALSE,
    fund_id           UUID NOT NULL REFERENCES funds (id),
    project_id        UUID REFERENCES projects (id),
    amount            money_amount NOT NULL CHECK (amount > 0),
    currency          currency_code NOT NULL,
    base_amount       money_amount NOT NULL CHECK (base_amount > 0),
    payment_method    TEXT NOT NULL,
    payment_ref       TEXT,
    note              TEXT,
    journal_entry_id  UUID NOT NULL UNIQUE REFERENCES journal_entries (id),
    audit_hash        BYTEA NOT NULL,
    created_by        TEXT NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX donations_donor_idx ON donations (donor_id, donation_date DESC);
CREATE INDEX donations_date_idx ON donations (donation_date);

CREATE TABLE beneficiaries (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_no       BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
    full_name     TEXT NOT NULL CHECK (btrim(full_name) <> ''),
    national_id   TEXT UNIQUE,
    phone         TEXT,
    city          TEXT,
    category      asnaf NOT NULL,
    profile       JSONB NOT NULL,
    score         SMALLINT NOT NULL CHECK (score BETWEEN 0 AND 100),
    priority      TEXT NOT NULL,
    status        case_status NOT NULL DEFAULT 'active',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX beneficiaries_priority_idx ON beneficiaries (status, score DESC);

-- Append-only case history: assessments, visits, decisions.
CREATE TABLE case_events (
    id              BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    beneficiary_id  UUID NOT NULL REFERENCES beneficiaries (id),
    event_type      TEXT NOT NULL,
    note            TEXT NOT NULL,
    actor           TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TRIGGER case_events_worm BEFORE UPDATE OR DELETE ON case_events FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TABLE aid_distributions (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    beneficiary_id    UUID NOT NULL REFERENCES beneficiaries (id),
    kind              aid_kind NOT NULL,
    fund_id           UUID NOT NULL REFERENCES funds (id),
    project_id        UUID REFERENCES projects (id),
    amount            money_amount NOT NULL CHECK (amount > 0),
    currency          currency_code NOT NULL,
    description       TEXT NOT NULL,
    items             JSONB,
    voucher_code      TEXT UNIQUE,
    voucher_status    voucher_status,
    redeemed_at       TIMESTAMPTZ,
    redeemed_by       TEXT,
    journal_entry_id  UUID NOT NULL REFERENCES journal_entries (id),
    distributed_on    DATE NOT NULL,
    created_by        TEXT NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((kind = 'voucher') = (voucher_code IS NOT NULL AND voucher_status IS NOT NULL))
);
CREATE INDEX aid_distributions_beneficiary_idx ON aid_distributions (beneficiary_id, distributed_on DESC);

-- ZATCA invoice chain per EGS unit: the counter (ICV) and previous hash (PIH) link invoices.
CREATE TABLE zatca_invoices (
    id             UUID PRIMARY KEY,
    number         TEXT NOT NULL UNIQUE,
    icv            BIGINT NOT NULL UNIQUE CHECK (icv > 0),
    invoice_hash   TEXT NOT NULL UNIQUE,
    previous_hash  TEXT NOT NULL,
    invoice_type   TEXT NOT NULL,
    xml            TEXT NOT NULL,
    qr             TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'signed' CHECK (status IN ('signed', 'reported', 'cleared', 'rejected')),
    zatca_response JSONB,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE payment_intents (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    gateway       TEXT NOT NULL,
    gateway_ref   TEXT NOT NULL,
    donor_id      UUID NOT NULL REFERENCES donors (id),
    kind          donation_kind NOT NULL,
    fund_id       UUID NOT NULL REFERENCES funds (id),
    amount        money_amount NOT NULL CHECK (amount > 0),
    currency      currency_code NOT NULL,
    status        payment_status NOT NULL DEFAULT 'pending',
    donation_id   UUID UNIQUE REFERENCES donations (id),
    checkout_url  TEXT,
    raw_event     JSONB,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at  TIMESTAMPTZ,
    UNIQUE (gateway, gateway_ref)
);
