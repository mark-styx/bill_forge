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
});
