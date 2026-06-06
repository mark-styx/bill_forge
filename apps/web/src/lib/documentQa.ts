import { api } from '@/lib/api';
import type { QaResponse } from '@/types/documentQa';

export function askDocument(documentId: string, question: string): Promise<QaResponse> {
  return api.post<QaResponse>(`/api/v1/documents/${documentId}/qa`, { question });
}
