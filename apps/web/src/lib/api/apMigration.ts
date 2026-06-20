import { api } from '@/lib/api';

export type ApMigrationEntityType =
  | 'vendor'
  | 'invoice'
  | 'approval_workflow'
  | 'gl_mapping'
  | 'approver'
  | 'document';

export type ApMigrationTargetAction = 'create' | 'update' | 'skip';

export interface ApMigrationPreviewRow {
  id: string;
  entity_type: ApMigrationEntityType;
  source_payload: Record<string, string>;
  target_action: ApMigrationTargetAction;
  target_match_id: string | null;
  conflict_reason: string | null;
}

export interface ApMigrationBundleSummary {
  id: string;
  source: 'bill' | 'coupa';
  status: 'uploaded' | 'parsed' | 'previewed' | 'committed' | 'failed';
  original_filename: string;
  uploaded_at: string;
  error_text: string | null;
}

export interface ApMigrationPreview {
  bundle: ApMigrationBundleSummary;
  entities: {
    vendors: ApMigrationPreviewRow[];
    invoices: ApMigrationPreviewRow[];
    approval_workflows: ApMigrationPreviewRow[];
    gl_mappings: ApMigrationPreviewRow[];
    approvers: ApMigrationPreviewRow[];
    documents: ApMigrationPreviewRow[];
  };
}

export interface ApMigrationUploadResponse {
  bundle_id: string;
  source: 'bill' | 'coupa';
  status: string;
  parse_errors: string[];
}

export interface ApMigrationCommitResponse {
  bundle_id: string;
  status: string;
  vendors_created: number;
  vendors_updated: number;
  invoices_created: number;
  invoices_updated: number;
  approval_workflows_created: number;
  gl_mappings_created: number;
  gl_mappings_updated: number;
  approvers_created: number;
  approvers_updated: number;
  documents_created: number;
  skipped: number;
}

export const apMigrationApi = {
  uploadBundle: (file: File) => {
    const form = new FormData();
    form.append('file', file);
    return api.upload<ApMigrationUploadResponse>('/api/v1/migrate/ap/bundle', form);
  },

  getPreview: (bundleId: string) =>
    api.get<ApMigrationPreview>(`/api/v1/migrate/ap/bundle/${bundleId}/preview`),

  commit: (bundleId: string) =>
    api.post<ApMigrationCommitResponse>(`/api/v1/migrate/ap/bundle/${bundleId}/commit`),

  cancel: (bundleId: string) =>
    api.post<{ bundle_id: string; status: string }>(
      `/api/v1/migrate/ap/bundle/${bundleId}/cancel`,
    ),
};
