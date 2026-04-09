-- Persist billing subscriptions so they survive server restarts.
-- One active subscription per tenant (UNIQUE on tenant_id enables upsert).

CREATE TABLE IF NOT EXISTS tenant_subscriptions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL UNIQUE REFERENCES tenants(id) ON DELETE CASCADE,
    plan_id TEXT NOT NULL,
    status TEXT NOT NULL,
    billing_cycle TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    current_period_start TIMESTAMPTZ NOT NULL,
    current_period_end TIMESTAMPTZ NOT NULL,
    canceled_at TIMESTAMPTZ NULL,
    trial_end TIMESTAMPTZ NULL,
    stripe_subscription_id TEXT NULL,
    stripe_customer_id TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tenant_subscriptions_tenant_id ON tenant_subscriptions(tenant_id);
