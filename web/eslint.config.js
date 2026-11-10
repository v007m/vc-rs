import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
  {
    ignores: [
      'build/',
      '.svelte-kit/',
      'src/lib/gen/',
      'report/',
      'test-results/',
      'playwright-report/',
    ],
  },
  js.configs.recommended,
  ...ts.configs.strict,
  ...svelte.configs.recommended,
  ...svelte.configs.prettier,
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        extraFileExtensions: ['.svelte'],
        parser: ts.parser,
      },
    },
  },
  {
    // vc ui's CSP blocks style attributes; `style:` directives and classes set styles it allows.
    files: ['**/*.svelte'],
    rules: {
      'no-restricted-syntax': [
        'error',
        {
          selector: "SvelteAttribute[key.name='style']",
          message: 'Use a class or a style: directive; the CSP blocks style attributes.',
        },
      ],
    },
  },
);
