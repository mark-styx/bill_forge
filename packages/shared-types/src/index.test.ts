import { describe, it, expectTypeOf } from 'vitest';
import type { ApiErrorBody, PaginationMeta } from './index';

describe('ApiErrorBody', () => {
  it('has required error.code and error.message', () => {
    const sample: ApiErrorBody = { error: { code: 'X', message: 'Y' } };
    expectTypeOf(sample.error.code).toEqualTypeOf<string>();
    expectTypeOf(sample.error.message).toEqualTypeOf<string>();
  });
});

describe('PaginationMeta', () => {
  it('accepts valid pagination shape', () => {
    const sample: PaginationMeta = { page: 1, per_page: 20, total_items: 100, total_pages: 5 };
    expectTypeOf(sample.page).toEqualTypeOf<number>();
    expectTypeOf(sample.per_page).toEqualTypeOf<number>();
    expectTypeOf(sample.total_items).toEqualTypeOf<number>();
    expectTypeOf(sample.total_pages).toEqualTypeOf<number>();
  });
});
