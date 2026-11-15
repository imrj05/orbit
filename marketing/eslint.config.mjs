import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
    // Vendored Stageshow kit (the product-demo runtime). It measures layout
    // and drives animation from effects on purpose, which the React Compiler
    // rules flag; it is kept as upstream ships it rather than forked to lint.
    "src/components/stageshow/kit/**",
  ]),
]);

export default eslintConfig;
