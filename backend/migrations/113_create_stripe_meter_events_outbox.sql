-- Durable outbox for Stripe Billing Meter Events.
-- When a meter send fails, a row is persisted here so a retry path exists.
-- On success a row with status='sent' is stored for audit / idempotency.

CREATE TABLE IF NOT EXISTS stripe_meter_events (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL,
    invoice_id          UUID NOT NULL,
    event_name          TEXT NOT NULL,
    stripe_customer_id  TEXT NOT NULL,
    payload             JSONB NOT NULL,
    status              TEXT NOT NULL DEFAULT 'pending',  -- pending|sent|failed
    attempts            INT NOT NULL DEFAULT 0,
    last_error          TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at             TIMESTAMPTZ NULL,

    -- Prevent double-emit for the same invoice+event
    UNIQUE (invoice_id, event_name)
);

CREATE INDEX IF NOT EXISTS idx_stripe_meter_events_tenant_status
    ON stripe_meter_events (tenant_id, status);
