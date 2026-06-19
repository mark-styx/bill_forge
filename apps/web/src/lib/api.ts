// Use empty string for relative URLs so requests go through Next.js rewrite proxy.
// This ensures LAN/remote access works (browser won't try to hit localhost:8080).
const API_BASE_URL = typeof window !== 'undefined' ? '' : (process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080');

import type { ApiErrorBody, PaginationMeta, DashboardKpis, Money, Invoice, InvoiceLineItem, CreateInvoiceInput, Schemas } from '@billforge/shared-types';
export type { ApiErrorBody, PaginationMeta, DashboardKpis, Money, Invoice, InvoiceLineItem, CreateInvoiceInput } from '@billforge/shared-types';

export class ApiClientError extends Error {
  status: number;
  code: string;
  body: ApiErrorBody | null;
  fieldErrors: Record<string, string[]> | undefined;

  constructor(status: number, body: ApiErrorBody | null) {
    super(body?.error?.message ?? `API error ${status}`);
    this.name = 'ApiClientError';
    this.status = status;
    this.code = body?.error?.code ?? 'UNKNOWN';
    this.body = body;
    this.fieldErrors = body?.error?.field_errors ?? undefined;
  }
}

class ApiClient {
  private baseUrl: string;
  private token: string | null = null;
  private refreshToken: string | null = null;
  private onTokenRefresh?: (accessToken: string, refreshToken: string) => void;
  private onLogout?: () => void;
  private isRefreshing = false;
  private refreshPromise: Promise<boolean> | null = null;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  setToken(token: string | null) {
    this.token = token;
  }

  setRefreshToken(token: string | null) {
    this.refreshToken = token;
  }

  setTokenRefreshCallback(callback: (accessToken: string, refreshToken: string) => void) {
    this.onTokenRefresh = callback;
  }

  setLogoutCallback(callback: () => void) {
    this.onLogout = callback;
  }

  private async refreshAccessToken(): Promise<boolean> {
    // Prevent multiple concurrent refresh requests
    if (this.isRefreshing && this.refreshPromise) {
      return this.refreshPromise;
    }

    if (!this.refreshToken) return false;

    this.isRefreshing = true;
    this.refreshPromise = this.doRefresh();

    try {
      return await this.refreshPromise;
    } finally {
      this.isRefreshing = false;
      this.refreshPromise = null;
    }
  }

  private async doRefresh(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}/api/v1/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: this.refreshToken }),
      });

      if (!response.ok) {
        console.error('Token refresh failed:', response.status, response.statusText);
        return false;
      }

      const data = await response.json();
      this.token = data.access_token;
      this.refreshToken = data.refresh_token;

      if (this.onTokenRefresh) {
        this.onTokenRefresh(data.access_token, data.refresh_token);
      }

      return true;
    } catch (error) {
      console.error('Token refresh error:', error);
      return false;
    }
  }

  private async executeRequest(
    path: string,
    method: string,
    headers: HeadersInit,
    body?: unknown,
    options?: RequestInit
  ): Promise<Response> {
    return fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
      ...options,
    });
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    options?: RequestInit
  ): Promise<T> {
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
    };

    let response = await this.executeRequest(path, method, headers, body, options);

    // Try to refresh token on 401
    if (response.status === 401 && this.refreshToken) {
      const refreshed = await this.refreshAccessToken();
      if (refreshed) {
        // Retry with new token
        const newHeaders: HeadersInit = {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${this.token}`,
        };
        response = await this.executeRequest(path, method, newHeaders, body, options);
      } else {
        // Refresh failed, logout
        if (this.onLogout) {
          this.onLogout();
        }
        throw new ApiClientError(401, { error: { code: 'SESSION_EXPIRED', message: 'Session expired. Please login again.' } });
      }
    }

    if (!response.ok) {
      let errorBody: ApiErrorBody | null = null;
      try {
        errorBody = await response.json();
      } catch {
        // non-JSON error body
      }
      throw new ApiClientError(response.status, errorBody);
    }

    // Handle empty responses
    const text = await response.text();
    return text ? JSON.parse(text) : (null as unknown as T);
  }

  async get<T>(path: string, params?: Record<string, string | number | undefined>): Promise<T> {
    if (params) {
      const searchParams = new URLSearchParams();
      for (const [key, value] of Object.entries(params)) {
        if (value !== undefined) searchParams.set(key, String(value));
      }
      const qs = searchParams.toString();
      if (qs) return this.request<T>('GET', `${path}?${qs}`);
    }
    return this.request<T>('GET', path);
  }

  async post<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('POST', path, body);
  }

  async put<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('PUT', path, body);
  }

  async patch<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('PATCH', path, body);
  }

  async delete<T>(path: string): Promise<T> {
    return this.request<T>('DELETE', path);
  }

  async upload<T>(path: string, formData: FormData): Promise<T> {
    const headers: HeadersInit = {
      ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
    };

    const response = await fetch(`${this.baseUrl}${path}`, {
      method: 'POST',
      headers,
      body: formData,
    });

    if (!response.ok) {
      let errorBody: ApiErrorBody | null = null;
      try {
        errorBody = await response.json();
      } catch {
        // non-JSON error body
      }
      throw new ApiClientError(response.status, errorBody);
    }

    return response.json();
  }

  async downloadBlob(path: string): Promise<Blob> {
    const headers: HeadersInit = {
      ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
    };

    const response = await fetch(`${this.baseUrl}${path}`, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      throw new Error('Failed to download document');
    }

    return response.blob();
  }
}

export const api = new ApiClient(API_BASE_URL);

// Auth API
export const authApi = {
  login: (data: { tenant_id: string; email: string; password: string }) =>
    api.post<AuthResponseData>('/api/v1/auth/login', data),

  register: (data: {
    tenant_id: string;
    email: string;
    password: string;
    name: string;
  }) =>
    api.post<AuthResponseData>('/api/v1/auth/register', data),

  provision: (data: {
    company_name: string;
    admin_email: string;
    admin_password: string;
    admin_name: string;
    timezone?: string;
    default_currency?: string;
    ocr_provider?: string;
    local_ocr_required?: boolean;
    industry?: string;
  }) =>
    api.post<AuthResponseData>('/api/v1/auth/provision', data),

  refresh: (refresh_token: string) =>
    api.post<AuthResponseData>('/api/v1/auth/refresh', { refresh_token }),

  logout: () => api.post('/api/v1/auth/logout'),

  me: () =>
    api.post<{
      user_id: string;
      tenant_id: string;
      email: string;
      roles: string[];
    }>('/api/v1/auth/me'),
};

// Auth response type
export interface AuthResponseData {
  access_token: string;
  refresh_token: string;
  user: {
    id: string;
    tenant_id: string;
    email: string;
    name: string;
    roles: string[];
  };
  tenant: {
    id: string;
    name: string;
    enabled_modules: string[];
    settings: {
      logo_url?: string;
      primary_color?: string;
      company_name: string;
      timezone: string;
      default_currency: string;
      ocr_provider?: string | null;
    };
  };
}

// Sandbox/Persona API
export const sandboxApi = {
  // List all available personas
  listPersonas: () => api.get<PersonaInfo[]>('/api/v1/sandbox/personas'),

  // Get current persona for tenant
  getCurrentPersona: () => api.get<CurrentPersonaResponse>('/api/v1/sandbox/personas/current'),

  // Switch to a different persona
  switchPersona: (personaId: string) =>
    api.post<SwitchPersonaResponse>('/api/v1/sandbox/personas/switch', { persona_id: personaId }),

  // Get full tenant context
  getTenantContext: () => api.get<TenantContextResponse>('/api/v1/sandbox/context'),
};

// Persona types
export interface PersonaInfo {
  id: string;
  name: string;
  description: string;
  modules: ModuleInfo[];
  roles: RoleInfo[];
  reporting_sections: string[];
}

export interface ModuleInfo {
  id: string;
  name: string;
  enabled: boolean;
}

export interface RoleInfo {
  id: string;
  name: string;
  available: boolean;
}

export interface CurrentPersonaResponse {
  persona: PersonaInfo;
  tenant_id: string;
  tenant_name: string;
}

export interface SwitchPersonaResponse {
  success: boolean;
  persona: PersonaInfo;
  message: string;
}

export interface TenantContextResponse {
  tenant_id: string;
  tenant_name: string;
  persona: PersonaInfo;
  enabled_modules: { id: string; name: string; enabled: boolean }[];
  available_roles: { id: string; name: string; available: boolean; description: string }[];
  reporting_sections: string[];
  settings: {
    logo_url?: string;
    primary_color?: string;
    company_name: string;
    timezone: string;
    default_currency: string;
  };
}

export type ImplementationPhaseStatus = 'not_started' | 'in_progress' | 'complete';
export type ImplementationErpProvider = 'quickbooks' | 'xero';

export interface ImplementationErpSubItems {
  chart_of_accounts: boolean;
  vendors: boolean;
  open_pos: boolean;
}

export interface ImplementationErpSyncSummary {
  provider: ImplementationErpProvider;
  connected: boolean;
  account_mappings: number;
  vendor_mappings: number;
  open_purchase_orders: number;
  message: string;
  synced_at: string;
}

export interface ImplementationStatus {
  started_at: string;
  day_number: number;
  percent_complete: number;
  phases: {
    erp: {
      status: ImplementationPhaseStatus;
      provider?: ImplementationErpProvider | null;
      sub_items: ImplementationErpSubItems;
      last_sync?: ImplementationErpSyncSummary | null;
      last_error?: string | null;
    };
    approvals: {
      status: ImplementationPhaseStatus;
      template?: string | null;
      template_id?: string | null;
    };
    ocr: {
      status: ImplementationPhaseStatus;
      count: number;
      sample_invoice_ids: string[];
      measured_accuracy?: number | null;
      accuracy_threshold: number;
      total_extractions: number;
      sufficient_sample: boolean;
    };
    configuration: ImplementationConfigurationPhase;
    go_live: {
      status: ImplementationPhaseStatus;
      checks: ImplementationGoLiveChecks;
    };
  };
}

export interface ImplementationGoLiveChecks {
  confirm_cutover_date: boolean;
  forwarding_email_verified: boolean;
  sample_invoice_routed: boolean;
  notifications_acknowledged: boolean;
  privacy_mode_confirmed: boolean;
}

export interface ImplementationPrivacyModeConfig {
  enabled: boolean;
  scope: string | null;
  confirmed_at: string | null;
}

export interface ImplementationCaptureChannelsConfig {
  email_forwarding: {
    address: string;
    verified_at: string | null;
  };
  manual_upload_enabled: boolean;
  erp_sync_enabled: boolean;
}

export interface ImplementationModuleEntitlement {
  module_key: string;
  enabled: boolean;
}

export interface ImplementationNotificationApprovalsConfig {
  ap_team_distribution: string[];
  escalation_distribution: string[];
  approved_at: string | null;
}

export interface ImplementationConfigurationSection {
  privacy_mode: ImplementationPrivacyModeConfig;
  capture_channels: ImplementationCaptureChannelsConfig;
  module_entitlements: ImplementationModuleEntitlement[];
  notification_approvals: ImplementationNotificationApprovalsConfig;
}

export interface ImplementationConfigurationPhase {
  status: ImplementationPhaseStatus;
  configuration: ImplementationConfigurationSection;
}

export interface ImplementationSampleUploadResponse {
  uploaded: {
    invoice_id: string;
    document_id: string;
    message: string;
  }[];
  status: ImplementationStatus;
}

export const implementationApi = {
  status: () => api.get<ImplementationStatus>('/api/v1/implementation/status'),
  syncErp: (provider: ImplementationErpProvider) =>
    api.post<ImplementationStatus>('/api/v1/implementation/erp/sync', { provider }),
  updateErpSubItems: (subItems: ImplementationErpSubItems) =>
    api.patch<ImplementationStatus>('/api/v1/implementation/erp/sub-items', { sub_items: subItems }),
  selectApprovalTemplate: (template: string) =>
    api.post<ImplementationStatus>('/api/v1/implementation/approval-template', { template }),
  uploadSampleInvoices: (files: File[]) => {
    const formData = new FormData();
    for (const file of files) formData.append('files', file);
    return api.upload<ImplementationSampleUploadResponse>('/api/v1/implementation/sample-invoices', formData);
  },
  updateChecklist: (checks: ImplementationGoLiveChecks) =>
    api.patch<ImplementationStatus>('/api/v1/implementation/checklist', { checks }),
  updatePrivacyMode: (enabled: boolean, scope?: string) =>
    api.put<ImplementationStatus>('/api/v1/implementation/configuration/privacy-mode', { enabled, scope }),
  updateCaptureChannels: (params: {
    email_forwarding_address?: string;
    manual_upload_enabled?: boolean;
    erp_sync_enabled?: boolean;
  }) => api.put<ImplementationStatus>('/api/v1/implementation/configuration/capture-channels', params),
  verifyEmailForwarding: (evidence?: string) =>
    api.post<ImplementationStatus>('/api/v1/implementation/configuration/capture-channels/email/verify', { evidence }),
  ackModuleEntitlements: (entitlements: ImplementationModuleEntitlement[]) =>
    api.put<ImplementationStatus>('/api/v1/implementation/configuration/module-entitlements/ack', { entitlements }),
  updateNotificationApprovals: (apTeamDistribution: string[], escalationDistribution: string[]) =>
    api.put<ImplementationStatus>('/api/v1/implementation/configuration/notification-approvals', {
      ap_team_distribution: apTeamDistribution,
      escalation_distribution: escalationDistribution,
    }),
};

// Invoices API
export const invoicesApi = {
  list: (params?: {
    page?: number;
    per_page?: number;
    capture_status?: string;
    processing_status?: string;
    vendor_id?: string;
    search?: string;
    min_ocr_confidence?: number;
    max_ocr_confidence?: number;
    ocr_exception_status?: string;
  }) => {
    const qs = new URLSearchParams();
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        if (v !== undefined) qs.set(k, String(v));
      }
    }
    return api.get<{
      data: Invoice[];
      pagination: PaginationMeta;
    }>(`/api/v1/invoices?${qs}`);
  },

  get: (id: string) => api.get<Invoice>(`/api/v1/invoices/${id}`),

  create: (data: CreateInvoiceInput, options?: { force?: boolean }) => {
    const params = options?.force ? '?force=true' : '';
    return api.post<CreateInvoiceResponse>(`/api/v1/invoices${params}`, data);
  },

  update: (id: string, data: Partial<Invoice>) =>
    api.put<Invoice>(`/api/v1/invoices/${id}`, data),

  delete: (id: string) => api.delete(`/api/v1/invoices/${id}`),

  upload: (file: File) => {
    const formData = new FormData();
    formData.append('file', file);
    return api.upload<{
      invoice_id: string;
      document_id: string;
      message: string;
      potential_duplicates: DuplicateMatch[];
    }>('/api/v1/invoices/upload', formData);
  },

  submitForProcessing: (id: string) =>
    api.post(`/api/v1/invoices/${id}/submit`),

  resolveOcrException: (id: string, action: 'approve' | 'reject') =>
    api.post<{ id: string; ocr_exception_status: string }>(`/api/v1/invoices/${id}/ocr-exception/resolve`, { action }),

  exportCsv: () => api.get<Blob>('/api/v1/export/invoices/csv'),
};

// Vendors API
export const vendorsApi = {
  list: (params?: { page?: number; per_page?: number; search?: string }) =>
    api.get<{
      data: Vendor[];
      pagination: PaginationMeta;
    }>(`/api/v1/vendors?${new URLSearchParams(params as any)}`),

  get: (id: string) => api.get<Vendor>(`/api/v1/vendors/${id}`),

  create: (data: CreateVendorInput) =>
    api.post<Vendor>('/api/v1/vendors', data),

  update: (id: string, data: Partial<Vendor>) =>
    api.put<Vendor>(`/api/v1/vendors/${id}`, data),

  delete: (id: string) => api.delete(`/api/v1/vendors/${id}`),

  importCsv: (file: File) => {
    const formData = new FormData();
    formData.append('file', file);
    return api.upload<ImportVendorsResult>('/api/v1/vendors/import', formData);
  },
};

// Workflows API
export const workflowsApi = {
  // Workflow Rules
  listRules: () => api.get<WorkflowRule[]>('/api/v1/workflows/rules'),
  createRule: (data: CreateWorkflowRuleInput) =>
    api.post<WorkflowRule>('/api/v1/workflows/rules', data),
  getRule: (id: string) => api.get<WorkflowRule>(`/api/v1/workflows/rules/${id}`),
  updateRule: (id: string, data: CreateWorkflowRuleInput) =>
    api.put<WorkflowRule>(`/api/v1/workflows/rules/${id}`, data),
  deleteRule: (id: string) => api.delete(`/api/v1/workflows/rules/${id}`),

  // Work Queues
  listQueues: () => api.get<WorkQueue[]>('/api/v1/workflows/queues'),
  getQueue: (id: string) => api.get<WorkQueue>(`/api/v1/workflows/queues/${id}`),
  createQueue: (data: CreateWorkQueueInput) =>
    api.post<WorkQueue>('/api/v1/workflows/queues', data),
  updateQueue: (id: string, data: CreateWorkQueueInput) =>
    api.put<WorkQueue>(`/api/v1/workflows/queues/${id}`, data),
  deleteQueue: (id: string) => api.delete(`/api/v1/workflows/queues/${id}`),
  listQueueItems: (queueId: string) =>
    api.get<QueueItem[]>(`/api/v1/workflows/queues/${queueId}/items`),
  claimQueueItem: (queueId: string, itemId: string) =>
    api.post<QueueItem>(`/api/v1/workflows/queues/${queueId}/items/${itemId}/claim`),
  completeQueueItem: (queueId: string, itemId: string, action: string) =>
    api.post(`/api/v1/workflows/queues/${queueId}/items/${itemId}/complete`, { action }),

  // Inbox
  listInboxItems: (params?: { page?: number; per_page?: number }) =>
    api.get<{ data: InboxItem[]; pagination: { page: number; per_page: number; total_items: number; total_pages: number } }>('/api/v1/workflows/inbox', params as Record<string, string | number | undefined>),

  // Assignment Rules
  listAssignmentRules: () => api.get<AssignmentRule[]>('/api/v1/workflows/assignment-rules'),
  createAssignmentRule: (data: CreateAssignmentRuleInput) =>
    api.post<AssignmentRule>('/api/v1/workflows/assignment-rules', data),
  getAssignmentRule: (id: string) => api.get<AssignmentRule>(`/api/v1/workflows/assignment-rules/${id}`),
  updateAssignmentRule: (id: string, data: CreateAssignmentRuleInput) =>
    api.put<AssignmentRule>(`/api/v1/workflows/assignment-rules/${id}`, data),
  deleteAssignmentRule: (id: string) => api.delete(`/api/v1/workflows/assignment-rules/${id}`),

  // Approvals
  listPendingApprovals: () =>
    api.get<ApprovalRequest[]>('/api/v1/workflows/approvals/pending'),
  getApproval: (id: string) => api.get<ApprovalRequest>(`/api/v1/workflows/approvals/${id}`),
  approve: (id: string, comments?: string) =>
    api.post(`/api/v1/workflows/approvals/${id}/approve`, { comments }),
  reject: (id: string, comments?: string) =>
    api.post(`/api/v1/workflows/approvals/${id}/reject`, { comments }),
  getApprovalLink: (id: string) =>
    api.post<ApprovalLinkResponse>(`/api/v1/workflows/approvals/${id}/approval-link`),
  resendApprovalEmail: (id: string) =>
    api.post<ResendApprovalEmailResponse>(`/api/v1/workflows/approvals/${id}/resend-approval-email`),

  // Bulk Operations
  bulkOperation: (data: BulkOperationInput) =>
    api.post<BulkOperationResult>('/api/v1/workflows/bulk', data),

  // Invoice Actions
  putOnHold: (invoiceId: string, reason: string) =>
    api.post(`/api/v1/workflows/invoices/${invoiceId}/hold`, { reason }),
  releaseFromHold: (invoiceId: string) =>
    api.post(`/api/v1/workflows/invoices/${invoiceId}/release`),
  voidInvoice: (invoiceId: string) =>
    api.post(`/api/v1/workflows/invoices/${invoiceId}/void`),
  markReadyForPayment: (invoiceId: string) =>
    api.post(`/api/v1/workflows/invoices/${invoiceId}/ready-for-payment`),
  moveToQueue: (invoiceId: string, queueId: string, assignTo?: string) =>
    api.post(`/api/v1/workflows/invoices/${invoiceId}/move-to-queue`, { queue_id: queueId, assign_to: assignTo }),

  // Workflow Templates
  listTemplates: () => api.get<WorkflowTemplate[]>('/api/v1/workflows/templates'),
  getTemplate: (id: string) => api.get<WorkflowTemplate>(`/api/v1/workflows/templates/${id}`),
  createTemplate: (data: CreateWorkflowTemplateInput) =>
    api.post<WorkflowTemplate>('/api/v1/workflows/templates', data),
  updateTemplate: (id: string, data: CreateWorkflowTemplateInput) =>
    api.put<WorkflowTemplate>(`/api/v1/workflows/templates/${id}`, data),
  deleteTemplate: (id: string) => api.delete(`/api/v1/workflows/templates/${id}`),
  activateTemplate: (id: string) =>
    api.post(`/api/v1/workflows/templates/${id}/activate`),
  deactivateTemplate: (id: string) =>
    api.post(`/api/v1/workflows/templates/${id}/deactivate`),

  // Approval Delegations
  listDelegations: () => api.get<ApprovalDelegation[]>('/api/v1/workflows/delegations'),
  getDelegation: (id: string) => api.get<ApprovalDelegation>(`/api/v1/workflows/delegations/${id}`),
  createDelegation: (data: CreateApprovalDelegationInput) =>
    api.post<ApprovalDelegation>('/api/v1/workflows/delegations', data),
  updateDelegation: (id: string, data: CreateApprovalDelegationInput) =>
    api.put<ApprovalDelegation>(`/api/v1/workflows/delegations/${id}`, data),
  deleteDelegation: (id: string) => api.delete(`/api/v1/workflows/delegations/${id}`),

  // Approval Limits
  listApprovalLimits: () => api.get<ApprovalLimitConfig[]>('/api/v1/workflows/approval-limits'),
  getApprovalLimit: (id: string) => api.get<ApprovalLimitConfig>(`/api/v1/workflows/approval-limits/${id}`),
  createApprovalLimit: (data: CreateApprovalLimitInput) =>
    api.post<ApprovalLimitConfig>('/api/v1/workflows/approval-limits', data),
  updateApprovalLimit: (id: string, data: CreateApprovalLimitInput) =>
    api.put<ApprovalLimitConfig>(`/api/v1/workflows/approval-limits/${id}`, data),
  deleteApprovalLimit: (id: string) => api.delete(`/api/v1/workflows/approval-limits/${id}`),
};

// Reports API
export const reportsApi = {
  dashboardSummary: () =>
    api.get<DashboardSummary>('/api/v1/reports/dashboard/summary'),

  dashboardKpis: () =>
    api.get<DashboardKpis>('/api/v1/reports/dashboard/kpis'),

  invoicesByVendor: (params?: { start_date?: string; end_date?: string }) => {
    const qs = new URLSearchParams();
    if (params?.start_date) qs.set('start_date', params.start_date);
    if (params?.end_date) qs.set('end_date', params.end_date);
    const query = qs.toString();
    return api.get<InvoicesByVendor[]>(`/api/v1/reports/invoices/by-vendor${query ? `?${query}` : ''}`);
  },

  invoicesByStatus: () =>
    api.get<InvoicesByStatus[]>('/api/v1/reports/invoices/by-status'),

  invoiceAging: () => api.get<AgingBucket[]>('/api/v1/reports/invoices/aging'),

  vendorSpend: () =>
    api.get<VendorSpend[]>('/api/v1/reports/vendors/spend'),

  workflowMetrics: () =>
    api.get<WorkflowMetrics>('/api/v1/reports/workflows/metrics'),

  spendTrends: (params?: { start_date?: string; end_date?: string; group_by?: string }) => {
    const end = new Date();
    const start = new Date(end);
    start.setMonth(start.getMonth() - 12);
    const qs = new URLSearchParams({
      start_date: params?.start_date ?? start.toISOString().slice(0, 10),
      end_date: params?.end_date ?? end.toISOString().slice(0, 10),
      group_by: params?.group_by ?? 'month',
    });
    return api.get<SpendTrend[]>(`/api/v1/reports/spend/trends?${qs}`);
  },

  categoryBreakdown: (params?: { category_type?: string; start_date?: string; end_date?: string }) => {
    const qs = new URLSearchParams({
      category_type: params?.category_type ?? 'gl_code',
    });
    if (params?.start_date) qs.set('start_date', params.start_date);
    if (params?.end_date) qs.set('end_date', params.end_date);
    return api.get<CategoryBreakdown[]>(`/api/v1/reports/categories/breakdown?${qs}`);
  },

  vendorPerformance: () =>
    api.get<VendorPerformance[]>('/api/v1/reports/vendors/performance'),

  approvalAnalytics: () =>
    api.get<ApprovalAnalytics>('/api/v1/reports/approvals/analytics'),

  approvalSla: () =>
    api.get<ApprovalSlaSummary>('/api/v1/reports/approvals/sla'),

  cashFlowObligations: () =>
    api.get<CashFlowObligation[]>('/api/v1/reports/cash-flow/obligations'),

  apCashFlowForecast: (params?: { horizon_weeks?: number; as_of_date?: string; min_daily_funding_threshold?: number }) => {
    const qs = new URLSearchParams();
    if (params?.horizon_weeks) qs.set('horizon_weeks', String(params.horizon_weeks));
    if (params?.as_of_date) qs.set('as_of_date', params.as_of_date);
    if (params?.min_daily_funding_threshold) qs.set('min_daily_funding_threshold', String(params.min_daily_funding_threshold));
    return api.get<ApCashFlowForecast>(`/api/v1/reports/cash-flow/forecast?${qs}`);
  },

  simulateApCashFlowForecast: (body: {
    horizon_weeks?: number;
    as_of_date?: string;
    min_daily_funding_threshold?: number;
    scenario: ScenarioInputs;
  }) => api.post<ApCashFlowSimulation>('/api/v1/reports/cash-flow/forecast/simulate', body),

  mlAccuracy: () =>
    api.get<{ accuracy_rate: number; total_suggestions: number; accepted: number; corrected: number; rejected: number }>('/api/v1/invoices/ml-accuracy'),
};

// Documents API
export const documentsApi = {
  // Upload a document (optionally linked to an invoice)
  upload: (file: File, invoiceId?: string) => {
    const formData = new FormData();
    formData.append('file', file);
    const params = invoiceId ? `?invoice_id=${invoiceId}` : '';
    return api.upload<DocumentUploadResponse>(`/api/v1/documents${params}`, formData);
  },

  // Upload a document directly for a specific invoice
  uploadForInvoice: (invoiceId: string, file: File) => {
    const formData = new FormData();
    formData.append('file', file);
    return api.upload<DocumentUploadResponse>(`/api/v1/documents/invoice/${invoiceId}`, formData);
  },

  // Get document metadata
  getMetadata: (id: string) =>
    api.get<DocumentMetadata>(`/api/v1/documents/${id}/metadata`),

  // List documents for an invoice
  listForInvoice: (invoiceId: string) =>
    api.get<DocumentMetadata[]>(`/api/v1/documents/invoice/${invoiceId}`),

  // Delete a document
  delete: (id: string) =>
    api.delete<{ success: boolean }>(`/api/v1/documents/${id}`),

  // Get download URL for a document (legacy - use downloadBlob instead for authenticated access)
  getDownloadUrl: (id: string) =>
    `${API_BASE_URL}/api/v1/documents/${id}`,

  // Download document as blob with authentication
  downloadBlob: (id: string) =>
    api.downloadBlob(`/api/v1/documents/${id}`),
};

// Types
export interface VendorContact {
  id: string;
  name: string;
  title?: string;
  email?: string;
  phone?: string;
  is_primary: boolean;
}

export interface Vendor {
  id: string;
  tenant_id: string;
  name: string;
  legal_name?: string;
  vendor_type: string;
  status: string;
  email?: string;
  phone?: string;
  website?: string;
  address?: {
    line1: string;
    line2?: string;
    city: string;
    state?: string;
    postal_code: string;
    country: string;
  };
  tax_id?: string;
  tax_id_type?: string;
  w9_on_file: boolean;
  w9_received_date?: string;
  payment_terms?: string;
  default_payment_method?: string;
  bank_account?: {
    bank_name: string;
    account_type: string;
    account_last_four: string;
  };
  vendor_code?: string;
  default_gl_code?: string;
  default_department?: string;
  primary_contact?: VendorContact;
  contacts: VendorContact[];
  notes?: string;
  tags: string[];
  custom_fields?: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface CreateVendorInput {
  name: string;
  vendor_type: string;
  email?: string;
}

export interface ImportVendorsResult {
  imported: number;
  skipped: number;
  errors: number;
  error_details: string[];
}

export interface WorkflowRule {
  id: string;
  name: string;
  description?: string;
  rule_type: string;
  is_active: boolean;
}

export interface CreateWorkflowRuleInput {
  name: string;
  description?: string;
  rule_type: string;
  conditions: unknown[];
  actions: unknown[];
}

export interface WorkQueue {
  id: string;
  name: string;
  description?: string;
  queue_type: string;
  assigned_users: string[];
  assigned_roles: string[];
  is_default: boolean;
  is_active: boolean;
  settings: {
    default_sort: string;
    sla_hours?: number;
    escalation_hours?: number;
    escalation_user_id?: string;
  };
}

export interface CreateWorkQueueInput {
  name: string;
  description?: string;
  queue_type: string;
  assigned_users: string[];
  assigned_roles: string[];
  settings: {
    default_sort: string;
    sla_hours?: number;
    escalation_hours?: number;
    escalation_user_id?: string;
  };
}

export interface QueueItem {
  id: string;
  queue_id: string;
  invoice_id: string;
  assigned_to?: string;
  priority: number;
  entered_at: string;
  due_at?: string;
  claimed_at?: string;
  completed_at?: string;
}

export interface InboxItem {
  id: string;
  queue_id: string;
  invoice_id: string;
  assigned_to?: string;
  priority: number;
  entered_at: string;
  due_at?: string;
  claimed_at?: string;
  completed_at?: string;
  queue_name: string;
  queue_type: string;
  invoice_number?: string;
  vendor_name?: string;
  total_amount_cents?: number;
  currency?: string;
  invoice_status?: string;
}

export interface AssignmentRule {
  id: string;
  queue_id: string;
  name: string;
  description?: string;
  priority: number;
  is_active: boolean;
  conditions: AssignmentCondition[];
  assign_to: AssignmentTarget;
}

export interface AssignmentCondition {
  field: string;
  operator: string;
  value: unknown;
}

export type AssignmentTarget =
  | { User: string }
  | { Role: string }
  | 'VendorApprover'
  | 'DepartmentApprover'
  | { RoundRobin: string[] }
  | { LeastLoaded: string[] };

export interface CreateAssignmentRuleInput {
  queue_id: string;
  name: string;
  description?: string;
  priority: number;
  conditions: AssignmentCondition[];
  assign_to: AssignmentTarget;
}

export interface ApprovalRequest {
  id: string;
  invoice_id: string;
  status: string;
  created_at: string;
  updated_at?: string;
  invoice_number?: string;
  vendor_name?: string;
  total_amount?: number;
  amount?: number;
  currency?: string;
  due_date?: string;
  requester?: string;
  notes?: string;
  priority?: 'low' | 'normal' | 'medium' | 'high';
}

export interface ApprovalLinkResponse {
  approve_url: string;
  reject_url: string;
  hold_url: string;
  view_url: string;
  expires_at: string;
}

export interface ResendApprovalEmailResponse {
  sent_to: string;
  expires_at: string;
}

export interface BulkOperationInput {
  invoice_ids: string[];
  operation: 'submit_for_payment' | 'approve' | 'reject' | 'move_to_queue' | 'assign_to';
  comment?: string;
}

export interface BulkOperationResult {
  total: number;
  successful: number;
  failed: number;
  errors: Array<{
    invoice_id: string;
    error: string;
  }>;
}

export interface ApprovalDelegation {
  id: string;
  tenant_id: string;
  delegator_id: string;
  delegate_id: string;
  start_date: string;
  end_date: string;
  is_active: boolean;
  conditions?: AssignmentCondition[];
  created_at: string;
}

export interface CreateApprovalDelegationInput {
  delegator_id: string;
  delegate_id: string;
  start_date: string;
  end_date: string;
  conditions?: AssignmentCondition[];
}

export interface ApprovalLimitConfig {
  id: string;
  tenant_id: string;
  user_id: string;
  max_amount: { amount: number; currency: string };
  vendor_restrictions?: string[];
  department_restrictions?: string[];
  created_at: string;
  updated_at: string;
}

export interface CreateApprovalLimitInput {
  user_id: string;
  max_amount: { amount: number; currency: string };
  vendor_restrictions?: string[];
  department_restrictions?: string[];
}

export interface DashboardSummary {
  invoices_pending_review: number;
  invoices_pending_approval: number;
  invoices_ready_for_payment: number;
  total_pending_amount: number;
  vendors_active: number;
  invoices_processed_today: number;
  avg_processing_time_hours: number;
}

export interface InvoicesByVendor {
  vendor_id: string;
  vendor_name: string;
  invoice_count: number;
  total_amount: number;
}

export interface InvoicesByStatus {
  status: string;
  count: number;
  total_amount: number;
}

export interface AgingBucket {
  bucket: string;
  count: number;
  total_amount: number;
}

export interface VendorSpend {
  vendor_id: string;
  vendor_name: string;
  invoice_count: number;
  ytd_spend: number;
  mtd_spend: number;
}

export interface WorkflowMetrics {
  avg_processing_time_hours: number;
  avg_approval_time_hours: number;
  auto_approval_rate: number;
  rejection_rate: number;
}

export interface SpendTrend {
  period: string;
  total_spend: number;
  invoice_count: number;
  avg_invoice_amount: number;
  change_from_prior_period?: number | null;
  change_percentage?: number | null;
}

export interface CategoryBreakdown {
  category_type: string;
  category_value: string;
  total_amount: number;
  invoice_count: number;
  percentage_of_total: number;
  avg_amount: number;
}

export interface VendorPerformance {
  vendor_id: string;
  vendor_name: string;
  total_invoices: number;
  total_spend: number;
  avg_payment_days: number;
  on_time_payment_rate: number;
  dispute_rate: number;
  credit_utilization: number;
  reliability_score: number;
}

export interface ApprovalAnalytics {
  total_approvals: number;
  avg_approval_time_hours: number;
  approval_rate: number;
  rejection_rate: number;
  bottleneck_stages: Array<{
    stage_name: string;
    avg_time_hours: number;
    invoice_count: number;
    percentage_of_total_time: number;
  }>;
  approver_workloads: Array<{
    approver_id: string;
    approver_name: string;
    approvals_completed: number;
    avg_time_to_approve_hours: number;
    pending_approvals: number;
    approval_rate: number;
  }>;
}

export interface ApprovalSlaItem {
  invoice_id: string;
  invoice_number: string;
  vendor_name: string;
  amount_cents: number;
  currency: string;
  approval_id: string;
  hours_waiting: number;
  sla_hours: number;
  deadline_at: string;
  percent_elapsed: number;
  sla_state: 'within_sla' | 'near_breach' | 'breached';
  approver_name?: string | null;
  approver_label: string;
}

export interface ApprovalSlaSummary {
  pending_count: number;
  near_breach_count: number;
  breached_count: number;
  items: ApprovalSlaItem[];
}

export interface CashFlowObligation {
  invoice_id: string;
  invoice_number: string;
  vendor_name: string;
  due_date?: string | null;
  projected_payment_date?: string | null;
  amount_cents: number;
  currency: string;
  processing_status: string;
  late_risk: boolean;
}

// AP Cash Flow Forecast types
export interface ForecastBreakdownEntry {
  name: string;
  amount_cents: number;
}

export interface ForecastDay {
  date: string;
  expected_amount: number;
  low_band: number;
  high_band: number;
  vendor_breakdown: ForecastBreakdownEntry[];
  gl_breakdown: ForecastBreakdownEntry[];
  funding_required: boolean;
}

export interface ForecastWeek {
  week_start: string;
  week_end: string;
  expected_amount: number;
  low_band: number;
  high_band: number;
}

export interface ForecastMonth {
  month: string;
  expected_amount: number;
  low_band: number;
  high_band: number;
}

export interface ApCashFlowForecast {
  as_of_date: string;
  horizon_weeks: number;
  daily: ForecastDay[];
  weekly: ForecastWeek[];
  monthly: ForecastMonth[];
}

// What-If Simulator types
export interface ScenarioInputs {
  pending_approval_delay_days?: number | null;
  capture_all_epd?: boolean | null;
  vendor_term_shift_days?: number | null;
  override_funding_threshold_cents?: number | null;
}

export interface ApCashFlowSimulation {
  baseline: ApCashFlowForecast;
  scenario: ApCashFlowForecast;
  scenario_inputs: ScenarioInputs;
}

// Workflow Template types
export interface WorkflowTemplate {
  id: string;
  name: string;
  description?: string;
  is_active: boolean;
  is_default: boolean;
  stages: WorkflowTemplateStage[];
  created_at: string;
  updated_at: string;
}

export interface WorkflowTemplateStage {
  order: number;
  name: string;
  stage_type: 'intake' | 'review' | 'approval' | 'exception' | 'payment' | 'custom';
  queue_id?: string;
  sla_hours?: number;
  escalation_hours?: number;
  requires_action: boolean;
  skip_conditions: RuleCondition[];
  auto_advance_conditions: RuleCondition[];
}

export interface RuleCondition {
  field: string;
  operator: string;
  value: unknown;
}

export interface CreateWorkflowTemplateInput {
  name: string;
  description?: string;
  is_default: boolean;
  stages: WorkflowTemplateStage[];
}

// Document types
export interface DocumentUploadResponse {
  id: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  url: string;
}

export interface DocumentMetadata {
  id: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  invoice_id?: string;
  doc_type: string;
  created_at: string;
  url: string;
}

// Feedback API
export interface FeedbackEntry {
  id: string;
  user_email: string;
  user_name: string;
  message: string;
  category: string;
  page?: string;
  timestamp: string;
}

export const feedbackApi = {
  submit: (data: { message: string; category?: string; page?: string }) =>
    api.post<FeedbackEntry>('/api/v1/feedback', data),

  list: () => api.get<FeedbackEntry[]>('/api/v1/feedback'),
};

// Integration Status API
export interface IntegrationStatusResponse {
  connected: boolean;
  company_id?: string;
  company_name?: string;
  last_sync_at?: string;
  sync_enabled?: boolean;
}

export async function getIntegrationStatus(statusEndpoint: string): Promise<IntegrationStatusResponse> {
  return api.get<IntegrationStatusResponse>(statusEndpoint);
}

// Shared Integration Types
export interface AccountMapping {
  local_account_id: string;
  remote_account_id: string;
  remote_account_name: string;
}

export interface SyncResult {
  imported: number;
  updated: number;
  skipped: number;
  errors: number;
}

export interface OAuthConnectResponse {
  redirect_url: string;
}

export interface SageIntacctConnectInput {
  sender_id: string;
  sender_password: string;
  company_id: string;
  entity_id?: string;
  user_id: string;
  user_password: string;
}

export interface BillComConnectInput {
  dev_key: string;
  org_id: string;
  user_name: string;
  password: string;
  environment: 'sandbox' | 'production';
}

// QuickBooks API
export const quickbooksApi = {
  connect: () => api.get<OAuthConnectResponse>('/api/v1/quickbooks/connect'),
  disconnect: () => api.post('/api/v1/quickbooks/disconnect'),
  status: () => api.get<IntegrationStatusResponse>('/api/v1/quickbooks/status'),
  callback: () => api.get('/api/v1/quickbooks/callback'),
  syncVendors: () => api.post<SyncResult>('/api/v1/quickbooks/sync/vendors'),
  syncAccounts: () => api.post<SyncResult>('/api/v1/quickbooks/sync/accounts'),
  exportInvoice: (id: string) => api.post('/api/v1/quickbooks/export/invoice/' + id),
  getAccountMappings: () => api.get<AccountMapping[]>('/api/v1/quickbooks/mappings/accounts'),
  updateAccountMappings: (mappings: AccountMapping[]) => api.post('/api/v1/quickbooks/mappings/accounts', mappings),
};

// Xero API
export const xeroApi = {
  connect: () => api.get<OAuthConnectResponse>('/api/v1/xero/connect'),
  disconnect: () => api.post('/api/v1/xero/disconnect'),
  status: () => api.get<IntegrationStatusResponse>('/api/v1/xero/status'),
  callback: () => api.get('/api/v1/xero/callback'),
  syncContacts: () => api.post<SyncResult>('/api/v1/xero/sync/contacts'),
  syncAccounts: () => api.post<SyncResult>('/api/v1/xero/sync/accounts'),
  exportInvoice: (id: string) => api.post('/api/v1/xero/export/invoice/' + id),
  getAccountMappings: () => api.get<AccountMapping[]>('/api/v1/xero/mappings/accounts'),
  updateAccountMappings: (mappings: AccountMapping[]) => api.post('/api/v1/xero/mappings/accounts', mappings),
};

// Sage Intacct API
export const sageIntacctApi = {
  connect: (credentials: SageIntacctConnectInput) =>
    api.post<OAuthConnectResponse>('/api/v1/sage-intacct/connect', credentials),
  disconnect: () => api.post('/api/v1/sage-intacct/disconnect'),
  status: () => api.get<IntegrationStatusResponse>('/api/v1/sage-intacct/status'),
  syncVendors: () => api.post<SyncResult>('/api/v1/sage-intacct/sync/vendors'),
  syncAccounts: () => api.post<SyncResult>('/api/v1/sage-intacct/sync/accounts'),
  exportInvoice: (id: string) => api.post('/api/v1/sage-intacct/export/invoice/' + id),
  getAccountMappings: () => api.get<AccountMapping[]>('/api/v1/sage-intacct/mappings/accounts'),
  updateAccountMappings: (mappings: AccountMapping[]) => api.post('/api/v1/sage-intacct/mappings/accounts', mappings),
  getEntities: () => api.get<unknown[]>('/api/v1/sage-intacct/entities'),
};

// Salesforce API
export const salesforceApi = {
  connect: () => api.get<OAuthConnectResponse>('/api/v1/salesforce/connect'),
  disconnect: () => api.post('/api/v1/salesforce/disconnect'),
  status: () => api.get<IntegrationStatusResponse>('/api/v1/salesforce/status'),
  callback: () => api.get('/api/v1/salesforce/callback'),
  syncAccounts: () => api.post<SyncResult>('/api/v1/salesforce/sync/accounts'),
  syncContacts: () => api.post<SyncResult>('/api/v1/salesforce/sync/contacts'),
  getAccountMappings: () => api.get<AccountMapping[]>('/api/v1/salesforce/mappings/accounts'),
  updateAccountMappings: (mappings: AccountMapping[]) => api.post('/api/v1/salesforce/mappings/accounts', mappings),
};

// Workday API
export const workdayApi = {
  connect: () => api.get<OAuthConnectResponse>('/api/v1/workday/connect'),
  disconnect: () => api.post('/api/v1/workday/disconnect'),
  status: () => api.get<IntegrationStatusResponse>('/api/v1/workday/status'),
  callback: () => api.get('/api/v1/workday/callback'),
  syncSuppliers: () => api.post<SyncResult>('/api/v1/workday/sync/suppliers'),
  syncAccounts: () => api.post<SyncResult>('/api/v1/workday/sync/accounts'),
  exportInvoice: (id: string) => api.post('/api/v1/workday/export/invoice/' + id),
  getAccountMappings: () => api.get<AccountMapping[]>('/api/v1/workday/mappings/accounts'),
  updateAccountMappings: (mappings: AccountMapping[]) => api.post('/api/v1/workday/mappings/accounts', mappings),
  getCompanies: () => api.get<unknown[]>('/api/v1/workday/companies'),
};

// Bill.com API
export const billComApi = {
  connect: (credentials: BillComConnectInput) =>
    api.post<OAuthConnectResponse>('/api/v1/bill-com/connect', credentials),
  disconnect: () => api.post('/api/v1/bill-com/disconnect'),
  status: () => api.get<IntegrationStatusResponse>('/api/v1/bill-com/status'),
  syncVendors: () => api.post<SyncResult>('/api/v1/bill-com/sync/vendors'),
  pushBill: (id: string) => api.post('/api/v1/bill-com/push/bill/' + id),
  payBill: (id: string) => api.post('/api/v1/bill-com/pay/bill/' + id),
  payBulk: (billIds: string[]) => api.post('/api/v1/bill-com/pay/bulk', { bill_ids: billIds }),
  listPayments: () => api.get<unknown[]>('/api/v1/bill-com/payments'),
  listFundingAccounts: () => api.get<unknown[]>('/api/v1/bill-com/funding-accounts'),
};

// EDI API
export const ediApi = {
  connect: () => api.get<OAuthConnectResponse>('/api/v1/edi/connect'),
  disconnect: () => api.post('/api/v1/edi/disconnect'),
  status: () => api.get<IntegrationStatusResponse>('/api/v1/edi/status'),
  webhookInbound: () => api.post('/api/v1/edi/webhook/inbound'),
  listDocuments: () => api.get<unknown[]>('/api/v1/edi/documents'),
  getDocument: (id: string) => api.get<unknown>('/api/v1/edi/documents/' + id),
  sendRemittance: (invoiceId: string) => api.post('/api/v1/edi/send-remittance/' + invoiceId),
  listOutbound: () => api.get<unknown[]>('/api/v1/edi/outbound'),
  getAckTimeouts: () => api.get<unknown[]>('/api/v1/edi/ack-timeouts'),
  listPartners: () => api.get<unknown[]>('/api/v1/edi/partners'),
  createPartner: (data: unknown) => api.post<unknown>('/api/v1/edi/partners', data),
  updatePartner: (id: string, data: unknown) => api.put<unknown>('/api/v1/edi/partners/' + id, data),
  deletePartner: (id: string) => api.delete('/api/v1/edi/partners/' + id),
};

// Invoice Status Config Types
export interface InvoiceStatusConfig {
  id: string;
  status_key: string;
  display_label: string;
  color: string;
  bg_color: string;
  text_color: string;
  sort_order: number;
  is_terminal: boolean;
  is_active: boolean;
  category: string;
  created_at: string;
  updated_at: string;
}

export interface InvoiceStatusConfigInput {
  status_key: string;
  display_label: string;
  color: string;
  bg_color: string;
  text_color: string;
  sort_order: number;
  is_terminal: boolean;
  is_active: boolean;
  category: string;
}

export const invoiceStatusApi = {
  list: (category?: string) =>
    api.get<InvoiceStatusConfig[]>(`/api/v1/settings/invoice-statuses${category ? `?category=${category}` : ''}`),

  update: (statuses: InvoiceStatusConfigInput[]) =>
    api.put<InvoiceStatusConfig[]>('/api/v1/settings/invoice-statuses', statuses),

  seedDefaults: () =>
    api.post<{ message: string }>('/api/v1/settings/invoice-statuses/seed-defaults', {}),

  delete: (statusKey: string) =>
    api.delete<{ success: boolean }>(`/api/v1/settings/invoice-statuses/${statusKey}`),
};

// Organization Theme Types
export interface OrganizationThemeColors {
  primary: string;
  accent: string;
  capture: string;
  processing: string;
  vendor: string;
  reporting: string;
}

export interface OrganizationBranding {
  logoUrl?: string;
  logoMark?: string;
  faviconUrl?: string;
  brandName: string;
  brandGradient?: string;
  customCSS?: string;
}

export interface OrganizationTheme {
  id: string;
  tenant_id: string;
  preset_id: string;
  custom_colors?: OrganizationThemeColors;
  branding: OrganizationBranding;
  enabled_for_all_users: boolean;
  allow_user_override: boolean;
  gradient_config?: {
    enabled: boolean;
    type: 'linear' | 'radial';
    angle?: number;
    positions?: { color: string; position: number }[];
  };
  created_at: string;
  updated_at: string;
}

export interface CreateOrganizationThemeInput {
  preset_id: string;
  custom_colors?: OrganizationThemeColors;
  branding: OrganizationBranding;
  enabled_for_all_users?: boolean;
  allow_user_override?: boolean;
  gradient_config?: {
    enabled: boolean;
    type: 'linear' | 'radial' | 'conic';
    angle?: number;
  };
}

export interface UserThemePreference {
  id: string;
  user_id: string;
  preset_id: string;
  custom_colors?: OrganizationThemeColors;
  mode: 'light' | 'dark' | 'system';
  created_at: string;
  updated_at: string;
}

export interface CreateUserThemeInput {
  preset_id: string;
  custom_colors?: OrganizationThemeColors;
  mode: 'light' | 'dark' | 'system';
}

// Organization Theme API
export const organizationThemeApi = {
  // Get organization theme
  getTheme: () =>
    api.get<OrganizationTheme>('/api/v1/organization/theme'),

  // Create or update organization theme
  saveTheme: (data: CreateOrganizationThemeInput) =>
    api.post<OrganizationTheme>('/api/v1/organization/theme', data),

  // Update organization theme
  updateTheme: (data: Partial<CreateOrganizationThemeInput>) =>
    api.put<OrganizationTheme>('/api/v1/organization/theme', data),

  // Delete organization theme (revert to default)
  deleteTheme: () =>
    api.delete('/api/v1/organization/theme'),

  // Upload organization logo
  uploadLogo: (file: File, type: 'logo' | 'logoMark' | 'favicon' = 'logo') => {
    const formData = new FormData();
    formData.append('file', file);
    formData.append('type', type);
    return api.upload<{ url: string; type: string }>('/api/v1/organization/theme/logo', formData);
  },

  // Delete logo
  deleteLogo: (type: 'logo' | 'logoMark' | 'favicon') =>
    api.delete(`/api/v1/organization/theme/logo/${type}`),

  // Preview theme (returns CSS variables)
  previewTheme: (data: CreateOrganizationThemeInput) =>
    api.post<{ css_variables: Record<string, string> }>('/api/v1/organization/theme/preview', data),

  // Export theme configuration
  exportTheme: () =>
    api.get<{ config: string; version: string; exported_at: string }>('/api/v1/organization/theme/export'),

  // Import theme configuration
  importTheme: (config: string) =>
    api.post<OrganizationTheme>('/api/v1/organization/theme/import', { config }),
};

// User Theme Preferences API
export const userThemeApi = {
  // Get user's theme preferences
  getPreferences: () =>
    api.get<UserThemePreference>('/api/v1/user/theme'),

  // Save user's theme preferences
  savePreferences: (data: CreateUserThemeInput) =>
    api.post<UserThemePreference>('/api/v1/user/theme', data),

  // Update user's theme preferences
  updatePreferences: (data: Partial<CreateUserThemeInput>) =>
    api.put<UserThemePreference>('/api/v1/user/theme', data),

  // Reset to organization default
  resetToDefault: () =>
    api.delete('/api/v1/user/theme'),

  // Get effective theme (combines org + user preferences)
  getEffectiveTheme: () =>
    api.get<{
      theme: OrganizationTheme | null;
      user_preference: UserThemePreference | null;
      effective_colors: OrganizationThemeColors;
      effective_mode: 'light' | 'dark' | 'system';
      can_override: boolean;
    }>('/api/v1/user/theme/effective'),
};

// ---------------------------------------------------------------------------
// Vendor Statement Reconciliation Types
// ---------------------------------------------------------------------------

export type StatementStatus = 'pending' | 'in_review' | 'reconciled' | 'disputed';
export type LineMatchStatus = 'unmatched' | 'matched' | 'discrepancy' | 'ignored';
export type LineType = 'invoice' | 'credit' | 'payment' | 'adjustment';
export type MatchConfidence = 'exact' | 'amount_only' | 'no_match';

export interface VendorStatement {
  id: string;
  tenant_id: string;
  vendor_id: string;
  statement_number: string | null;
  statement_date: string | null;
  period_start: string;
  period_end: string;
  opening_balance_cents: number;
  closing_balance_cents: number;
  currency: string;
  status: StatementStatus;
  reconciled_by: string | null;
  reconciled_at: string | null;
  notes: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface StatementLineItem {
  id: string;
  statement_id: string;
  tenant_id: string;
  line_date: string;
  description: string;
  reference_number: string | null;
  amount_cents: number;
  line_type: LineType;
  match_status: LineMatchStatus;
  matched_invoice_id: string | null;
  variance_cents: number;
  matched_at: string | null;
  matched_by: string | null;
  notes: string | null;
  created_at: string;
  updated_at: string;
}

export interface ReconciliationSummary {
  total_lines: number;
  matched: number;
  unmatched: number;
  discrepancies: number;
  ignored: number;
  total_variance_cents: number;
}

export interface StatementDetailResponse {
  statement: VendorStatement;
  lines: StatementLineItem[];
  summary: ReconciliationSummary;
}

export interface StatementListResponse {
  data: VendorStatement[];
  pagination: { page: number; per_page: number; total_items: number; total_pages: number };
}

export interface MatchResult {
  line_id: string;
  confidence: MatchConfidence;
  matched_invoice_id: string | null;
  variance_cents: number;
  match_status: LineMatchStatus;
}

export interface MatchResponse {
  results: MatchResult[];
  summary: ReconciliationSummary;
}

export interface CreateStatementInput {
  vendor_id: string;
  statement_number?: string;
  statement_date?: string;
  period_start: string;
  period_end: string;
  opening_balance_cents: number;
  closing_balance_cents: number;
  currency?: string;
  notes?: string;
  lines: CreateStatementLineInput[];
}

export interface CreateStatementLineInput {
  line_date: string;
  description: string;
  reference_number?: string;
  amount_cents: number;
  line_type?: LineType;
}

export interface UpdateLineMatchInput {
  match_status: LineMatchStatus;
  matched_invoice_id?: string | null;
  notes?: string;
}

// Vendor Statements API
export const vendorStatementsApi = {
  /** Create a new statement with line items and auto-match */
  create: (vendorId: string, data: Omit<CreateStatementInput, 'vendor_id'>) =>
    api.post<StatementDetailResponse>(`/api/v1/vendors/${vendorId}/statements`, { ...data, vendor_id: vendorId }),

  /** List statements for a vendor */
  list: (vendorId: string, params?: { page?: number; per_page?: number; status?: string }) => {
    const qs = new URLSearchParams();
    if (params?.page) qs.set('page', String(params.page));
    if (params?.per_page) qs.set('per_page', String(params.per_page));
    if (params?.status) qs.set('status', params.status);
    const query = qs.toString();
    return api.get<StatementListResponse>(`/api/v1/vendors/${vendorId}/statements${query ? `?${query}` : ''}`);
  },

  /** Get statement detail with lines and reconciliation summary */
  get: (vendorId: string, statementId: string) =>
    api.get<StatementDetailResponse>(`/api/v1/vendors/${vendorId}/statements/${statementId}`),

  /** Re-run auto-matching on a statement */
  runMatch: (vendorId: string, statementId: string) =>
    api.post<MatchResponse>(`/api/v1/vendors/${vendorId}/statements/${statementId}/match`, {}),

  /** Manually update a line's match status */
  updateLine: (vendorId: string, statementId: string, lineId: string, data: UpdateLineMatchInput) =>
    api.put<{ success: boolean }>(`/api/v1/vendors/${vendorId}/statements/${statementId}/lines/${lineId}`, data),

  /** Mark a statement as reconciled */
  reconcile: (vendorId: string, statementId: string) =>
    api.post<{ success: boolean; status: string }>(`/api/v1/vendors/${vendorId}/statements/${statementId}/reconcile`, {}),
};

// ---------------------------------------------------------------------------
// Dashboard Metrics Types
// ---------------------------------------------------------------------------

export interface DashboardInvoiceMetrics {
  total_invoices: number;
  pending_ocr: number;
  ready_for_review: number;
  submitted: number;
  approved: number;
  rejected: number;
  paid: number;
  avg_processing_time_hours: number;
  total_value: number;
  this_month: number;
  trend_vs_last_month: number;
}

export interface DashboardApprovalMetrics {
  pending_approvals: number;
  approved_today: number;
  rejected_today: number;
  avg_approval_time_hours: number;
  approval_rate: number;
  escalated: number;
  overdue: number;
}

export interface DashboardTopVendor {
  vendor_id: string;
  vendor_name: string;
  invoice_count: number;
  total_amount: number;
}

export interface DashboardVendorMetrics {
  total_vendors: number;
  new_this_month: number;
  top_vendors: DashboardTopVendor[];
  concentration_percentage: number;
}

export interface DashboardTeamMemberStats {
  user_id: string;
  user_name: string;
  approvals_this_month: number;
  rejections_this_month: number;
  avg_response_time_hours: number;
}

export interface DashboardTeamMetrics {
  members: DashboardTeamMemberStats[];
  avg_approvals_per_member: number;
  total_pending_actions: number;
}

export interface DashboardMetrics {
  invoices: DashboardInvoiceMetrics;
  approvals: DashboardApprovalMetrics;
  vendors: DashboardVendorMetrics;
  team: DashboardTeamMetrics;
}

// Dashboard API
export const dashboardApi = {
  /** Alias kept for backward compatibility with existing consumers */
  metrics: () =>
    api.get<DashboardMetrics>('/api/v1/dashboard/metrics'),

  getMetrics: () =>
    api.get<DashboardMetrics>('/api/v1/dashboard/metrics'),

  /** KPIs backed by materialized view for sub-second reads */
  getKpis: () =>
    api.get<DashboardKpis>('/api/v1/reports/dashboard/kpis'),

  getInvoiceMetrics: () =>
    api.get<DashboardInvoiceMetrics>('/api/v1/dashboard/metrics/invoices'),

  getApprovalMetrics: () =>
    api.get<DashboardApprovalMetrics>('/api/v1/dashboard/metrics/approvals'),

  getVendorMetrics: () =>
    api.get<DashboardVendorMetrics>('/api/v1/dashboard/metrics/vendors'),

  getTeamMetrics: () =>
    api.get<DashboardTeamMetrics>('/api/v1/dashboard/metrics/team'),

  /** Stage dwell-time bottleneck heat map */
  getStageDwell: () =>
    api.get<StageDwellRow[]>('/api/v1/dashboard/stage-dwell'),

  /** Per-approver workload distribution */
  getApproverWorkload: () =>
    api.get<ApproverWorkloadRow[]>('/api/v1/dashboard/approver-workload'),

  /** Exception rate trend over N days (default 14) */
  getExceptionTrend: (days?: number) => {
    const qs = days ? `?days=${days}` : '';
    return api.get<ExceptionTrendPoint[]>(`/api/v1/dashboard/exception-trend${qs}`);
  },
};

// ---------------------------------------------------------------------------
// Dashboard SLA / Bottleneck Types
// ---------------------------------------------------------------------------

export interface StageDwellRow {
  stage: string;
  median_minutes: number;
  p90_minutes: number;
  count: number;
}

export interface ApproverWorkloadRow {
  approver_id: string;
  approver_name: string;
  pending_count: number;
  near_breach_count: number;
  breached_count: number;
  avg_response_hours: number;
}

export interface ExceptionTrendPoint {
  date: string;
  total_invoices: number;
  exception_count: number;
  exception_rate: number;
}

// ---------------------------------------------------------------------------
// AP Command Center — standup view
// ---------------------------------------------------------------------------

export interface ApCommandCenterBucket {
  label: string;
  range_start: string;
  range_end: string;
  total_payable_cents: number;
  invoices: ApCommandCenterInvoice[];
}

export interface ApCommandCenterInvoice {
  invoice_id: string;
  invoice_number: string;
  vendor_name: string;
  amount_cents: number;
  due_date: string;
  blocking_approver_id: string | null;
  blocking_approver_name: string | null;
  days_stuck: number;
  late_fee_risk_cents: number;
  discount_expiring_cents: number;
  discount_expires_at: string | null;
}

export interface ApCommandCenterResponse {
  week_buckets: [ApCommandCenterBucket, ApCommandCenterBucket];
  late_fee_risk_total_cents: number;
  discount_expiring_total_cents: number;
  generated_at: string;
}

export const apCommandCenterApi = {
  /** Fetch this-week + next-week payables with blocker & discount metadata. */
  thisWeek: () =>
    api.get<ApCommandCenterResponse>('/api/v1/dashboard/ap-command-center/this-week'),
};

// ---------------------------------------------------------------------------
// Benchmark Peer Insights
// ---------------------------------------------------------------------------

export interface BenchmarkKpis {
  dpo_days: number;
  avg_approval_cycle_hours: number;
  ocr_straight_through_rate: number;
  exception_rate: number;
  discount_capture_rate: number;
  cost_per_invoice: number;
}

export interface CohortPercentiles {
  p25: BenchmarkKpis;
  p50: BenchmarkKpis;
  p75: BenchmarkKpis;
}

export interface CohortDescriptor {
  industry: string;
  headcount_band: string;
  volume_band: string;
}

export interface BenchmarkResponse {
  opted_in: boolean;
  cohort?: CohortDescriptor;
  tenant_kpis?: BenchmarkKpis;
  cohort_kpis?: CohortPercentiles;
  cohort_size?: number;
}

export interface BenchmarkOptInRequest {
  industry: string;
  headcount_band: string;
  volume_band: string;
}

export const benchmarkApi = {
  /** Fetch benchmark data (KPIs + cohort percentiles) for the current tenant. */
  get: () =>
    api.get<BenchmarkResponse>('/api/v1/analytics/benchmark'),

  /** Opt in to peer benchmarking with cohort descriptor. */
  optIn: (body: BenchmarkOptInRequest) =>
    api.post<BenchmarkResponse>('/api/v1/analytics/benchmark/opt-in', body),
};

/** Inline approval actions — reassign and nudge via session-authenticated AP Command Center endpoints. */
export const approvalsActions = {
  /** Reassign the current pending approval on an invoice to another user. */
  reassign: (invoiceId: string, newApproverId: string) =>
    api.post<{ ok: boolean }>(
      `/api/v1/dashboard/ap-command-center/${invoiceId}/reassign`,
      { new_approver_id: newApproverId },
    ),

  /** Send a nudge / comment on a stuck approval. */
  nudge: (invoiceId: string, commentBody: string) =>
    api.post<{ ok: boolean }>(
      `/api/v1/dashboard/ap-command-center/${invoiceId}/nudge`,
      { comment_body: commentBody },
    ),
};

// ---------------------------------------------------------------------------
// Audit Log Types
// ---------------------------------------------------------------------------

export interface AuditEntry {
  id: string;
  tenant_id: string;
  user_id: string | null;
  user_email: string | null;
  action: string;
  resource_type: string;
  resource_id: string;
  description: string;
  old_value: unknown | null;
  new_value: unknown | null;
  metadata: unknown | null;
  ip_address: string | null;
  user_agent: string | null;
  request_id: string | null;
  created_at: string;
}

export interface AuditQueryParams {
  page?: number;
  per_page?: number;
  user_id?: string;
  action?: string;
  resource_type?: string;
  resource_id?: string;
  from_date?: string;
  to_date?: string;
}

export interface AuditListResponse {
  data: AuditEntry[];
  pagination: PaginationMeta;
}

// Audit API
export const auditApi = {
  list: (params?: AuditQueryParams) => {
    const qs = new URLSearchParams();
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        if (v !== undefined) qs.set(k, String(v));
      }
    }
    return api.get<AuditListResponse>(`/api/v1/audit?${qs}`);
  },
};

// ---------------------------------------------------------------------------
// Billing Types
// ---------------------------------------------------------------------------

export interface BillingPlanModule {
  id: string;
  name: string;
  enabled: boolean;
}

export type BillingModule =
  | 'invoice_capture'
  | 'invoice_processing'
  | 'vendor_management'
  | 'reporting'
  | 'ai_assistant';

export interface BillingPlanFeatures {
  max_users: number;
  max_invoices_per_month: number;
  max_vendors: number;
  storage_gb: number;
  modules: BillingModule[] | BillingPlanModule[];
  advanced_ocr: boolean;
  api_access: boolean;
  custom_workflows: boolean;
  priority_support: boolean;
  sso_enabled: boolean;
  audit_log_retention_days: number;
}

export interface BillingPlan {
  id: string;
  name: string;
  description: string;
  monthly_price_cents: number;
  annual_price_cents: number;
  features: BillingPlanFeatures;
  stripe_monthly_price_id: string | null;
  stripe_annual_price_id: string | null;
  is_public: boolean;
}

export interface BillingSubscription {
  id: string;
  tenant_id: string;
  plan_id: string;
  status: string;
  billing_cycle: string;
  add_on_modules: BillingModule[];
  started_at: string;
  current_period_start: string;
  current_period_end: string;
  canceled_at: string | null;
  trial_end: string | null;
  stripe_subscription_id: string | null;
  stripe_customer_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface BillingModuleAddOn {
  module: BillingModule;
  name: string;
  description: string;
  monthly_price_cents: number;
  annual_price_cents: number;
  stripe_monthly_price_id: string | null;
  stripe_annual_price_id: string | null;
}

export interface BillingQuote {
  base_plan: string;
  base_monthly_cents: number;
  base_annual_cents: number;
  addon_modules: BillingModule[];
  addon_monthly_cents: number;
  addon_annual_cents: number;
  total_monthly_cents: number;
  total_annual_cents: number;
}

// Billing API
export const billingApi = {
  listPlans: () =>
    api.get<{ plans: BillingPlan[] }>('/api/v1/billing/plans'),

  listModuleAddons: () =>
    api.get<{ module_addons: BillingModuleAddOn[] }>('/api/v1/billing/module-addons'),

  quote: (data: { plan_id: string; add_on_modules?: BillingModule[] }) =>
    api.post<{ quote: BillingQuote }>('/api/v1/billing/quote', data),

  getSubscription: () =>
    api.get<{ subscription: BillingSubscription }>('/api/v1/billing/subscription'),

  createCheckout: (data: {
    plan_id: string;
    billing_cycle?: string;
    add_on_modules?: BillingModule[];
  }) =>
    api.post<{ mode: string; url: string }>('/api/v1/billing/checkout', data),
};

/**
 * Brief plan shape returned by the unauthenticated public pricing endpoint
 * (GET /api/public/plans). Mirrors `PublicPlan` / `PlanFeaturesBrief` in
 * `backend/crates/api/src/routes/public_signup.rs`. Only public plans
 * (is_public=true) are returned, so Enterprise is never present.
 */
export interface PublicPlan {
  id: string;
  name: string;
  description: string;
  monthly_price_cents: number;
  annual_price_cents: number;
  metered_invoice_unit_price_cents: number;
  features: {
    max_users: number;
    max_invoices_per_month: number;
    max_vendors: number;
  };
}

/**
 * Public billing API - no auth headers required. Used by the marketing
 * pricing page to source plan data directly from the backend so prices
 * cannot drift from `backend/crates/billing/src/plans.rs`.
 */
export const publicBillingApi = {
  listPlans: () => api.get<PublicPlan[]>('/api/public/plans'),
};

// ---------------------------------------------------------------------------
// Notifications Types
// ---------------------------------------------------------------------------

export interface SlackInstallResponse {
  authorize_url: string;
  state: string;
}

export interface SlackCallbackResponse {
  success: boolean;
  slack_team_name: string;
}

export interface SlackStatus {
  slack_team_id: string;
  slack_team_name: string;
  installed_at: string;
  is_active: boolean;
}

export interface TeamsConfigureResponse {
  success: boolean;
  webhook_id: string;
}

export interface TeamsStatus {
  id: string;
  channel_name: string | null;
  created_at: string;
  is_active: boolean;
}

export interface NotificationPreference {
  channel: string;
  enabled: boolean;
  notification_types: string[] | null;
  priority_filter: string | null;
  quiet_hours_start: string | null;
  quiet_hours_end: string | null;
  quiet_hours_timezone: string | null;
}

export interface UpdateNotificationPreferencesInput {
  channel: string;
  enabled: boolean;
  notification_types?: string[];
  priority_filter?: string;
  quiet_hours_start?: string;
  quiet_hours_end?: string;
  quiet_hours_timezone?: string;
}

// ---------------------------------------------------------------------------
// In-app notification inbox (refs #375)
//
// Mirrors the JSON shape returned by GET /api/v1/notifications. The bell in
// notification-center.tsx consumes a `Notification` with a small fixed set of
// `type` values ('info' | 'success' | 'warning' | 'error'), so callers map
// `kind` -> `type` at the call site (see apps/web/src/app/(dashboard)/layout.tsx).
// ---------------------------------------------------------------------------

export interface InAppNotification {
  id: string;
  kind: string;
  title: string;
  message: string | null;
  link: string | null;
  read: boolean;
  created_at: string;
}

export interface InAppNotificationFeed {
  items: InAppNotification[];
  unread_count: number;
}

// Notifications API
export const notificationsApi = {
  // In-app inbox feed (refs #375)
  list: () => api.get<InAppNotificationFeed>('/api/v1/notifications'),

  markRead: (id: string) =>
    api.post<{ success: boolean }>(`/api/v1/notifications/${encodeURIComponent(id)}/read`),

  markAllRead: () =>
    api.post<{ success: boolean }>('/api/v1/notifications/read-all'),

  remove: (id: string) =>
    api.delete<{ success: boolean }>(`/api/v1/notifications/${encodeURIComponent(id)}`),

  // Slack
  installSlack: (redirectUrl?: string) => {
    const qs = redirectUrl ? `?redirect_url=${encodeURIComponent(redirectUrl)}` : '';
    return api.post<SlackInstallResponse>(`/api/v1/notifications/slack/install${qs}`);
  },

  slackCallback: (code: string, state: string) =>
    api.get<SlackCallbackResponse>(`/api/v1/notifications/slack/callback?code=${encodeURIComponent(code)}&state=${encodeURIComponent(state)}`),

  getSlackStatus: () =>
    api.get<SlackStatus | null>('/api/v1/notifications/slack/status'),

  disconnectSlack: () =>
    api.post<{ success: boolean }>('/api/v1/notifications/slack/disconnect'),

  // Teams
  configureTeams: (data: { webhook_url: string; channel_name?: string }) =>
    api.post<TeamsConfigureResponse>('/api/v1/notifications/teams/configure', data),

  getTeamsStatus: () =>
    api.get<TeamsStatus | null>('/api/v1/notifications/teams/status'),

  disconnectTeams: () =>
    api.post<{ success: boolean }>('/api/v1/notifications/teams/disconnect'),

  // Preferences
  getPreferences: () =>
    api.get<NotificationPreference[]>('/api/v1/notifications/preferences'),

  updatePreferences: (data: UpdateNotificationPreferencesInput) =>
    api.put<{ success: boolean }>('/api/v1/notifications/preferences', data),
};

// ---------------------------------------------------------------------------
// Predictive Analytics Types
// ---------------------------------------------------------------------------

export type PredictiveEntityType = 'vendor' | 'department' | 'gl_code' | 'tenant' | 'approver';
export type ForecastHorizon = 'days_30' | 'days_60' | 'days_90';
export type AnomalyType = 'invoice_amount_outlier' | 'duplicate_invoice' | 'vendor_volume_spike' | 'approval_time_anomaly' | 'budget_threshold' | 'vendor_concentration';
export type AnomalySeverity = 'low' | 'medium' | 'high' | 'critical';

export interface Forecast {
  entity_id: string;
  entity_type: PredictiveEntityType;
  metric_name: string;
  horizon: ForecastHorizon;
  predicted_value: number;
  confidence_lower: number;
  confidence_upper: number;
  confidence_level: number;
  generated_at: string;
  model_version: string;
  seasonality_detected: boolean;
}

export interface ForecastQuery {
  entity_type: string;
  entity_id: string;
  horizon: string;
}

export interface Anomaly {
  id: string;
  tenant_id: string;
  anomaly_type: AnomalyType;
  entity_id: string;
  entity_type: PredictiveEntityType;
  severity: AnomalySeverity;
  detected_value: number;
  expected_range: [number, number];
  deviation_score: number;
  detected_at: string;
  metadata: Record<string, unknown>;
  acknowledged: boolean;
  acknowledged_at: string | null;
  acknowledged_by: string | null;
}

export interface BudgetAlert {
  id: string;
  alert_type: string;
  severity: string;
  entity_id: string | null;
  entity_type: string | null;
  title: string;
  message: string;
  threshold_value: number | null;
  current_value: number | null;
  threshold_percentage: number | null;
  recommended_action: string | null;
  triggered_at: string;
  dismissed: boolean;
}

export interface AnomalyRule {
  id: string;
  entity_type: string | null;
  entity_id: string | null;
  anomaly_type: string;
  zscore_threshold: number | null;
  iqr_multiplier: number | null;
  volume_spike_threshold: number | null;
  notification_channels: string[] | null;
  notify_on_severity: string[] | null;
  enabled: boolean;
}

export interface ConfigureAnomalyRuleInput {
  entity_type?: string;
  entity_id?: string;
  anomaly_type: string;
  zscore_threshold?: number;
  iqr_multiplier?: number;
  volume_spike_threshold?: number;
  notification_channels?: string[];
  notify_on_severity?: string[];
  enabled?: boolean;
}

// Predictive Analytics API
export const predictiveApi = {
  // Forecasts
  getForecasts: (query: ForecastQuery) => {
    const qs = new URLSearchParams({
      entity_type: query.entity_type,
      entity_id: query.entity_id,
      horizon: query.horizon,
    });
    return api.get<Forecast[]>(`/api/v1/analytics/predictive/forecasts?${qs}`);
  },

  generateForecast: (query: ForecastQuery) =>
    api.post<Forecast>('/api/v1/analytics/predictive/forecasts/generate', query),

  getForecastById: (id: string) =>
    api.get<Forecast>(`/api/v1/analytics/predictive/forecasts/${id}`),

  // Anomalies
  getAnomalies: () =>
    api.get<Anomaly[]>('/api/v1/analytics/predictive/anomalies'),

  acknowledgeAnomaly: (id: string, notes?: string) =>
    api.post<null>(`/api/v1/analytics/predictive/anomalies/${id}/acknowledge`, { notes }),

  detectAnomalies: () =>
    api.post<Anomaly[]>('/api/v1/analytics/predictive/anomalies/detect'),

  // Budget Alerts
  getBudgetAlerts: () =>
    api.get<BudgetAlert[]>('/api/v1/analytics/predictive/alerts'),

  dismissAlert: (id: string) =>
    api.post<null>(`/api/v1/analytics/predictive/alerts/${id}/dismiss`),

  // Anomaly Rules
  getAnomalyRules: () =>
    api.get<AnomalyRule[]>('/api/v1/analytics/predictive/rules'),

  configureAnomalyRule: (data: ConfigureAnomalyRuleInput) =>
    api.post<string>('/api/v1/analytics/predictive/rules', data),

  getAnomalyRule: (id: string) =>
    api.get<AnomalyRule>(`/api/v1/analytics/predictive/rules/${id}`),

  updateAnomalyRule: (id: string, data: ConfigureAnomalyRuleInput) =>
    api.post<null>(`/api/v1/analytics/predictive/rules/${id}`, data),
};

// ---------------------------------------------------------------------------
// Tenant Settings Types
// ---------------------------------------------------------------------------

export interface TenantSettings {
  company_name: string;
  timezone: string;
  default_currency: string;
  ocr_provider: string | null;
  logo_url: string | null;
  primary_color: string | null;
  features: {
    advanced_ocr: boolean;
    api_access: boolean;
    custom_workflows: boolean;
    audit_logs: boolean;
    sso_enabled: boolean;
    local_ocr_required: boolean;
  };
}

export interface UpdateTenantSettingsInput {
  company_name?: string;
  timezone?: string;
  default_currency?: string;
  ocr_provider?: string | null;
  features?: TenantSettings['features'];
}

// Settings API
export const settingsApi = {
  get: () =>
    api.get<TenantSettings>('/api/v1/settings'),

  update: (data: UpdateTenantSettingsInput) =>
    api.put<TenantSettings>('/api/v1/settings', data),
};

// ---------------------------------------------------------------------------
// Purchase Order Types
// ---------------------------------------------------------------------------

export interface PurchaseOrderMoney {
  amount: number;
  currency: string;
}

export interface POLineItem {
  id: string;
  line_number: number;
  description: string;
  quantity: number;
  unit_of_measure: string;
  unit_price: PurchaseOrderMoney;
  total: PurchaseOrderMoney;
  product_id: string | null;
  received_quantity: number;
  invoiced_quantity: number;
}

export interface PurchaseOrder {
  id: string;
  tenant_id: string;
  po_number: string;
  vendor_id: string;
  vendor_name: string;
  order_date: string;
  expected_delivery: string | null;
  status: string;
  line_items: POLineItem[];
  total_amount: PurchaseOrderMoney;
  ship_to_address: string | null;
  notes: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CreatePOLineItemInput {
  line_number?: number;
  description: string;
  quantity: number;
  unit_of_measure: string;
  unit_price: PurchaseOrderMoney;
  total: PurchaseOrderMoney;
  product_id?: string;
}

export interface CreatePurchaseOrderInput {
  po_number: string;
  vendor_id: string;
  vendor_name: string;
  order_date: string;
  expected_delivery?: string;
  line_items: CreatePOLineItemInput[];
  total_amount: PurchaseOrderMoney;
  ship_to_address?: string;
  notes?: string;
}

export interface PurchaseOrderListQuery {
  page?: number;
  per_page?: number;
  vendor_id?: string;
  status?: string;
  search?: string;
}

export interface PurchaseOrderListResponse {
  data: PurchaseOrder[];
  pagination: PaginationMeta;
}

export interface MatchTolerancesInput {
  price_variance_pct?: number;
  quantity_variance_pct?: number;
  auto_approve_below_cents?: number;
}

export interface RunMatchRequest {
  invoice_id: string;
  tolerances?: MatchTolerancesInput;
}

export interface PurchaseOrderMatchResponse {
  match_type: string;
  price_variance_pct: number;
  quantity_variance_pct: number;
  match_result_id: string;
  details: Record<string, unknown>;
}

// Purchase Orders API
export const purchaseOrdersApi = {
  list: (params?: PurchaseOrderListQuery) => {
    const qs = new URLSearchParams();
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        if (v !== undefined) qs.set(k, String(v));
      }
    }
    return api.get<PurchaseOrderListResponse>(`/api/v1/edi/purchase-orders?${qs}`);
  },

  create: (data: CreatePurchaseOrderInput) =>
    api.post<PurchaseOrder>('/api/v1/edi/purchase-orders', data),

  get: (id: string) =>
    api.get<PurchaseOrder>(`/api/v1/edi/purchase-orders/${id}`),

  delete: (id: string) =>
    api.delete<{ deleted: boolean }>(`/api/v1/edi/purchase-orders/${id}`),

  runMatch: (id: string, data: RunMatchRequest) =>
    api.post<PurchaseOrderMatchResponse>(`/api/v1/edi/purchase-orders/${id}/match`, data),
};

// ---------------------------------------------------------------------------
// Export Types
// ---------------------------------------------------------------------------

export interface ExportQueryParams {
  start_date?: string;
  end_date?: string;
  status?: string;
  vendor_id?: string;
}

// Export API
export const exportApi = {
  exportInvoicesCsv: (params?: ExportQueryParams) => {
    const qs = new URLSearchParams();
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        if (v !== undefined) qs.set(k, String(v));
      }
    }
    return api.downloadBlob(`/api/v1/export/invoices/csv?${qs}`);
  },

  exportInvoicesJson: (params?: ExportQueryParams) => {
    const qs = new URLSearchParams();
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        if (v !== undefined) qs.set(k, String(v));
      }
    }
    return api.downloadBlob(`/api/v1/export/invoices/json?${qs}`);
  },

  exportVendorsCsv: () =>
    api.downloadBlob('/api/v1/export/vendors/csv'),
};

// ---------------------------------------------------------------------------
// AI Assistant (Winston) Types
// ---------------------------------------------------------------------------

export interface AiMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  created_at: string;
}

export interface AiConversation {
  id: string;
  tenant_id: string;
  user_id: string;
  messages: AiMessage[];
  created_at: string;
  updated_at: string;
}

export interface AiChatRequest {
  message: string;
  conversation_id?: string;
}

export interface AiTraceContextRecord {
  record_type: string;
  label: string;
}

export interface AiTraceToolUsed {
  tool_name: string;
}

export interface AiTraceProvider {
  provider: string;
  model: string;
  model_route?: string;
  finish_reason?: string;
  provider_request_id?: string;
  latency_ms?: number;
  usage?: {
    prompt_tokens?: number;
    completion_tokens?: number;
    total_tokens?: number;
  };
}

export interface AiAnswerTrace {
  context_records: AiTraceContextRecord[];
  tools_used: AiTraceToolUsed[];
  provider: AiTraceProvider;
}

export interface AiChatResponse {
  conversation_id: string;
  message: AiMessage;
  trace: AiAnswerTrace;
}

export type AiAnswerFeedbackRating = 'positive' | 'negative';

export interface AiAnswerFeedbackRequest {
  rating: AiAnswerFeedbackRating;
  comment?: string;
}

export interface AiAnswerFeedbackResponse {
  id: string;
  tenant_id: string;
  user_id: string;
  conversation_id: string;
  message_id: string;
  rating: string;
  comment: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export type AiActionProposalRisk = 'low' | 'medium' | 'high';

export type AiActionProposalStatus =
  | 'pending'
  | 'approved'
  | 'rejected'
  | 'executed'
  | 'failed';

export interface AiPendingActionProposal {
  id: string;
  tenant_id: string;
  user_id: string;
  conversation_id: string;
  tool_name: string;
  payload: Record<string, unknown>;
  risk: AiActionProposalRisk;
  permission: string;
  status: AiActionProposalStatus;
  execution_error_code: string | null;
  execution_error_message: string | null;
  created_at: string;
  updated_at: string;
}

export interface AiActionProposalDecisionRequest {
  reason?: string;
}

// Bug Report Draft Types
export type BugReportPriority = 'low' | 'medium' | 'high' | 'critical';

export interface IssueSourceMetadata {
  source_conversation_id?: string;
  source_conversation_link?: string;
  intake_channel: string;
  issue_kind: string;
}

export interface BugReportDraftRequest {
  description: string;
  conversation_id?: string;
}

export interface BugReportDraftResponse {
  title: string;
  current_behavior: string;
  expected_behavior: string;
  reproduction_steps: string[];
  priority: BugReportPriority;
  affected_module: string;
  acceptance_criteria: string[];
  metadata?: IssueSourceMetadata;
}

// Feature Request Draft Types
export type FeatureRequestPriority = 'low' | 'medium' | 'high' | 'critical';

export interface FeatureRequestDraftRequest {
  description: string;
  conversation_id?: string;
}

export interface FeatureRequestDraftResponse {
  problem_statement: string;
  proposed_value: string;
  affected_module: string;
  priority: FeatureRequestPriority;
  acceptance_criteria: string[];
  metadata?: IssueSourceMetadata;
}

// AI Assistant API
export const aiAssistantApi = {
  chat: (body: AiChatRequest) =>
    api.post<AiChatResponse>('/api/v1/ai/chat', body),

  listConversations: () =>
    api.get<AiConversation[]>('/api/v1/ai/conversations'),

  continueConversation: (id: string, body: { message: string }) =>
    api.post<AiChatResponse>(`/api/v1/ai/conversations/${id}/messages`, body),

  submitAnswerFeedback: (
    conversationId: string,
    messageId: string,
    body: AiAnswerFeedbackRequest,
  ) =>
    api.post<AiAnswerFeedbackResponse>(
      `/api/v1/ai/conversations/${conversationId}/messages/${messageId}/feedback`,
      body,
    ),

  listPendingActionProposals: (conversationId: string) =>
    api.get<AiPendingActionProposal[]>(
      `/api/v1/ai/conversations/${conversationId}/action-proposals/pending`,
    ),

  approveActionProposal: (
    _conversationId: string,
    proposalId: string,
    body: AiActionProposalDecisionRequest = {},
  ) =>
    api.post<AiPendingActionProposal>(
      `/api/v1/ai/action-proposals/${proposalId}/approve`,
      body,
    ),

  rejectActionProposal: (
    _conversationId: string,
    proposalId: string,
    body: AiActionProposalDecisionRequest = {},
  ) =>
    api.post<AiPendingActionProposal>(
      `/api/v1/ai/action-proposals/${proposalId}/reject`,
      body,
    ),

  generateBugReportDraft: (body: BugReportDraftRequest) =>
    api.post<BugReportDraftResponse>('/api/v1/ai/bug-report-drafts', body),

  generateFeatureRequestDraft: (body: FeatureRequestDraftRequest) =>
    api.post<FeatureRequestDraftResponse>('/api/v1/ai/feature-request-drafts', body),
};

// Vendor Portal API (vendor-facing, uses vendor-portal JWT, not user token)
export const vendorPortalApi = {
  submitInvoice: (token: string, body: {
    invoice_number: string;
    invoice_date?: string;
    due_date?: string;
    amount: number;
    currency?: string;
    notes?: string;
  }) =>
    fetch(`${API_BASE_URL}/api/v1/vendor-portal/invoices`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify(body),
    }).then(async (res) => {
      if (!res.ok) {
        const err = await res.json().catch(() => null);
        throw new ApiClientError(res.status, err);
      }
      return res.json() as Promise<{ id: string; invoice_number: string }>;
    }),

  listInvoices: (token: string) =>
    fetch(`${API_BASE_URL}/api/v1/vendor-portal/invoices`, {
      method: 'GET',
      headers: { Authorization: `Bearer ${token}` },
    }).then(async (res) => {
      if (!res.ok) {
        const err = await res.json().catch(() => null);
        throw new ApiClientError(res.status, err);
      }
      return res.json() as Promise<Array<{
        id: string;
        invoice_number: string;
        invoice_date: string | null;
        due_date: string | null;
        total_amount: number;
        currency: string;
        processing_status: string;
      }>>;
    }),

  uploadInvoicePdf: (token: string, formData: FormData) =>
    fetch(`${API_BASE_URL}/api/v1/vendor-portal/invoices/upload`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
      body: formData,
    }).then(async (res) => {
      if (!res.ok) {
        const err = await res.json().catch(() => null);
        throw new ApiClientError(res.status, err);
      }
      return res.json() as Promise<{ id: string; invoice_number: string }>;
    }),

  submitOnboarding: (
    token: string,
    payload: {
      legal_name: string;
      dba?: string;
      address?: Record<string, unknown>;
      tax_form_type: 'w9' | 'w8ben';
      banking?: Record<string, unknown>;
      remit_contacts?: Record<string, unknown>[];
    },
    taxDocument?: File,
  ) => {
    const formData = new FormData();
    formData.append('legal_name', payload.legal_name);
    if (payload.dba) formData.append('dba', payload.dba);
    if (payload.address) formData.append('address', JSON.stringify(payload.address));
    formData.append('tax_form_type', payload.tax_form_type);
    if (payload.banking) formData.append('banking', JSON.stringify(payload.banking));
    if (payload.remit_contacts)
      formData.append('remit_contacts', JSON.stringify(payload.remit_contacts));
    if (taxDocument) formData.append('tax_document', taxDocument);

    return fetch(`${API_BASE_URL}/api/v1/vendor-portal/onboarding`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
      body: formData,
    }).then(async (res) => {
      if (!res.ok) {
        const err = await res.json().catch(() => null);
        throw new ApiClientError(res.status, err);
      }
      return res.json() as Promise<{ submission_id: string; status: string }>;
    });
  },
};

// ---------------------------------------------------------------------------
// Intelligent Routing types & API
// Mirrors backend/crates/api/src/routes/routing.rs + billforge_core::intelligent_routing
// ---------------------------------------------------------------------------

/** Routing strategy used by the intelligent routing engine */
export type RoutingStrategy =
  | 'least_loaded'
  | 'round_robin'
  | 'expert_based'
  | 'availability_based'
  | 'hybrid'
  | 'fallback';

/** Score breakdown for a candidate approver */
export interface CandidateScore {
  user_id: string;
  score: number;
  workload_score: number;
  expertise_score: number;
  availability_score: number;
  reason: string;
}

/** Factors that influenced a routing decision */
export interface RoutingFactors {
  workload_weight: number;
  expertise_weight: number;
  availability_weight: number;
  invoice_amount: number;
  vendor_id: string | null;
  department: string | null;
  gl_code: string | null;
}

/** Full routing decision returned by the engine */
export interface RoutingDecision {
  approver_id: string | null;
  strategy: RoutingStrategy;
  score: number;
  candidates: CandidateScore[];
  factors: RoutingFactors;
  delegated_from: string | null;
}

/** Request body for routing an invoice */
export interface RouteInvoiceRequest {
  queue_id: string;
}

/** Response from route-invoice endpoint */
export interface RouteInvoiceResponse {
  decision: RoutingDecision;
}

/** Workload distribution stats (mirrors WorkloadDistributionStatsResponse from OpenAPI) */
export type WorkloadDistributionStats = Schemas['WorkloadDistributionStatsResponse'];

/** Summary for a single approver's workload (from OpenAPI) */
export type ApproverWorkloadSummary = Schemas['ApproverWorkloadSummary'];

/** Response from the workload endpoint */
export interface WorkloadResponse {
  stats: WorkloadDistributionStats;
  approvers: ApproverWorkloadSummary[];
}

/** Availability status variants (snake_case over the wire per serde rename) */
export type AvailabilityStatusInput =
  | 'available'
  | 'busy'
  | 'out_of_office'
  | 'vacation';

/** Request body for setting approver availability */
export interface SetAvailabilityRequest {
  user_id: string;
  status: AvailabilityStatusInput;
  start_at: string;
  end_at: string;
  delegate_id?: string | null;
  reason?: string | null;
}

/** Full routing configuration (all 12 fields from RoutingConfigResponse) */
export interface RoutingConfig {
  workload_weight: number;
  expertise_weight: number;
  availability_weight: number;
  max_workload_score: number;
  min_expertise_score: number;
  enable_auto_delegation: boolean;
  enable_pattern_learning: boolean;
  enable_calendar_sync: boolean;
  working_hours_start: string;
  working_hours_end: string;
  working_timezone: string;
  working_days: number[];
}

/** Partial update for routing config (all fields optional) */
export type UpdateRoutingConfigRequest = Partial<RoutingConfig>;

// ---------------------------------------------------------------------------
// Routing Simulation types (what-if rule testing, issue #246)
// ---------------------------------------------------------------------------

/** Partial routing config for simulation - all fields optional (overlays on live config). */
export type SimulationConfigInput = Partial<RoutingConfig>;

/** Per-invoice result of comparing live vs candidate routing. */
export interface SimulatedOutcomeResponse {
  invoice_id: string;
  predicted_approver: string | null;
  live_approver: string | null;
  changed: boolean;
  predicted_cycle_hours: number;
  live_cycle_hours: number;
  would_stall: boolean;
}

/** Aggregate summary returned by the simulation endpoint. */
export interface SimulationSummaryResponse {
  outcomes: SimulatedOutcomeResponse[];
  approver_load_candidate: Record<string, number>;
  approver_load_live: Record<string, number>;
  avg_cycle_hours_candidate: number;
  avg_cycle_hours_live: number;
  stalled_count_candidate: number;
  stalled_count_live: number;
  changed_count: number;
  total_simulated: number;
}

/** Request body for `POST /api/v1/routing/simulate`. */
export interface SimulateRoutingRequest {
  candidate_config: SimulationConfigInput;
  sample_size?: number;
}

/** Typed wrapper for the Intelligent Routing backend module (/api/v1/routing) */
export const routingApi = {
  routeInvoice: (invoiceId: string, body: RouteInvoiceRequest) =>
    api.post<RouteInvoiceResponse>(
      `/api/v1/routing/invoices/${invoiceId}/route`,
      body,
    ),

  getWorkload: () =>
    api.get<WorkloadResponse>('/api/v1/routing/workload'),

  setAvailability: (body: SetAvailabilityRequest) =>
    api.post<void>('/api/v1/routing/availability', body),

  getConfig: () =>
    api.get<RoutingConfig>('/api/v1/routing/config'),

  updateConfig: (body: UpdateRoutingConfigRequest) =>
    api.put<RoutingConfig>('/api/v1/routing/config', body),

  /** Run a what-if simulation: replay recent invoices through a candidate config. */
  simulate: (body: SimulateRoutingRequest) =>
    api.post<SimulationSummaryResponse>('/api/v1/routing/simulate', body),
};

// ---------------------------------------------------------------------------
// Month-End Close Periods
// ---------------------------------------------------------------------------

export interface ClosePeriod {
  id: string;
  tenant_id: string;
  period_label: string;
  period_start: string;
  period_end: string;
  cutoff_date: string;
  status: 'open' | 'cutoff_passed' | 'locked';
  locked_at: string | null;
  locked_by_user_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface RunCloseResponse {
  period_id: string;
  accrual_entries_created: number;
  erp_post_status: 'pending' | 'posted' | 'failed' | 'unsupported';
  erp_post_error: string | null;
}

export interface ReadinessException {
  id: string;
  label: string;
  count: number;
  severity: string;
}

export interface ReadinessTotals {
  total_invoices: number;
  unapproved_invoices: number;
  accruals_drafted: number;
  invoices_needing_accrual: number;
  invoices_missing_gl_coding: number;
  days_until_cutoff: number | null;
}

export interface ReadinessResponse {
  period: ClosePeriod | null;
  score: number | null;
  computed_at: string;
  totals: ReadinessTotals;
  exceptions: ReadinessException[];
}

export const closePeriodsApi = {
  list: () =>
    api.get<ClosePeriod[]>('/api/v1/close-periods'),

  create: (body: {
    period_label: string;
    period_start: string;
    period_end: string;
    cutoff_date: string;
  }) =>
    api.post<ClosePeriod>('/api/v1/close-periods', body),

  update: (id: string, body: { cutoff_date?: string }) =>
    api.patch<ClosePeriod>(`/api/v1/close-periods/${id}`, body),

  runClose: (id: string) =>
    api.post<RunCloseResponse>(`/api/v1/close-periods/${id}/close`, {}),

  readiness: () =>
    api.get<ReadinessResponse>('/api/v1/close-periods/current/readiness'),
};

// ---------------------------------------------------------------------------
// Duplicate Detection Types
// ---------------------------------------------------------------------------

export interface DuplicateSignalBreakdown {
  vendor: number;
  invoice_number: number;
  amount: number;
  date: number;
  line_item_fingerprint: number;
}

export interface DuplicateMatch {
  existing_invoice_id: string;
  score: number;
  severity: string;
  signal_breakdown: DuplicateSignalBreakdown;
}

export interface CreateInvoiceResponse {
  invoice: Invoice;
  potential_duplicates: DuplicateMatch[];
}

// Extend invoicesApi with duplicate detection methods
export const duplicateApi = {
  /** Merge a duplicate invoice into an existing one (soft-deletes the dup). */
  mergeDuplicate: (duplicateInvoiceId: string, keepInvoiceId: string) =>
    api.post<{ success: boolean; action: string; kept_invoice_id: string; discarded_invoice_id: string }>(
      `/api/v1/invoices/${duplicateInvoiceId}/merge-duplicate`,
      { keep_invoice_id: keepInvoiceId },
    ),

  /** Reject the duplicate flag, keeping both invoices. */
  rejectDuplicate: (invoiceId: string) =>
    api.post<{ success: boolean; action: string; invoice_id: string }>(
      `/api/v1/invoices/${invoiceId}/reject-duplicate`,
    ),
};

// ---------------------------------------------------------------------------
// Early-Payment Discount Optimizer
// ---------------------------------------------------------------------------

export interface DiscountWorklistRow {
  invoice_id: string;
  vendor_name: string;
  invoice_number: string;
  amount_cents: number;
  currency: string;
  discount_percent: number;
  discount_days: number;
  net_days: number;
  discount_deadline: string;
  days_remaining: number;
  net_savings_cents: number;
  effective_apr_bps: number;
  recommended: boolean;
}

export interface DiscountWorklistResponse {
  total_potential_savings_cents: number;
  count_recommended: number;
  items: DiscountWorklistRow[];
}

export interface DiscountKpi {
  captured_count_30d: number;
  captured_savings_cents_30d: number;
  missed_count_30d: number;
  missed_savings_cents_30d: number;
  capture_rate_pct: number;
  captured_count_90d: number;
  captured_savings_cents_90d: number;
  missed_count_90d: number;
  missed_savings_cents_90d: number;
}

export interface DiscountCaptureResponse {
  payment_request_id: string;
  invoice_id: string;
  discounted_amount_cents: number;
}

export const discountsApi = {
  worklist: () =>
    api.get<DiscountWorklistResponse>('/api/v1/discounts/worklist'),

  kpi: () =>
    api.get<DiscountKpi>('/api/v1/discounts/kpi'),

  capture: (invoiceId: string) =>
    api.post<DiscountCaptureResponse>(`/api/v1/discounts/${invoiceId}/capture`),
};

// ---------------------------------------------------------------------------
// Budget Guardrails API
// ---------------------------------------------------------------------------

export interface Budget {
  id: string;
  tenant_id: string;
  scope_type: 'department' | 'cost_center' | 'gl_account' | 'project';
  scope_value: string;
  period_type: 'monthly' | 'quarterly' | 'annual';
  period_start: string;
  period_end: string;
  amount_cents: number;
  enforcement: 'warn' | 'block';
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface BudgetCheckResult {
  scope_type: string;
  scope_value: string;
  budget_amount_cents: number;
  committed_cents: number;
  remaining_after_cents: number;
  enforcement: string;
  status: 'ok' | 'warn' | 'block';
}

export interface InvoiceBudgetCheckResult {
  results: BudgetCheckResult[];
  blocked: boolean;
  warnings: BudgetCheckResult[];
  violations: BudgetCheckResult[];
}

export interface CreateBudgetInput {
  scope_type: string;
  scope_value: string;
  period_type: string;
  period_start: string;
  period_end: string;
  amount_cents: number;
  enforcement?: 'warn' | 'block';
}

export interface PatchBudgetInput {
  amount_cents?: number;
  enforcement?: 'warn' | 'block';
}

export const budgetsApi = {
  list: () =>
    api.get<Budget[]>('/api/v1/budgets'),

  create: (data: CreateBudgetInput) =>
    api.post<Budget>('/api/v1/budgets', data),

  update: (id: string, data: PatchBudgetInput) =>
    api.patch<Budget>(`/api/v1/budgets/${id}`, data),

  delete: (id: string) =>
    api.delete<null>(`/api/v1/budgets/${id}`),

  check: (params: { scope_type: string; scope_value: string; date: string; amount_cents: number }) =>
    api.get<BudgetCheckResult>('/api/v1/budgets/check', params),

  checkInvoice: (invoiceId: string) =>
    // This is a client-side convenience; the actual budget check happens server-side at approval time.
    // Exposed for the pre-approval summary UI.
    api.get<InvoiceBudgetCheckResult>(`/api/v1/budgets/check-invoice/${invoiceId}`),
};

// ---------------------------------------------------------------------------
// Policy Composer (NL -> workflow rules)
// ---------------------------------------------------------------------------

export interface ProposedRule {
  name: string;
  description: string;
  priority: number;
  guardrail_kind: 'approval_limit' | 'budget_cap' | 'routing_rule' | 'block';
  condition_json: Record<string, unknown>;
  action_json: Record<string, unknown>;
  summary: string;
}

export interface InvoiceSummary {
  id: string;
  invoice_number?: string;
  vendor_name?: string;
  total_amount_cents?: number;
  processing_status?: string;
  invoice_date?: string;
}

export interface PolicyPreviewResponse {
  matched_count: number;
  total_invoices: number;
  sample_invoices: InvoiceSummary[];
  projected_action_breakdown: Record<string, unknown>;
}

export interface ComposeResponse {
  proposed_rule: ProposedRule;
  preview: PolicyPreviewResponse;
  warnings: string[];
  unparseable_segments: string[];
}

export const policiesApi = {
  compose: (text: string) =>
    api.post<ComposeResponse>('/api/v1/policies/compose', { text }),

  commit: (proposed_rule: ProposedRule, original_text: string) =>
    api.post<{ success: boolean; rule_id: string }>('/api/v1/policies/commit', {
      proposed_rule,
      original_text,
    }),
};
