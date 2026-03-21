const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080';

interface ApiResponse<T> {
  data?: T;
  error?: {
    code: string;
    message: string;
    details?: unknown;
  };
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
        throw new Error('Session expired. Please login again.');
      }
    }

    if (!response.ok) {
      const error = await response.json().catch(() => ({
        error: { code: 'UNKNOWN', message: 'An error occurred' },
      }));
      throw new Error(error.error?.message || 'Request failed');
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
      const error = await response.json().catch(() => ({
        error: { code: 'UNKNOWN', message: 'Upload failed' },
      }));
      throw new Error(error.error?.message || 'Upload failed');
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
  list: (params?: { page?: number; per_page?: number; status?: string }) =>
    api.get<{
      data: Invoice[];
      pagination: PaginationMeta;
    }>(`/api/v1/invoices?${new URLSearchParams(params as any)}`),

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
  description: string;
  quantity?: number;
  unit_price?: { amount: number; currency: string };
  total_price?: { amount: number; currency: string };
}

export interface Invoice {
  id: string;
  vendor_id?: string;
  vendor_name: string;
  invoice_number: string;
  invoice_date?: string;
  due_date?: string;
  po_number?: string;
  total_amount: { amount: number; currency: string };
  capture_status: string;
  processing_status: string;
  ocr_confidence?: number; // 0.0-1.0 scale (Sprint 3)
  created_at: string;
  description?: string;
  line_items?: InvoiceLineItem[];
}

export interface CreateInvoiceInput {
  vendor_name: string;
  invoice_number: string;
  total_amount: { amount: number; currency: string };
}

