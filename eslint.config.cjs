const js = require('@eslint/js');
const globals = require('globals');
const prettier = require('eslint-config-prettier');

module.exports = [
  {
    ignores: [
      '.tools/**',
      'target/**',
      'dist/**',
      'bin/**',
      'gen/**',
      'node_modules/**',
      'build/**',
      'desktop/assets/**',
    ],
  },
  js.configs.recommended,
  {
    files: ['**/*.cjs'],
    languageOptions: { sourceType: 'commonjs', globals: globals.node },
  },
  {
    files: ['desktop/**/*.js'],
    languageOptions: {
      sourceType: 'script',
      globals: globals.browser,
    },
  },
  {
    files: ['desktop/grouping.js'],
    languageOptions: { globals: { module: 'readonly' } },
  },
  {
    rules: {
      'no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrors: 'none' },
      ],
      'no-empty': ['error', { allowEmptyCatch: true }],
    },
  },
  prettier,
];
