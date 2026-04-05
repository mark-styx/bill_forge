import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  quickbooksApi,
  xeroApi,
  sageIntacctApi,
  salesforceApi,
  workdayApi,
  billComApi,
  ediApi,
  AccountMapping,
  SyncResult,
  OAuthConnectResponse,
} from '../api';

type ApiMethods = Record<string, (...args: unknown[]) => unknown>;

const namespaces: [string, ApiMethods][] = [
  ['quickbooksApi', quickbooksApi as unknown as ApiMethods],
  ['xeroApi', xeroApi as unknown as ApiMethods],
  ['sageIntacctApi', sageIntacctApi as unknown as ApiMethods],
  ['salesforceApi', salesforceApi as unknown as ApiMethods],
  ['workdayApi', workdayApi as unknown as ApiMethods],
  ['billComApi', billComApi as unknown as ApiMethods],
  ['ediApi', ediApi as unknown as ApiMethods],
];

describe('Integration API namespaces', () => {
  it('exports all 7 namespace objects', () => {
    expect(typeof quickbooksApi).toBe('object');
    expect(typeof xeroApi).toBe('object');
    expect(typeof sageIntacctApi).toBe('object');
    expect(typeof salesforceApi).toBe('object');
    expect(typeof workdayApi).toBe('object');
    expect(typeof billComApi).toBe('object');
    expect(typeof ediApi).toBe('object');
  });

  it('every method in every namespace is a function', () => {
    for (const [name, ns] of namespaces) {
      for (const key of Object.keys(ns)) {
        expect(typeof ns[key]).toBe('function');
      }
    }
  });
});

describe('quickbooksApi', () => {
  it('has expected method names', () => {
    const methods = Object.keys(quickbooksApi);
    expect(methods).toContain('connect');
    expect(methods).toContain('disconnect');
    expect(methods).toContain('status');
    expect(methods).toContain('callback');
    expect(methods).toContain('syncVendors');
    expect(methods).toContain('syncAccounts');
    expect(methods).toContain('exportInvoice');
    expect(methods).toContain('getAccountMappings');
    expect(methods).toContain('updateAccountMappings');
  });
});

describe('xeroApi', () => {
  it('has expected method names', () => {
    const methods = Object.keys(xeroApi);
    expect(methods).toContain('connect');
    expect(methods).toContain('disconnect');
    expect(methods).toContain('status');
    expect(methods).toContain('callback');
    expect(methods).toContain('syncContacts');
    expect(methods).toContain('syncAccounts');
    expect(methods).toContain('exportInvoice');
    expect(methods).toContain('getAccountMappings');
    expect(methods).toContain('updateAccountMappings');
  });
});

describe('sageIntacctApi', () => {
  it('has expected method names', () => {
    const methods = Object.keys(sageIntacctApi);
    expect(methods).toContain('connect');
    expect(methods).toContain('disconnect');
    expect(methods).toContain('status');
    expect(methods).toContain('syncVendors');
    expect(methods).toContain('syncAccounts');
    expect(methods).toContain('exportInvoice');
    expect(methods).toContain('getAccountMappings');
    expect(methods).toContain('updateAccountMappings');
    expect(methods).toContain('getEntities');
  });
});

describe('salesforceApi', () => {
  it('has expected method names', () => {
    const methods = Object.keys(salesforceApi);
    expect(methods).toContain('connect');
    expect(methods).toContain('disconnect');
    expect(methods).toContain('status');
    expect(methods).toContain('callback');
    expect(methods).toContain('syncAccounts');
    expect(methods).toContain('syncContacts');
    expect(methods).toContain('getAccountMappings');
    expect(methods).toContain('updateAccountMappings');
  });
});

describe('workdayApi', () => {
  it('has expected method names', () => {
    const methods = Object.keys(workdayApi);
    expect(methods).toContain('connect');
    expect(methods).toContain('disconnect');
    expect(methods).toContain('status');
    expect(methods).toContain('callback');
    expect(methods).toContain('syncSuppliers');
    expect(methods).toContain('syncAccounts');
    expect(methods).toContain('exportInvoice');
    expect(methods).toContain('getAccountMappings');
    expect(methods).toContain('updateAccountMappings');
    expect(methods).toContain('getCompanies');
  });
});

describe('billComApi', () => {
  it('has expected method names', () => {
    const methods = Object.keys(billComApi);
    expect(methods).toContain('connect');
    expect(methods).toContain('disconnect');
    expect(methods).toContain('status');
    expect(methods).toContain('syncVendors');
    expect(methods).toContain('pushBill');
    expect(methods).toContain('payBill');
    expect(methods).toContain('payBulk');
    expect(methods).toContain('listPayments');
    expect(methods).toContain('listFundingAccounts');
  });
});

describe('ediApi', () => {
  it('has expected method names', () => {
    const methods = Object.keys(ediApi);
    expect(methods).toContain('connect');
    expect(methods).toContain('disconnect');
    expect(methods).toContain('status');
    expect(methods).toContain('webhookInbound');
    expect(methods).toContain('listDocuments');
    expect(methods).toContain('getDocument');
    expect(methods).toContain('sendRemittance');
    expect(methods).toContain('listOutbound');
    expect(methods).toContain('getAckTimeouts');
    expect(methods).toContain('listPartners');
    expect(methods).toContain('createPartner');
    expect(methods).toContain('updatePartner');
    expect(methods).toContain('deletePartner');
  });
});

// Verify correct URL paths via fetch mock
describe('Integration API URL paths', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  function mockOk() {
    return vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
      ok: true,
      status: 200,
      text: async () => JSON.stringify({}),
    } as Response);
  }

  it('quickbooksApi.status hits /api/v1/quickbooks/status', async () => {
    const spy = mockOk();
    await quickbooksApi.status();
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/quickbooks/status'),
      expect.anything(),
    );
  });

  it('xeroApi.syncContacts hits /api/v1/xero/sync/contacts', async () => {
    const spy = mockOk();
    await xeroApi.syncContacts();
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/xero/sync/contacts'),
      expect.anything(),
    );
  });

  it('sageIntacctApi.getEntities hits /api/v1/sage-intacct/entities', async () => {
    const spy = mockOk();
    await sageIntacctApi.getEntities();
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/sage-intacct/entities'),
      expect.anything(),
    );
  });

  it('salesforceApi.syncAccounts hits /api/v1/salesforce/sync/accounts', async () => {
    const spy = mockOk();
    await salesforceApi.syncAccounts();
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/salesforce/sync/accounts'),
      expect.anything(),
    );
  });

  it('workdayApi.getCompanies hits /api/v1/workday/companies', async () => {
    const spy = mockOk();
    await workdayApi.getCompanies();
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/workday/companies'),
      expect.anything(),
    );
  });

  it('billComApi.payBill hits /api/v1/bill-com/pay/bill/:id', async () => {
    const spy = mockOk();
    await billComApi.payBill('abc-123');
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/bill-com/pay/bill/abc-123'),
      expect.anything(),
    );
  });

  it('ediApi.deletePartner hits /api/v1/edi/partners/:id with DELETE', async () => {
    const spy = mockOk();
    await ediApi.deletePartner('partner-1');
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/edi/partners/partner-1'),
      expect.objectContaining({ method: 'DELETE' }),
    );
  });
});
