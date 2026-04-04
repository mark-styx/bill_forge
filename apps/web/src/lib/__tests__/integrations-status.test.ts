import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getIntegrationStatus, IntegrationStatusResponse, api } from '../api';

describe('getIntegrationStatus', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('calls the correct endpoint and returns parsed response when connected', async () => {
    const response: IntegrationStatusResponse = {
      connected: true,
      company_id: 'comp-123',
      company_name: 'Acme Corp',
      last_sync_at: '2026-04-04T12:00:00Z',
      sync_enabled: true,
    };

    vi.spyOn(api, 'get').mockResolvedValueOnce(response);

    const result = await getIntegrationStatus('/api/v1/quickbooks/status');

    expect(api.get).toHaveBeenCalledWith('/api/v1/quickbooks/status');
    expect(result).toEqual(response);
    expect(result.connected).toBe(true);
    expect(result.company_name).toBe('Acme Corp');
  });

  it('returns disconnected response when integration is not connected', async () => {
    const response: IntegrationStatusResponse = {
      connected: false,
    };

    vi.spyOn(api, 'get').mockResolvedValueOnce(response);

    const result = await getIntegrationStatus('/api/v1/xero/status');

    expect(api.get).toHaveBeenCalledWith('/api/v1/xero/status');
    expect(result.connected).toBe(false);
  });

  it('throws on API error, allowing caller to default to disconnected', async () => {
    vi.spyOn(api, 'get').mockRejectedValueOnce(new Error('Network error'));

    await expect(getIntegrationStatus('/api/v1/salesforce/status')).rejects.toThrow('Network error');
  });
});

describe('integration status mapping', () => {
  it('maps connected: true to display status "connected"', () => {
    const liveStatus: IntegrationStatusResponse = { connected: true };
    const displayStatus = liveStatus?.connected ? 'connected' : 'disconnected';
    expect(displayStatus).toBe('connected');
  });

  it('maps connected: false to display status "disconnected"', () => {
    const liveStatus: IntegrationStatusResponse = { connected: false };
    const displayStatus = liveStatus?.connected ? 'connected' : 'disconnected';
    expect(displayStatus).toBe('disconnected');
  });

  it('maps undefined status to display status "disconnected"', () => {
    const liveStatus: IntegrationStatusResponse | undefined = undefined;
    const displayStatus = liveStatus?.connected ? 'connected' : 'disconnected';
    expect(displayStatus).toBe('disconnected');
  });
});
