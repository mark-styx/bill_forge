/**
 * @billforge/shared-types — cross-app TypeScript contract shared between
 * apps/web and apps/mobile. Migrate duplicated types here one-at-a-time
 * per follow-up to issue #126.
 * Migrated: ApiErrorBody, PaginationMeta, Money, InvoiceLineItem, Invoice,
 * CreateInvoiceInput.
 */

// ---------------------------------------------------------------------------
// Generated types from backend OpenAPI spec (utoipa → openapi-typescript)
// ---------------------------------------------------------------------------
import type { components as GeneratedComponents, paths as GeneratedPaths } from './generated';
export type { components as ApiComponents, paths as ApiPaths } from './generated';

/** Convenience alias for all generated schema types. */
export type Schemas = GeneratedComponents['schemas'];
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

/** Canonical money representation — single source of truth for invoice amounts. */
export interface Money {
  amount: number;
  currency: string;
}

export interface InvoiceLineItem {
  id: string;
  line_number: number;
  description: string;
  quantity?: number;
  unit_price?: Money;
  amount: Money;
  gl_code?: string;
  department?: string;
  project?: string;
}

export interface Invoice {
  id: string;
  tenant_id: string;
  vendor_id?: string;
  vendor_name: string;
  invoice_number: string;
  invoice_date?: string;
  due_date?: string;
  po_number?: string;
  subtotal?: Money;
  tax_amount?: Money;
  total_amount: Money;
  currency: string;
  line_items: InvoiceLineItem[];
  capture_status: string;
  processing_status: string;
  current_queue_id?: string;
  assigned_to?: string;
  document_id: string;
  supporting_documents: string[];
  ocr_confidence?: number;
  categorization_confidence?: number;
  department?: string;
  gl_code?: string;
  cost_center?: string;
  notes?: string;
  tags: string[];
  custom_fields?: Record<string, unknown>;
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateInvoiceInput {
  vendor_name: string;
  invoice_number: string;
  total_amount: Money;
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
