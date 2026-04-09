import { describe, it, expectTypeOf } from 'vitest';
import type { ApiErrorBody } from './index';

describe('ApiErrorBody', () => {
  it('has required error.code and error.message', () => {
    const sample: ApiErrorBody = { error: { code: 'X', message: 'Y' } };
    expectTypeOf(sample.error.code).toEqualTypeOf<string>();
    expectTypeOf(sample.error.message).toEqualTypeOf<string>();
  });
});
