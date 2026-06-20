import { api } from '@/lib/api';

export interface ExplanationSignal {
  name: string;
  weight: number;
  direction: string;
  value: string;
}

export interface ExplanationCitation {
  kind: string;
  ref: string;
  span: string;
}

export interface ExplanationCounterfactual {
  variable: string;
  current: string;
  alternative: string;
  predicted_outcome: string;
}

export interface ExplanationResponse {
  decision_id: string;
  decision_kind: string;
  inputs: Record<string, unknown>;
  top_signals: ExplanationSignal[];
  citations: ExplanationCitation[];
  counterfactual: ExplanationCounterfactual;
  current_outcome: string;
  rationale_text: string;
}

export interface OverrideRequest {
  corrected_gl_code: string;
  reason?: string;
}

export interface OverrideResponse {
  recorded: boolean;
  correction_type: string;
}

export const getCategorizationExplanation = (invoiceId: string) =>
  api.get<ExplanationResponse>(`/api/v1/explain/categorization/${invoiceId}`);

export const submitOverride = (invoiceId: string, body: OverrideRequest) =>
  api.post<OverrideResponse>(
    `/api/v1/explain/categorization/${invoiceId}/override`,
    body,
  );
