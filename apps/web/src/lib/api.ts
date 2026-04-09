// Use empty string for relative URLs so requests go through Next.js rewrite proxy.
// This ensures LAN/remote access works (browser won't try to hit localhost:8080).
const API_BASE_URL = typeof window !== 'undefined' ? '' : (process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080');

export type { ApiErrorBody } from '@billforge/shared-types';

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

  async get<T>(path: string): Promise<T> {
    return this.request<T>('GET', path);
  }

  async post<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('POST', path, body);
  }

  async put<T>(path: string, body?: unknown): Promise<T> {
    return this.request<T>('PUT', path, body);
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

// Invoices API
export const invoicesApi = {
  list: (params?: {
    page?: number;
    per_page?: number;
    capture_status?: string;
    processing_status?: string;
    vendor_id?: string;
    search?: string;
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

  create: (data: CreateInvoiceInput) =>
    api.post<Invoice>('/api/v1/invoices', data),

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
    }>('/api/v1/invoices/upload', formData);
  },

  submitForProcessing: (id: string) =>
    api.post(`/api/v1/invoices/${id}/submit`),

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

  invoicesByVendor: () =>
    api.get<InvoicesByVendor[]>('/api/v1/reports/invoices/by-vendor'),

  invoiceAging: () => api.get<AgingBucket[]>('/api/v1/reports/invoices/aging'),

  workflowMetrics: () =>
    api.get<WorkflowMetrics>('/api/v1/reports/workflows/metrics'),
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
export interface InvoiceLineItem {
  id: string;
  line_number: number;
  description: string;
  quantity?: number;
  unit_price?: { amount: number; currency: string };
  amount: { amount: number; currency: string };
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
  subtotal?: { amount: number; currency: string };
  tax_amount?: { amount: number; currency: string };
  total_amount: { amount: number; currency: string };
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
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CreateInvoiceInput {
  vendor_name: string;
  invoice_number: string;
  total_amount: { amount: number; currency: string };
}

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

export interface PaginationMeta {
  page: number;
  per_page: number;
  total_items: number;
  total_pages: number;
}

export interface DashboardSummary {
  invoices_pending_review: number;
  invoices_pending_approval: number;
  invoices_ready_for_payment: number;
  total_amount_pending: number;
  vendors_active: number;
  invoices_this_month: number;
}

export interface InvoicesByVendor {
  vendor_id: string;
  vendor_name: string;
  invoice_count: number;
  total_amount: number;
}

export interface AgingBucket {
  bucket: string;
  count: number;
  total_amount: number;
}

export interface WorkflowMetrics {
  avg_processing_time_hours: number;
  avg_approval_time_hours: number;
  auto_approval_rate: number;
  rejection_rate: number;
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
  synced: number;
  created: number;
  updated: number;
  errors: string[];
}

export interface OAuthConnectResponse {
  redirect_url: string;
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
  connect: (credentials: { company_id: string; user_id: string; user_password: string }) =>
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
  connect: () => api.get<OAuthConnectResponse>('/api/v1/bill-com/connect'),
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

// Payment Request types
export interface PaymentRequestItem {
  id: string;
  invoice_id: string;
  invoice_number: string;
  vendor_name: string;
  amount_cents: number;
  currency: string;
  due_date?: string;
}

export interface PaymentRequest {
  id: string;
  request_number: string;
  status: string;
  vendor_id?: string;
  vendor_name?: string;
  total_amount_cents: number;
  currency: string;
  invoice_count: number;
  earliest_due_date?: string;
  latest_due_date?: string;
  items: PaymentRequestItem[];
  notes?: string;
  created_by: string;
  submitted_at?: string;
  created_at: string;
}

export interface PaymentRequestSummary {
  id: string;
  request_number: string;
  status: string;
  vendor_id?: string;
  total_amount_cents: number;
  currency: string;
  invoice_count: number;
  earliest_due_date?: string;
  latest_due_date?: string;
  notes?: string;
  created_by: string;
  submitted_at?: string;
  created_at: string;
}

export interface CreatePaymentRequestInput {
  invoice_ids: string[];
  notes?: string;
}

// Payment Requests API
export const paymentRequestsApi = {
  create: (data: CreatePaymentRequestInput) =>
    api.post<PaymentRequest>('/api/v1/payment-requests', data),

  list: (params?: { page?: number; per_page?: number; status?: string; vendor_id?: string }) => {
    const qs = new URLSearchParams();
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        if (v !== undefined) qs.set(k, String(v));
      }
    }
    return api.get<{
      data: PaymentRequestSummary[];
      pagination: PaginationMeta;
    }>(`/api/v1/payment-requests?${qs}`);
  },

  get: (id: string) => api.get<PaymentRequest>(`/api/v1/payment-requests/${id}`),

  addInvoices: (id: string, invoiceIds: string[]) =>
    api.post<{ success: boolean }>(`/api/v1/payment-requests/${id}/invoices`, { invoice_ids: invoiceIds }),

  submit: (id: string) =>
    api.post<PaymentRequest>(`/api/v1/payment-requests/${id}/submit`),
};

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
