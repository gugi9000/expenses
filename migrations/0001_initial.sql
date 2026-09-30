-- Timestamps are UTC ISO-8601 text; money is stored in minor units (øre/cents).

CREATE TABLE users (
    id              INTEGER PRIMARY KEY,
    auth_provider   TEXT    NOT NULL CHECK (auth_provider IN ('entra', 'local')),
    entra_oid       TEXT    UNIQUE,
    username        TEXT    NOT NULL,
    display_name    TEXT    NOT NULL,
    email           TEXT,
    password_hash   TEXT,
    role            TEXT    NOT NULL CHECK (role IN ('user', 'admin')),
    disabled        INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_login_at   TEXT,
    UNIQUE (auth_provider, username),
    CHECK (auth_provider <> 'local' OR password_hash IS NOT NULL),
    CHECK (auth_provider <> 'entra' OR entra_oid IS NOT NULL)
);

CREATE TABLE sessions (
    token_hash  TEXT    PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at  TEXT    NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE oidc_pending (
    state          TEXT PRIMARY KEY,
    nonce          TEXT NOT NULL,
    pkce_verifier  TEXT NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE expense_categories (
    id          INTEGER PRIMARY KEY,
    code        TEXT    NOT NULL UNIQUE,
    name_da     TEXT    NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    active      INTEGER NOT NULL DEFAULT 1
);

INSERT INTO expense_categories (code, name_da, sort_order) VALUES
    ('travel',     'Rejse',          10),
    ('transport',  'Transport',      20),
    ('meals',      'Forplejning',    30),
    ('lodging',    'Overnatning',    40),
    ('supplies',   'Kontorartikler', 50),
    ('software',   'Software',       60),
    ('other',      'Andet',          99);

CREATE TABLE expenses (
    id                 INTEGER PRIMARY KEY,
    owner_id           INTEGER NOT NULL REFERENCES users (id),
    kind               TEXT    NOT NULL DEFAULT 'receipt' CHECK (kind IN ('receipt', 'bill', 'invoice')),
    category_id        INTEGER REFERENCES expense_categories (id),
    vendor             TEXT,
    description        TEXT,
    expense_date       TEXT,
    amount_minor       INTEGER,
    currency           TEXT    CHECK (currency IS NULL OR length(currency) = 3),
    fx_rate            TEXT,
    fx_rate_date       TEXT,
    amount_base_minor  INTEGER,
    status             TEXT    NOT NULL DEFAULT 'draft'
                               CHECK (status IN ('draft', 'new', 'used', 'invalid', 'duplicate')),
    deleted_at         TEXT,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX expenses_owner_status ON expenses (owner_id, status);

CREATE TABLE attachments (
    id             INTEGER PRIMARY KEY,
    expense_id     INTEGER NOT NULL REFERENCES expenses (id),
    sha256         TEXT    NOT NULL,
    mime           TEXT    NOT NULL,
    size_bytes     INTEGER NOT NULL,
    original_name  TEXT,
    page_order     INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX attachments_expense ON attachments (expense_id);
CREATE INDEX attachments_sha ON attachments (sha256);

-- ECB publishes rates as units of currency per 1 EUR.
CREATE TABLE fx_rates (
    rate_date     TEXT NOT NULL,
    currency      TEXT NOT NULL,
    rate_per_eur  TEXT NOT NULL,
    PRIMARY KEY (rate_date, currency)
);

CREATE TABLE expense_sheets (
    id                 INTEGER PRIMARY KEY,
    owner_id           INTEGER NOT NULL REFERENCES users (id),
    title              TEXT    NOT NULL,
    status             TEXT    NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'voided')),
    total_base_minor   INTEGER NOT NULL,
    pdf_path           TEXT,
    pdf_sha256         TEXT,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    voided_at          TEXT
);
CREATE INDEX expense_sheets_owner ON expense_sheets (owner_id);

CREATE TABLE expense_sheet_items (
    sheet_id           INTEGER NOT NULL REFERENCES expense_sheets (id),
    expense_id         INTEGER NOT NULL REFERENCES expenses (id),
    -- 1 while the sheet is active, NULL once voided, so the unique index only covers active sheets.
    active             INTEGER DEFAULT 1 CHECK (active IS NULL OR active = 1),
    kind               TEXT    NOT NULL,
    category_name      TEXT,
    vendor             TEXT,
    description        TEXT,
    expense_date       TEXT    NOT NULL,
    amount_minor       INTEGER NOT NULL,
    currency           TEXT    NOT NULL,
    fx_rate            TEXT    NOT NULL,
    amount_base_minor  INTEGER NOT NULL,
    PRIMARY KEY (sheet_id, expense_id)
);
CREATE UNIQUE INDEX expense_sheet_items_one_active ON expense_sheet_items (expense_id, active);

CREATE TABLE audit_log (
    id             INTEGER PRIMARY KEY,
    at             TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    actor_user_id  INTEGER REFERENCES users (id),
    action         TEXT    NOT NULL,
    entity_type    TEXT,
    entity_id      TEXT,
    details        TEXT,
    ip             TEXT
);
CREATE INDEX audit_log_at ON audit_log (at);
CREATE INDEX audit_log_actor ON audit_log (actor_user_id, at);

CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;
CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;
