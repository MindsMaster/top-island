import pluginVue from 'eslint-plugin-vue';
import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript';
import prettierConfig from '@vue/eslint-config-prettier';

export default defineConfigWithVueTs(
  {
    // 只 lint 应用源码；其余目录要么是产物，要么不是 TS/Vue 工程
    ignores: [
      '**/node_modules/**',
      '**/target/**',
      'dist/**',
      'release/**',
      'release-feed/**',
      'build/**',
      'docs/**',
      '.archive/**',
      // 注入网易云 CEF 的独立脚本，不属于本 TS 工程
      'crates/island-cloudmusic-bridge/js/**',
      '*.config.*',
    ],
  },
  pluginVue.configs['flat/recommended'],
  vueTsConfigs.recommended,
  prettierConfig,
  {
    rules: {
      'vue/multi-word-component-names': 'off',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
    },
  }
);
