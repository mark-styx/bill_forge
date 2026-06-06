export interface Citation {
  id: number;
  page: number;
  bbox: [number, number, number, number];
  quote: string;
}

export interface QaResponse {
  answer: string;
  citations: Citation[];
}

export interface QaMessage {
  role: 'user' | 'assistant';
  text: string;
  citations?: Citation[];
}
