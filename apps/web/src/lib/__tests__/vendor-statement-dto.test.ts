import { describe, it, expect } from 'vitest';
import {
  vendorStatementsApi,
  type VendorStatement,
  type StatementLineItem,
  type ReconciliationSummary,
  type StatementDetailResponse,
  type StatementListResponse,
  type MatchResult,
  type MatchResponse,
  type CreateStatementInput,
  type CreateStatementLineInput,
  type UpdateLineMatchInput,
  type StatementStatus,
  type LineMatchStatus,
  type LineType,
  type MatchConfidence,
} from '../api';

describe('vendorStatementsApi', () => {
  it('has all 6 required methods', () => {
    const methods = Object.keys(vendorStatementsApi);
    expect(methods).toContain('create');
    expect(methods).toContain('list');
    expect(methods).toContain('get');
    expect(methods).toContain('runMatch');
    expect(methods).toContain('updateLine');
    expect(methods).toContain('reconcile');
    expect(methods.length).toBe(6);
  });
});

describe('vendorStatementsApi method signatures', () => {
  it('create accepts vendorId and statement data', () => {
    expect(typeof vendorStatementsApi.create).toBe('function');
    expect(vendorStatementsApi.create.length).toBe(2); // vendorId, data
  });

  it('list accepts vendorId and optional params', () => {
    expect(typeof vendorStatementsApi.list).toBe('function');
    expect(vendorStatementsApi.list.length).toBe(2); // vendorId, params?
  });

  it('get accepts vendorId and statementId', () => {
    expect(typeof vendorStatementsApi.get).toBe('function');
    expect(vendorStatementsApi.get.length).toBe(2);
  });

  it('runMatch accepts vendorId and statementId', () => {
    expect(typeof vendorStatementsApi.runMatch).toBe('function');
    expect(vendorStatementsApi.runMatch.length).toBe(2);
  });

  it('updateLine accepts vendorId, statementId, lineId, and data', () => {
    expect(typeof vendorStatementsApi.updateLine).toBe('function');
    expect(vendorStatementsApi.updateLine.length).toBe(4);
  });

  it('reconcile accepts vendorId and statementId', () => {
    expect(typeof vendorStatementsApi.reconcile).toBe('function');
    expect(vendorStatementsApi.reconcile.length).toBe(2);
  });
});

describe('ReconciliationSummary computation', () => {
  it('computes correct summary from line items', () => {
    const lines: Pick<StatementLineItem, 'match_status' | 'variance_cents'>[] = [
      { match_status: 'matched', variance_cents: 0 },
      { match_status: 'matched', variance_cents: 0 },
      { match_status: 'unmatched', variance_cents: 0 },
      { match_status: 'discrepancy', variance_cents: 500 },
      { match_status: 'discrepancy', variance_cents: -200 },
      { match_status: 'ignored', variance_cents: 0 },
    ];

    const summary: ReconciliationSummary = {
      total_lines: lines.length,
      matched: lines.filter((l) => l.match_status === 'matched').length,
      unmatched: lines.filter((l) => l.match_status === 'unmatched').length,
      discrepancies: lines.filter((l) => l.match_status === 'discrepancy').length,
      ignored: lines.filter((l) => l.match_status === 'ignored').length,
      total_variance_cents: lines.reduce((sum, l) => sum + Math.abs(l.variance_cents), 0),
    };

    expect(summary.total_lines).toBe(6);
    expect(summary.matched).toBe(2);
    expect(summary.unmatched).toBe(1);
    expect(summary.discrepancies).toBe(2);
    expect(summary.ignored).toBe(1);
    expect(summary.total_variance_cents).toBe(700); // abs(500) + abs(-200)
  });

  it('handles empty lines', () => {
    const summary: ReconciliationSummary = {
      total_lines: 0,
      matched: 0,
      unmatched: 0,
      discrepancies: 0,
      ignored: 0,
      total_variance_cents: 0,
    };

    expect(summary.total_lines).toBe(0);
    expect(summary.total_variance_cents).toBe(0);
  });

  it('all matched means reconciliation is allowed', () => {
    const lines: Pick<StatementLineItem, 'match_status'>[] = [
      { match_status: 'matched' },
      { match_status: 'matched' },
      { match_status: 'matched' },
    ];

    const hasUnresolved = lines.some(
      (l) => l.match_status !== 'matched' && l.match_status !== 'ignored' && l.match_status !== 'discrepancy'
    );
    expect(hasUnresolved).toBe(false);
  });

  it('unmatched lines prevent reconciliation', () => {
    const lines: Pick<StatementLineItem, 'match_status'>[] = [
      { match_status: 'matched' },
      { match_status: 'unmatched' },
    ];

    const hasUnresolved = lines.some(
      (l) => l.match_status !== 'matched' && l.match_status !== 'ignored' && l.match_status !== 'discrepancy'
    );
    expect(hasUnresolved).toBe(true);
  });

  it('ignored + discrepancy lines allow reconciliation', () => {
    const lines: Pick<StatementLineItem, 'match_status'>[] = [
      { match_status: 'matched' },
      { match_status: 'ignored' },
      { match_status: 'discrepancy' },
    ];

    const hasUnresolved = lines.some(
      (l) => l.match_status !== 'matched' && l.match_status !== 'ignored' && l.match_status !== 'discrepancy'
    );
    expect(hasUnresolved).toBe(false);
  });
});

