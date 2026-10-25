-- Electronic archive (documents with attached files) and outgoing correspondence
-- (official letters and telegrams).

CREATE TYPE document_category AS ENUM (
    'incoming', 'outgoing', 'contract', 'financial', 'report', 'beneficiary', 'donor', 'hr', 'legal', 'other'
);

CREATE TABLE archive_counters (
    year     INTEGER PRIMARY KEY,
    last_no  INTEGER NOT NULL
);

CREATE TABLE documents (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    doc_no         TEXT NOT NULL UNIQUE,
    title          TEXT NOT NULL CHECK (btrim(title) <> ''),
    category       document_category NOT NULL,
    reference_no   TEXT,
    document_date  DATE NOT NULL,
    -- Sender for incoming mail, recipient for outgoing, counterparty for contracts.
    party          TEXT,
    description    TEXT,
    tags           TEXT[] NOT NULL DEFAULT '{}',
    voided_at      TIMESTAMPTZ,
    voided_by      TEXT,
    void_reason    TEXT,
    created_by     TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((voided_at IS NULL) = (void_reason IS NULL))
);
CREATE INDEX documents_date_idx ON documents (document_date DESC);
CREATE INDEX documents_category_idx ON documents (category, document_date DESC);

CREATE TABLE document_files (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id   UUID NOT NULL REFERENCES documents (id),
    file_name     TEXT NOT NULL CHECK (btrim(file_name) <> ''),
    content_type  TEXT NOT NULL,
    size_bytes    BIGINT NOT NULL CHECK (size_bytes > 0),
    sha256        TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    data          BYTEA NOT NULL,
    uploaded_by   TEXT NOT NULL,
    uploaded_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (octet_length(data) = size_bytes)
);
CREATE INDEX document_files_document_idx ON document_files (document_id);

-- Archived files are evidence: never rewritten or removed. A wrong document is voided instead.
CREATE TRIGGER document_files_worm BEFORE UPDATE OR DELETE ON document_files
    FOR EACH ROW EXECUTE FUNCTION reject_mutation();
CREATE TRIGGER document_files_no_truncate BEFORE TRUNCATE ON document_files
    FOR EACH STATEMENT EXECUTE FUNCTION reject_mutation();

CREATE TYPE letter_kind AS ENUM ('official', 'congratulation', 'condolence', 'thanks', 'invitation', 'circular');

-- Numbered per Hijri year, as outgoing registers (سجل الصادر) are kept.
CREATE TABLE letter_counters (
    hijri_year  INTEGER PRIMARY KEY,
    last_no     INTEGER NOT NULL
);

CREATE TABLE letters (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ref_no           TEXT UNIQUE,
    kind             letter_kind NOT NULL,
    recipient_name   TEXT NOT NULL CHECK (btrim(recipient_name) <> ''),
    recipient_title  TEXT,
    recipient_org    TEXT,
    subject          TEXT NOT NULL CHECK (btrim(subject) <> ''),
    body             TEXT NOT NULL CHECK (btrim(body) <> ''),
    closing          TEXT,
    signatory_name   TEXT NOT NULL CHECK (btrim(signatory_name) <> ''),
    signatory_title  TEXT,
    attachments      TEXT,
    cc               TEXT,
    letter_date      DATE NOT NULL,
    status           TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'issued')),
    issued_at        TIMESTAMPTZ,
    issued_by        TEXT,
    archive_id       UUID REFERENCES documents (id),
    created_by       TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'issued') = (ref_no IS NOT NULL AND issued_at IS NOT NULL))
);
CREATE INDEX letters_status_idx ON letters (status, letter_date DESC);

-- An issued letter is a sent document: only linking its archived copy is allowed afterwards.
CREATE FUNCTION protect_issued_letter() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.status = 'issued' THEN
            RAISE EXCEPTION 'issued letter % cannot be deleted', OLD.ref_no USING ERRCODE = 'insufficient_privilege';
        END IF;
        RETURN OLD;
    END IF;
    IF OLD.status = 'issued' THEN
        -- The only permitted change: attaching the archived copy, once.
        IF OLD.archive_id IS NOT NULL OR NEW.archive_id IS NULL
           OR ROW(NEW.ref_no, NEW.kind, NEW.recipient_name, NEW.recipient_title, NEW.recipient_org, NEW.subject, NEW.body,
                  NEW.closing, NEW.signatory_name, NEW.signatory_title, NEW.attachments, NEW.cc, NEW.letter_date,
                  NEW.status, NEW.issued_at, NEW.issued_by)
              IS DISTINCT FROM
              ROW(OLD.ref_no, OLD.kind, OLD.recipient_name, OLD.recipient_title, OLD.recipient_org, OLD.subject, OLD.body,
                  OLD.closing, OLD.signatory_name, OLD.signatory_title, OLD.attachments, OLD.cc, OLD.letter_date,
                  OLD.status, OLD.issued_at, OLD.issued_by) THEN
            RAISE EXCEPTION 'issued letter % is read-only', OLD.ref_no USING ERRCODE = 'insufficient_privilege';
        END IF;
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER letters_protect_issued BEFORE UPDATE OR DELETE ON letters
    FOR EACH ROW EXECUTE FUNCTION protect_issued_letter();
