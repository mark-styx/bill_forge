import { describe, it, expect, vi, beforeEach } from 'vitest';
import { api } from '../api';
import { aiAssistantApi } from '../api';

describe('aiAssistantApi', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('chat() POSTs to /api/v1/ai/chat and returns AiChatResponse', async () => {
    const response = {
      conversation_id: 'conv-1',
      message: {
        id: 'msg-1',
        role: 'assistant' as const,
        content: 'Hello! How can I help?',
        created_at: '2025-01-01T00:00:00Z',
      },
    };

    vi.spyOn(api, 'post').mockResolvedValueOnce(response);

    const result = await aiAssistantApi.chat({
      message: 'Hi',
      conversation_id: 'conv-1',
    });

    expect(api.post).toHaveBeenCalledWith('/api/v1/ai/chat', {
      message: 'Hi',
      conversation_id: 'conv-1',
    });
    expect(result).toEqual(response);
    expect(result.conversation_id).toBe('conv-1');
    expect(result.message.role).toBe('assistant');
  });

  it('listConversations() GETs /api/v1/ai/conversations', async () => {
    const conversations = [
      {
        id: 'conv-1',
        tenant_id: 't-1',
        user_id: 'u-1',
        messages: [],
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      },
    ];

    vi.spyOn(api, 'get').mockResolvedValueOnce(conversations);

    const result = await aiAssistantApi.listConversations();

    expect(api.get).toHaveBeenCalledWith('/api/v1/ai/conversations');
    expect(result).toEqual(conversations);
    expect(result).toHaveLength(1);
  });

  it('continueConversation() POSTs to /api/v1/ai/conversations/{id}/messages', async () => {
    const response = {
      conversation_id: 'conv-42',
      message: {
        id: 'msg-2',
        role: 'assistant' as const,
        content: 'Follow-up answer',
        created_at: '2025-01-01T00:01:00Z',
      },
    };

    vi.spyOn(api, 'post').mockResolvedValueOnce(response);

    const result = await aiAssistantApi.continueConversation('conv-42', {
      message: 'Tell me more',
    });

    expect(api.post).toHaveBeenCalledWith(
      '/api/v1/ai/conversations/conv-42/messages',
      { message: 'Tell me more' },
    );
    expect(result).toEqual(response);
  });

  it('submitAnswerFeedback() POSTs to feedback endpoint with expected body', async () => {
    const feedbackResponse = {
      id: 'fb-1',
      tenant_id: 't-1',
      user_id: 'u-1',
      conversation_id: 'conv-42',
      message_id: 'msg-2',
      rating: 'positive',
      comment: null,
      metadata: {},
      created_at: '2025-01-01T00:01:00Z',
      updated_at: '2025-01-01T00:01:00Z',
    };

    vi.spyOn(api, 'post').mockResolvedValueOnce(feedbackResponse);

    const result = await aiAssistantApi.submitAnswerFeedback('conv-42', 'msg-2', {
      rating: 'positive',
    });

    expect(api.post).toHaveBeenCalledWith(
      '/api/v1/ai/conversations/conv-42/messages/msg-2/feedback',
      { rating: 'positive' },
    );
    expect(result).toEqual(feedbackResponse);
    expect(result.rating).toBe('positive');
  });

  it('submitAnswerFeedback() passes comment when provided', async () => {
    const feedbackResponse = {
      id: 'fb-2',
      tenant_id: 't-1',
      user_id: 'u-1',
      conversation_id: 'conv-10',
      message_id: 'msg-5',
      rating: 'negative',
      comment: 'Not helpful',
      metadata: {},
      created_at: '2025-01-01T00:02:00Z',
      updated_at: '2025-01-01T00:02:00Z',
    };

    vi.spyOn(api, 'post').mockResolvedValueOnce(feedbackResponse);

    const result = await aiAssistantApi.submitAnswerFeedback('conv-10', 'msg-5', {
      rating: 'negative',
      comment: 'Not helpful',
    });

    expect(api.post).toHaveBeenCalledWith(
      '/api/v1/ai/conversations/conv-10/messages/msg-5/feedback',
      { rating: 'negative', comment: 'Not helpful' },
    );
    expect(result.rating).toBe('negative');
    expect(result.comment).toBe('Not helpful');
  });

  it('generateBugReportDraft() POSTs to /api/v1/ai/bug-report-drafts and returns structured fields', async () => {
    const draftResponse = {
      title: 'Login page crashes on submit',
      current_behavior: 'Page shows a white screen after clicking login',
      expected_behavior: 'User is redirected to the dashboard',
      reproduction_steps: ['Go to /login', 'Enter credentials', 'Click submit'],
      priority: 'high' as const,
      affected_module: 'Authentication',
      acceptance_criteria: [
        'Login submits without crash',
        'User sees dashboard after login',
      ],
    };

    vi.spyOn(api, 'post').mockResolvedValueOnce(draftResponse);

    const result = await aiAssistantApi.generateBugReportDraft({
      description: 'Login page crashes when I click submit.',
    });

    expect(api.post).toHaveBeenCalledWith('/api/v1/ai/bug-report-drafts', {
      description: 'Login page crashes when I click submit.',
    });
    expect(result).toEqual(draftResponse);
    expect(result.title).toBe('Login page crashes on submit');
    expect(result.current_behavior).toBe('Page shows a white screen after clicking login');
    expect(result.expected_behavior).toBe('User is redirected to the dashboard');
    expect(result.reproduction_steps).toHaveLength(3);
    expect(result.priority).toBe('high');
    expect(result.affected_module).toBe('Authentication');
    expect(result.acceptance_criteria).toHaveLength(2);
  });

  it('generateFeatureRequestDraft() POSTs to /api/v1/ai/feature-request-drafts and returns structured fields', async () => {
    const draftResponse = {
      problem_statement: 'Users cannot export invoices in bulk',
      proposed_value: 'Add a bulk export feature supporting CSV and PDF formats',
      affected_module: 'Reporting',
      priority: 'medium' as const,
      acceptance_criteria: [
        'User can select multiple invoices for export',
        'Export completes within 30 seconds for up to 1000 invoices',
      ],
    };

    vi.spyOn(api, 'post').mockResolvedValueOnce(draftResponse);

    const result = await aiAssistantApi.generateFeatureRequestDraft({
      description: 'We need to export many invoices at once instead of one by one.',
    });

    expect(api.post).toHaveBeenCalledWith('/api/v1/ai/feature-request-drafts', {
      description: 'We need to export many invoices at once instead of one by one.',
    });
    expect(result).toEqual(draftResponse);
    expect(result.problem_statement).toBe('Users cannot export invoices in bulk');
    expect(result.proposed_value).toBe('Add a bulk export feature supporting CSV and PDF formats');
    expect(result.affected_module).toBe('Reporting');
    expect(result.priority).toBe('medium');
    expect(result.acceptance_criteria).toHaveLength(2);
  });
});
