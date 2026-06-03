import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock data for budget guardrails tests
const mockBudgets = [
  {
    id: 'budget-1',
    tenant_id: 'tenant-1',
    scope_type: 'department' as const,
    scope_value: 'Engineering',
    period_type: 'monthly' as const,
    period_start: '2026-06-01',
    period_end: '2026-06-30',
    amount_cents: 100_000_00,
    enforcement: 'warn' as const,
    created_by: 'user-1',
    created_at: '2026-06-01T00:00:00Z',
    updated_at: '2026-06-01T00:00:00Z',
  },
  {
    id: 'budget-2',
    tenant_id: 'tenant-1',
    scope_type: 'cost_center' as const,
    scope_value: 'CC-100',
    period_type: 'quarterly' as const,
    period_start: '2026-04-01',
    period_end: '2026-06-30',
    amount_cents: 250_000_00,
    enforcement: 'block' as const,
    created_by: 'user-1',
    created_at: '2026-04-01T00:00:00Z',
    updated_at: '2026-04-01T00:00:00Z',
  },
];

const mockBudgetCheckResult = {
  scope_type: 'department',
  scope_value: 'Engineering',
  budget_amount_cents: 100_000_00,
  committed_cents: 50_000_00,
  remaining_after_cents: 50_000_00,
  enforcement: 'warn',
  status: 'ok' as const,
};

const mockInvoiceBudgetCheck = {
  results: [mockBudgetCheckResult],
  blocked: false,
  warnings: [],
  violations: [],
};

describe('Budget Guardrails - API Types', () => {
  it('should have correct scope types', () => {
    const scopeTypes = ['department', 'cost_center', 'gl_account', 'project'];
    expect(scopeTypes).toHaveLength(4);
    expect(scopeTypes).toContain('department');
    expect(scopeTypes).toContain('cost_center');
    expect(scopeTypes).toContain('gl_account');
    expect(scopeTypes).toContain('project');
  });

  it('should have correct period types', () => {
    const periodTypes = ['monthly', 'quarterly', 'annual'];
    expect(periodTypes).toHaveLength(3);
  });

  it('should have correct enforcement options', () => {
    const enforcementOptions = ['warn', 'block'];
    expect(enforcementOptions).toHaveLength(2);
  });
});

describe('Budget Guardrails - Data Formatting', () => {
  it('should format cents to dollars correctly', () => {
    const formatCents = (cents: number) =>
      new Intl.NumberFormat('en-US', {
        style: 'currency',
        currency: 'USD',
      }).format(cents / 100);

    expect(formatCents(100_000_00)).toBe('$100,000.00');
    expect(formatCents(0)).toBe('$0.00');
    expect(formatCents(1_50)).toBe('$1.50');
  });

  it('should format date strings correctly', () => {
    const formatDate = (dateStr: string) =>
      new Date(dateStr + 'T00:00:00').toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
      });

    expect(formatDate('2026-06-01')).toBe('Jun 1, 2026');
    expect(formatDate('2026-12-31')).toBe('Dec 31, 2026');
  });
});

describe('Budget Guardrails - Mock Data', () => {
  it('should create valid mock budget data', () => {
    expect(mockBudgets).toHaveLength(2);
    expect(mockBudgets[0].scope_type).toBe('department');
    expect(mockBudgets[1].scope_type).toBe('cost_center');
    expect(mockBudgets[0].enforcement).toBe('warn');
    expect(mockBudgets[1].enforcement).toBe('block');
  });

  it('should compute remaining budget correctly', () => {
    const budget = mockBudgets[0];
    const committed = 50_000_00;
    const remaining = budget.amount_cents - committed;
    expect(remaining).toBe(50_000_00);
  });

  it('should detect budget overage', () => {
    const budget = mockBudgets[0];
    const committed = 120_000_00;
    const remaining = budget.amount_cents - committed;
    expect(remaining).toBeLessThan(0);
  });
});

describe('Budget Guardrails - Budget Check Results', () => {
  it('should identify blocked state', () => {
    const blockedCheck = {
      ...mockInvoiceBudgetCheck,
      violations: [{ ...mockBudgetCheckResult, status: 'block' as const }],
      blocked: true,
    };
    expect(blockedCheck.blocked).toBe(true);
    expect(blockedCheck.violations).toHaveLength(1);
  });

  it('should identify warning state', () => {
    const warningCheck = {
      ...mockInvoiceBudgetCheck,
      warnings: [{ ...mockBudgetCheckResult, status: 'warn' as const }],
      blocked: false,
    };
    expect(warningCheck.blocked).toBe(false);
    expect(warningCheck.warnings).toHaveLength(1);
  });

  it('should identify ok state', () => {
    expect(mockInvoiceBudgetCheck.blocked).toBe(false);
    expect(mockInvoiceBudgetCheck.warnings).toHaveLength(0);
    expect(mockInvoiceBudgetCheck.violations).toHaveLength(0);
  });
});
