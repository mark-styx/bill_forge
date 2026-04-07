/** @type {import('jest').Config} */
module.exports = {
  testMatch: ['<rootDir>/__tests__/**/*.{test,spec}.{ts,tsx}'],
  transform: {
    '^.+\\.tsx?$': 'ts-jest',
    '^.+\\.jsx?$': 'babel-jest',
  },
  // pnpm stores packages under node_modules/.pnpm/pkg@ver/node_modules/pkg/
  // The optional group skips the .pnpm/.../node_modules/ prefix so the
  // negative lookahead still sees the real package name.
  transformIgnorePatterns: [
    'node_modules/(?!(?:\\.pnpm/[^/]+/node_modules/)?(?:expo|expo-router|@tanstack|zustand|react-native|@react-navigation|@testing-library|react-test-renderer)(/|$))',
  ],
  globals: {
    __DEV__: true,
  },
  moduleFileExtensions: ['ts', 'tsx', 'js', 'jsx', 'json'],
};
