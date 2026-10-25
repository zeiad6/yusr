-- Module 4: Dynamic Zakat Nisab historical rates & manual fallback
CREATE TABLE IF NOT EXISTS zakat_rates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    gold_gram_price NUMERIC(14, 4) NOT NULL,
    silver_gram_price NUMERIC(14, 4) NOT NULL,
    currency VARCHAR(8) NOT NULL DEFAULT 'SAR',
    source VARCHAR(32) NOT NULL DEFAULT 'manual', -- 'manual', 'api_sync', 'default'
    notes TEXT,
    updated_by VARCHAR(64) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed initial standard official rates (approx gold 24k ~290 SAR/g, silver ~3.8 SAR/g)
INSERT INTO zakat_rates (gold_gram_price, silver_gram_price, currency, source, notes, updated_by)
VALUES (295.5000, 3.8500, 'SAR', 'default', 'Initial official baseline rates', 'system');

-- Module 6: Internet-Independent Asynchronous Gateway (Queue Sync Engine)
CREATE TABLE IF NOT EXISTS gateway_transactions_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    gateway VARCHAR(32) NOT NULL, -- 'tap', 'moyasar', 'fawry', 'bank_import'
    external_id VARCHAR(128) NOT NULL,
    idempotency_key VARCHAR(128) NOT NULL UNIQUE,
    amount NUMERIC(14, 2) NOT NULL,
    currency VARCHAR(8) NOT NULL DEFAULT 'SAR',
    donor_name VARCHAR(128),
    donor_phone VARCHAR(32),
    donor_email VARCHAR(128),
    fund_id UUID REFERENCES funds(id),
    status VARCHAR(24) NOT NULL DEFAULT 'staged', -- 'staged', 'verified', 'reconciled', 'rejected'
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    reconciled_entry_id UUID REFERENCES journal_entries(id),
    receipt_id UUID REFERENCES donations(id),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    verified_at TIMESTAMPTZ,
    reconciled_at TIMESTAMPTZ,
    error_message TEXT,
    CONSTRAINT uq_gateway_external_id UNIQUE (gateway, external_id)
);

CREATE INDEX IF NOT EXISTS idx_gateway_queue_status ON gateway_transactions_queue(status);

-- Module 7: Cryptographically Signed Donor Transparency Reports
CREATE TABLE IF NOT EXISTS transparency_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_number VARCHAR(64) NOT NULL UNIQUE,
    title VARCHAR(256) NOT NULL,
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    total_donations NUMERIC(14, 2) NOT NULL,
    total_aid NUMERIC(14, 2) NOT NULL,
    active_cases INTEGER NOT NULL,
    audit_head_hash VARCHAR(64) NOT NULL,
    digital_signature VARCHAR(128) NOT NULL,
    public_verification_token VARCHAR(64) NOT NULL UNIQUE,
    metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_transparency_token ON transparency_reports(public_verification_token);
