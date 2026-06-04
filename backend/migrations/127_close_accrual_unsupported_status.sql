-- 127: Add 'unsupported' to erp_post_status CHECK constraint on close_accrual_entries
-- See issue #339: QBO journal entry posting is not yet implemented, so the close flow
-- needs to distinguish "unsupported ERP posting" from "pending" / "posted" / "failed".

ALTER TABLE close_accrual_entries
    DROP CONSTRAINT close_accrual_entries_erp_post_status_check;

ALTER TABLE close_accrual_entries
    ADD CONSTRAINT close_accrual_entries_erp_post_status_check
    CHECK (erp_post_status IN ('pending', 'posted', 'failed', 'unsupported'));