export interface Vendor {
  id: string;
  name: string;
  legal_name?: string;
  vendor_type: string;
  status: string;
  email?: string;
  phone?: string;
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

// ──────────────────────────── OCR Pipeline Types ────────────────────────────

export interface OcrJob {
  id: string;
  tenant_id: string;
  document_id: string;
  file_name: string;
  mime_type: string;
  provider: string;
  status: 'pending' | 'processing' | 'completed' | 'failed' | 'cancelled';
  attempt_count: number;
  max_attempts: number;
  result: Record<string, unknown> | null;
  matched_vendor_id: string | null;
  vendor_match_confidence: number | null;
  error_message: string | null;
  processing_time_ms: number | null;
  priority: number;
  created_at: string;
  updated_at: string;
  started_at: string | null;
  completed_at: string | null;
}

export interface BatchUploadResult {
  total_files: number;
  jobs_created: number;
  job_ids: string[];
  errors: { file_name: string; error: string }[];
}

export interface OcrPipelineStats {
  total_jobs: number;
  pending_jobs: number;
  processing_jobs: number;
  completed_jobs: number;
  failed_jobs: number;
  cancelled_jobs: number;
  avg_processing_time_ms: number | null;
  total_corrections: number;
  vendor_match_rate: number | null;
}

export interface VendorAlias {
  id: string;
  tenant_id: string;
  vendor_id: string;
  alias: string;
  is_learned: boolean;
  created_at: string;
}

// ──────────────────────────── Approval Chain Types ────────────────────────────

export interface ApprovalChainLevel {
  id: string;
  policy_id: string;
  level: number;
  name: string;
  approver_type: 'user' | 'role' | 'department_head' | 'manager_chain';
  approver_ids: string[];
  required_approvals: number;
  escalation_timeout_hours: number | null;
  escalate_to: string | null;
  allow_delegation: boolean;
  auto_approve_below: number | null;
  created_at: string;
}

export interface ApprovalPolicy {
  id: string;
  tenant_id: string;
  name: string;
  description: string | null;
  is_active: boolean;
  priority: number;
  match_criteria: Record<string, unknown>;
  require_sequential: boolean;
  allow_self_approval: boolean;
  created_at: string;
  updated_at: string;
  levels?: ApprovalChainLevel[];
}

export interface CreatePolicyInput {
  name: string;
  description?: string;
  is_active: boolean;
  priority: number;
  match_criteria: Record<string, unknown>;
  require_sequential: boolean;
  allow_self_approval: boolean;
  levels: {
    level: number;
    name: string;
    approver_type: string;
    approver_ids: string[];
    required_approvals: number;
    escalation_timeout_hours?: number;
    escalate_to?: string;
    allow_delegation: boolean;
    auto_approve_below?: number;
  }[];
}

export interface ApprovalChain {
  id: string;
  tenant_id: string;
  invoice_id: string;
  policy_id: string;
  status: 'pending' | 'in_progress' | 'approved' | 'rejected' | 'cancelled';
  current_level: number;
  total_levels: number;
  final_decision: string | null;
  final_decided_by: string | null;
  final_decided_at: string | null;
  escalation_count: number;
  last_escalated_at: string | null;
  initiated_by: string;
  initiated_at: string;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface ApprovalChainStep {
  id: string;
  chain_id: string;
  level: number;
  approver_id: string;
  status: string;
  decision: string | null;
  comments: string | null;
  decided_at: string | null;
  delegated_from: string | null;
  delegated_reason: string | null;
  escalated: boolean;
  due_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface ApprovalChainDetail {
  chain: ApprovalChain;
  policy: ApprovalPolicy;
  steps: ApprovalChainStep[];
  activity: ApprovalActivity[];
}

export interface PendingApproval {
  step_id: string;
  chain_id: string;
  invoice_id: string;
  level: number;
  policy_name: string;
  due_at: string | null;
  escalated: boolean;
  created_at: string;
}

export interface ApprovalActivity {
  id: string;
  tenant_id: string;
  chain_id: string;
  step_id: string | null;
  invoice_id: string;
  action: string;
  actor_id: string;
  actor_role: string | null;
  comments: string | null;
  metadata: Record<string, unknown> | null;
  ip_address: string | null;
  created_at: string;
}

export interface EscalationResult {
  escalated_count: number;
  steps: ApprovalChainStep[];
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

// OCR Pipeline API
export const ocrPipelineApi = {
  // Batch upload files for OCR processing
  batchUpload: (files: File[]) => {
    const formData = new FormData();
    files.forEach((file) => formData.append('files', file));
    return api.upload<BatchUploadResult>('/api/v1/ocr/upload', formData);
  },

  // List OCR jobs
  listJobs: (params?: { status?: string; limit?: number; offset?: number }) => {
    const query = params ? `?${new URLSearchParams(Object.entries(params).filter(([, v]) => v != null).map(([k, v]) => [k, String(v)]))}` : '';
    return api.get<OcrJob[]>(`/api/v1/ocr/jobs${query}`);
  },

  // Get single OCR job
  getJob: (id: string) =>
    api.get<OcrJob>(`/api/v1/ocr/jobs/${id}`),

  // Retry a failed job
  retryJob: (id: string) =>
    api.post<OcrJob>(`/api/v1/ocr/jobs/${id}/retry`),

  // Cancel a pending/processing job
  cancelJob: (id: string) =>
    api.post<OcrJob>(`/api/v1/ocr/jobs/${id}/cancel`),

  // Trigger processing of next queued job
  processNext: () =>
    api.post<{ job: OcrJob | null }>('/api/v1/ocr/process'),

  // Get OCR pipeline statistics
  getStats: () =>
    api.get<OcrPipelineStats>('/api/v1/ocr/stats'),

  // Record a user correction to an OCR field
  recordCorrection: (jobId: string, data: { field_name: string; original_value?: string; corrected_value: string }) =>
    api.post<{ success: boolean }>(`/api/v1/ocr/jobs/${jobId}/corrections`, data),

  // List vendor aliases
  listVendorAliases: () =>
    api.get<VendorAlias[]>('/api/v1/ocr/vendor-aliases'),
};

// Approval Chains API
export const approvalChainsApi = {
  // List approval policies
  listPolicies: () =>
    api.get<ApprovalPolicy[]>('/api/v1/approval-chains/policies'),

  // Create approval policy
  createPolicy: (data: CreatePolicyInput) =>
    api.post<ApprovalPolicy>('/api/v1/approval-chains/policies', data),

  // Get single policy
  getPolicy: (id: string) =>
    api.get<ApprovalPolicy>(`/api/v1/approval-chains/policies/${id}`),

  // Update policy (partial)
  updatePolicy: (id: string, data: Partial<CreatePolicyInput>) =>
    api.put<ApprovalPolicy>(`/api/v1/approval-chains/policies/${id}`, data),

  // Delete policy
  deletePolicy: (id: string) =>
    api.delete(`/api/v1/approval-chains/policies/${id}`),

  // List active approval chains
  listChains: (params?: { status?: string; limit?: number; offset?: number }) => {
    const query = params ? `?${new URLSearchParams(Object.entries(params).filter(([, v]) => v != null).map(([k, v]) => [k, String(v)]))}` : '';
    return api.get<ApprovalChain[]>(`/api/v1/approval-chains/chains${query}`);
  },

  // Get chain detail
  getChainDetail: (id: string) =>
    api.get<ApprovalChainDetail>(`/api/v1/approval-chains/chains/${id}`),

  // Recall (cancel) a chain
  recallChain: (id: string) =>
    api.post<ApprovalChain>(`/api/v1/approval-chains/chains/${id}/recall`),

  // Approve a step
  approveStep: (stepId: string, data?: { comments?: string }) =>
    api.post<ApprovalChainStep>(`/api/v1/approval-chains/steps/${stepId}/approve`, data || {}),

  // Reject a step
  rejectStep: (stepId: string, data?: { comments?: string }) =>
    api.post<ApprovalChainStep>(`/api/v1/approval-chains/steps/${stepId}/reject`, data || {}),

  // Delegate a step
  delegateStep: (stepId: string, data: { delegate_to: string; reason?: string }) =>
    api.post<ApprovalChainStep>(`/api/v1/approval-chains/steps/${stepId}/delegate`, data),

  // Get current user's pending approvals
  myPendingApprovals: () =>
    api.get<PendingApproval[]>('/api/v1/approval-chains/pending'),

  // Submit invoice for approval
  submitForApproval: (invoiceId: string, data: { amount_cents: number; department?: string; vendor_id?: string }) =>
    api.post<ApprovalChain>(`/api/v1/approval-chains/submit/${invoiceId}`, data),

  // Process overdue escalations (admin)
  escalateOverdue: () =>
    api.post<EscalationResult>('/api/v1/approval-chains/escalate'),

  // Get activity log for an invoice
  getActivity: (invoiceId: string) =>
    api.get<ApprovalActivity[]>(`/api/v1/approval-chains/activity/${invoiceId}`),
};
