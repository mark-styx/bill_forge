import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ediApi, workflowsApi } from '../api';

describe('EDI API pagination', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('ediApi.listDocuments passes pagination query params', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({
        data: [],
        pagination: { page: 2, per_page: 10, total_items: 0, total_pages: 0 },
      }),
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse as Response);

    const result = await ediApi.listDocuments({ page: 2, per_page: 10 });

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const calledUrl = fetchSpy.mock.calls[0][0] as string;
    expect(calledUrl).toContain('page=2');
    expect(calledUrl).toContain('per_page=10');

    expect(result.pagination).toEqual({
      page: 2,
      per_page: 10,
      total_items: 0,
      total_pages: 0,
    });
  });

  it('ediApi.listDocuments works without params', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({
        data: [],
        pagination: { page: 1, per_page: 25, total_items: 0, total_pages: 0 },
      }),
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse as Response);

    await ediApi.listDocuments();

    const calledUrl = fetchSpy.mock.calls[0][0] as string;
    expect(calledUrl).toContain('/api/v1/edi/documents?');
  });

  it('ediApi.listOutbound passes pagination query params', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({
        data: [],
        pagination: { page: 3, per_page: 5, total_items: 50, total_pages: 10 },
      }),
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse as Response);

    await ediApi.listOutbound({ page: 3, per_page: 5 });

    const calledUrl = fetchSpy.mock.calls[0][0] as string;
    expect(calledUrl).toContain('page=3');
    expect(calledUrl).toContain('per_page=5');
  });
});

describe('Workflow API pagination', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('workflowApi.listQueueItems passes pagination params', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({
        data: [],
        pagination: { page: 1, per_page: 25, total_items: 0, total_pages: 0 },
      }),
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse as Response);

    await workflowsApi.listQueueItems('queue-123', { page: 1 });

    const calledUrl = fetchSpy.mock.calls[0][0] as string;
    expect(calledUrl).toContain('/api/v1/workflows/queues/queue-123/items?');
    expect(calledUrl).toContain('page=1');
  });

  it('workflowApi.listQueueItems works without pagination params', async () => {
    const mockResponse = {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({
        data: [],
        pagination: { page: 1, per_page: 25, total_items: 0, total_pages: 0 },
      }),
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse as Response);

    await workflowsApi.listQueueItems('queue-456');

    const calledUrl = fetchSpy.mock.calls[0][0] as string;
    expect(calledUrl).toContain('/api/v1/workflows/queues/queue-456/items?');
  });
});
