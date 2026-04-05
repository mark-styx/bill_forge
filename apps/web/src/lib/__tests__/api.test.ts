import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ApiClientError, api } from '../api';

describe('ApiClientError', () => {
  it('preserves status, code, body, and fieldErrors from backend error', () => {
    const body = {
      error: {
        code: 'VALIDATION_ERROR',
        message: 'Validation failed',
        field_errors: {
          email: ['is required'],
          amount: ['must be positive'],
        },
      },
    };

    const err = new ApiClientError(422, body);

    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(ApiClientError);
    expect(err.status).toBe(422);
    expect(err.code).toBe('VALIDATION_ERROR');
    expect(err.body).toBe(body);
    expect(err.fieldErrors).toEqual({
      email: ['is required'],
      amount: ['must be positive'],
    });
  });

  it('has a backward-compatible .message property', () => {
    const err = new ApiClientError(400, {
      error: { code: 'BAD_REQUEST', message: 'Something went wrong' },
    });
    expect(err.message).toBe('Something went wrong');
  });

  it('falls back to status-based message when body is null', () => {
    const err = new ApiClientError(500, null);
    expect(err.message).toBe('API error 500');
    expect(err.code).toBe('UNKNOWN');
    expect(err.body).toBeNull();
    expect(err.fieldErrors).toBeUndefined();
  });

  it('handles body without field_errors', () => {
    const err = new ApiClientError(404, {
      error: { code: 'NOT_FOUND', message: 'Resource not found' },
    });
    expect(err.code).toBe('NOT_FOUND');
    expect(err.fieldErrors).toBeUndefined();
  });
});

describe('ApiClient request error handling', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('throws ApiClientError on non-2xx response preserving error code and status', async () => {
    const errorBody = {
      error: {
        code: 'NOT_FOUND',
        message: 'Invoice not found',
      },
    };

    vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
      ok: false,
      status: 404,
      json: async () => errorBody,
    } as Response);

    try {
      await api.get('/api/v1/invoices/nonexistent');
      expect.fail('should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ApiClientError);
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(404);
      expect(apiErr.code).toBe('NOT_FOUND');
      expect(apiErr.message).toBe('Invoice not found');
      expect(apiErr.body).toEqual(errorBody);
    }
  });

  it('throws ApiClientError with null body for non-JSON error responses', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
      ok: false,
      status: 502,
      json: async () => {
        throw new SyntaxError('Unexpected token');
      },
    } as unknown as Response);

    try {
      await api.get('/api/v1/invoices');
      expect.fail('should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ApiClientError);
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(502);
      expect(apiErr.body).toBeNull();
      expect(apiErr.code).toBe('UNKNOWN');
      expect(apiErr.message).toBe('API error 502');
    }
  });

  it('preserves field_errors in validation error responses', async () => {
    const validationBody = {
      error: {
        code: 'VALIDATION_ERROR',
        message: 'Validation failed',
        field_errors: {
          vendor_name: ['is required'],
          total_amount: ['must be positive'],
        },
      },
    };

    vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
      ok: false,
      status: 422,
      json: async () => validationBody,
    } as Response);

    try {
      await api.post('/api/v1/invoices', {});
      expect.fail('should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ApiClientError);
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(422);
      expect(apiErr.code).toBe('VALIDATION_ERROR');
      expect(apiErr.fieldErrors).toEqual({
        vendor_name: ['is required'],
        total_amount: ['must be positive'],
      });
    }
  });
});

describe('ApiClient upload error handling', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('throws ApiClientError on upload failure', async () => {
    const errorBody = {
      error: {
        code: 'UNSUPPORTED_MEDIA_TYPE',
        message: 'File type not supported',
      },
    };

    vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
      ok: false,
      status: 415,
      json: async () => errorBody,
    } as Response);

    const formData = new FormData();
    formData.append('file', new Blob(['test']), 'test.txt');

    try {
      await api.upload('/api/v1/documents', formData);
      expect.fail('should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(ApiClientError);
      const apiErr = err as ApiClientError;
      expect(apiErr.status).toBe(415);
      expect(apiErr.code).toBe('UNSUPPORTED_MEDIA_TYPE');
      expect(apiErr.message).toBe('File type not supported');
    }
  });
});