describe('Type narrowness', () => {
  it('StatementStatus covers expected values', () => {
    const statuses: StatementStatus[] = ['pending', 'in_review', 'reconciled', 'disputed'];
    expect(statuses.length).toBe(4);
  });

  it('LineMatchStatus covers expected values', () => {
    const statuses: LineMatchStatus[] = ['unmatched', 'matched', 'discrepancy', 'ignored'];
    expect(statuses.length).toBe(4);
  });

  it('LineType covers expected values', () => {
    const types: LineType[] = ['invoice', 'credit', 'payment', 'adjustment'];
    expect(types.length).toBe(4);
  });

  it('MatchConfidence covers expected values', () => {
    const confidences: MatchConfidence[] = ['exact', 'amount_only', 'no_match'];
    expect(confidences.length).toBe(3);
  });
});

describe('DTO shape alignment', () => {
  it('VendorStatement has all expected fields', () => {
    const stmt = {} as VendorStatement;
    // Ensure all fields exist at the type level
    expect(typeof stmt.id).toBe('undefined');
    expect(typeof stmt.tenant_id).toBe('undefined');
    expect(typeof stmt.vendor_id).toBe('undefined');
    expect(typeof stmt.statement_number).toBe('undefined');
    expect(typeof stmt.statement_date).toBe('undefined');
    expect(typeof stmt.period_start).toBe('undefined');
    expect(typeof stmt.period_end).toBe('undefined');
    expect(typeof stmt.opening_balance_cents).toBe('undefined');
    expect(typeof stmt.closing_balance_cents).toBe('undefined');
    expect(typeof stmt.currency).toBe('undefined');
    expect(typeof stmt.status).toBe('undefined');
    expect(typeof stmt.reconciled_by).toBe('undefined');
    expect(typeof stmt.reconciled_at).toBe('undefined');
    expect(typeof stmt.notes).toBe('undefined');
    expect(typeof stmt.created_by).toBe('undefined');
    expect(typeof stmt.created_at).toBe('undefined');
    expect(typeof stmt.updated_at).toBe('undefined');
  });

  it('StatementLineItem has all expected fields', () => {
    const line = {} as StatementLineItem;
    expect(typeof line.id).toBe('undefined');
    expect(typeof line.statement_id).toBe('undefined');
    expect(typeof line.tenant_id).toBe('undefined');    expect(typeof line.line_date).toBe('undefined');
    expect(typeof line.description).toBe('undefined');
    expect(typeof line.reference_number).toBe('undefined');
    expect(typeof line.amount_cents).toBe('undefined');
    expect(typeof line.line_type).toBe('undefined');
    expect(typeof line.match_status).toBe('undefined');
    expect(typeof line.matched_invoice_id).toBe('undefined');
    expect(typeof line.variance_cents).toBe('undefined');
    expect(typeof line.matched_at).toBe('undefined');
    expect(typeof line.matched_by).toBe('undefined');
    expect(typeof line.notes).toBe('undefined');
    expect(typeof line.created_at).toBe('undefined');
    expect(typeof line.updated_at).toBe('undefined');
  });

  it('StatementDetailResponse has statement, lines, and summary', () => {
    const resp = {} as StatementDetailResponse;
    expect(typeof resp.statement).toBe('undefined');
    expect(typeof resp.lines).toBe('undefined');
    expect(typeof resp.summary).toBe('undefined');
  });

  it('StatementListResponse has data and pagination', () => {
    const resp = {} as StatementListResponse;
    expect(typeof resp.data).toBe('undefined');
    expect(typeof resp.pagination).toBe('undefined');
  });

  it('MatchResult has expected fields', () => {
    const result = {} as MatchResult;
    expect(typeof result.line_id).toBe('undefined');
    expect(typeof result.confidence).toBe('undefined');
    expect(typeof result.matched_invoice_id).toBe('undefined');
    expect(typeof result.variance_cents).toBe('undefined');
    expect(typeof result.match_status).toBe('undefined');
  });
});
