/**
 * @billforge/shared-types — cross-app TypeScript contract shared between
 * apps/web and apps/mobile. Migrate duplicated types here one-at-a-time
 * per follow-up to issue #126. Migrated: ApiErrorBody, PaginationMeta.
 */
export interface ApiErrorBody {
  error: {
    code: string;
    message: string;
    details?: unknown;
    field_errors?: Record<string, string[]>;
  };
}

export interface PaginationMeta {
  page: number;
  per_page: number;
  total_items: number;
  total_pages: number;
}

// ---------------------------------------------------------------------------
// Dashboard KPIs (materialized view backed)
// ---------------------------------------------------------------------------

export interface AgingBuckets {
  aging_0_7: number;
  aging_0_7_amount: number;
  aging_8_14: number;
  aging_8_14_amount: number;
  aging_15_30: number;
  aging_15_30_amount: number;
  aging_30_plus: number;
  aging_30_plus_amount: number;
}

export interface VendorSpendEntry {
  vendor_id: string;
  vendor_name: string;
  total_amount: number;
  invoice_count: number;
}

export interface DashboardKpis {
  queue_count: number;
  approved_count: number;
  paid_count: number;
  rejected_count: number;
  aging: AgingBuckets;
  spend_by_vendor: VendorSpendEntry[];
  total_spend_30d: number;
  avg_processing_hours: number;
}
