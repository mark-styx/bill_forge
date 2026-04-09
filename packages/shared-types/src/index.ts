/**
 * @billforge/shared-types — cross-app TypeScript contract shared between
 * apps/web and apps/mobile. Migrate duplicated types here one-at-a-time
 * per follow-up to issue #126. First migration: ApiErrorBody.
 */
export interface ApiErrorBody {
  error: {
    code: string;
    message: string;
    details?: unknown;
    field_errors?: Record<string, string[]>;
  };
}
