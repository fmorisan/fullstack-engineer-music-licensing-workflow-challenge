-- notification_service initial schema (ADR-006).

-- Org-level inbox rows; recipient_user_id is reserved for future direct
-- (user-targeted) notifications. One row per source event: the unique
-- index is the at-least-once idempotency anchor.
CREATE TABLE notifications (
    id UUID PRIMARY KEY,
    recipient_user_id UUID,
    recipient_org_id UUID,
    type TEXT NOT NULL CHECK (type IN ('OFFER_RECEIVED', 'COUNTER_OFFER_RECEIVED', 'OFFER_ACCEPTED', 'OFFER_REJECTED')),
    payload JSONB NOT NULL,
    source_event_id UUID NOT NULL UNIQUE,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (recipient_user_id IS NOT NULL OR recipient_org_id IS NOT NULL)
);

CREATE INDEX notifications_org_created_idx ON notifications (recipient_org_id, created_at DESC);
CREATE INDEX notifications_user_created_idx ON notifications (recipient_user_id, created_at DESC);
