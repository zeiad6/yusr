-- Replay protection for money-moving requests, recurring sponsorships (كفالات) and volunteers.

-- One row per (caller, endpoint, client key): a retried or double-submitted request gets the
-- stored response instead of running twice.
CREATE TABLE idempotency_keys (
    scope        TEXT NOT NULL,
    key          TEXT NOT NULL CHECK (key ~ '^[A-Za-z0-9_-]{8,100}$'),
    status_code  SMALLINT,
    response     JSONB,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (scope, key)
);
CREATE INDEX idempotency_keys_created_idx ON idempotency_keys (created_at);

CREATE TYPE sponsorship_program AS ENUM ('orphan', 'family', 'student', 'patient', 'elderly', 'other');
CREATE TYPE sponsorship_status AS ENUM ('active', 'paused', 'ended');

CREATE TABLE sponsorships (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sponsorship_no   BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
    donor_id         UUID NOT NULL REFERENCES donors (id),
    beneficiary_id   UUID REFERENCES beneficiaries (id),
    program          sponsorship_program NOT NULL,
    kind             donation_kind NOT NULL DEFAULT 'sadaqah',
    fund_id          UUID REFERENCES funds (id),
    monthly_amount   money_amount NOT NULL CHECK (monthly_amount > 0),
    currency         currency_code NOT NULL,
    start_date       DATE NOT NULL,
    end_date         DATE,
    status           sponsorship_status NOT NULL DEFAULT 'active',
    notes            TEXT,
    created_by       TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (end_date IS NULL OR end_date >= start_date)
);
CREATE INDEX sponsorships_donor_idx ON sponsorships (donor_id);
CREATE INDEX sponsorships_beneficiary_idx ON sponsorships (beneficiary_id);

-- A sponsorship payment is an ordinary donation (receipt, journal entry, audit hash) tagged
-- with the months it covers.
CREATE TABLE sponsorship_payments (
    sponsorship_id  UUID NOT NULL REFERENCES sponsorships (id),
    donation_id     UUID NOT NULL UNIQUE REFERENCES donations (id),
    months          SMALLINT NOT NULL CHECK (months BETWEEN 1 AND 60),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (sponsorship_id, donation_id)
);
CREATE TRIGGER sponsorship_payments_worm BEFORE UPDATE OR DELETE ON sponsorship_payments
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();

CREATE TYPE volunteer_status AS ENUM ('active', 'inactive');

CREATE TABLE volunteers (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    volunteer_no  BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
    full_name     TEXT NOT NULL CHECK (btrim(full_name) <> ''),
    national_id   TEXT UNIQUE,
    phone         TEXT,
    email         TEXT,
    city          TEXT,
    skills        TEXT[] NOT NULL DEFAULT '{}',
    status        volunteer_status NOT NULL DEFAULT 'active',
    joined_on     DATE NOT NULL,
    notes         TEXT,
    created_by    TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE volunteer_hours (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    volunteer_id  UUID NOT NULL REFERENCES volunteers (id),
    activity      TEXT NOT NULL CHECK (btrim(activity) <> ''),
    project_id    UUID REFERENCES projects (id),
    worked_on     DATE NOT NULL,
    hours         NUMERIC(5, 2) NOT NULL CHECK (hours > 0 AND hours <= 24),
    notes         TEXT,
    created_by    TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX volunteer_hours_volunteer_idx ON volunteer_hours (volunteer_id, worked_on DESC);
-- Logged hours feed statutory reports: corrected by a new entry, never edited.
CREATE TRIGGER volunteer_hours_worm BEFORE UPDATE OR DELETE ON volunteer_hours
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
