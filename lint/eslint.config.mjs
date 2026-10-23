// Fallback ESLint config for the diff gate. Used only for repos that have no ESLint config of
// their own, or whose ESLint is not installed on this host (for example, it runs in Docker).
import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

export default [
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // ESLint matches only *.js, *.mjs and *.cjs unless a config names more file types.
    files: ["**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}"],
    languageOptions: {
      // The TypeScript parser also reads plain JS/JSX, and its scope analysis counts JSX
      // usage, so components used only in JSX are not reported as unused.
      parser: tseslint.parser,
      parserOptions: { ecmaFeatures: { jsx: true } },
      globals: { ...globals.browser, ...globals.node },
    },
  },
  {
    // TypeScript checks undefined names itself; this rule only gives false reports there.
    files: ["**/*.{ts,tsx,mts,cts}"],
    rules: { "no-undef": "off" },
  },
];
