const js = require('@eslint/js');
const globals = require('globals');
const tsParser = require('@typescript-eslint/parser');
const tsPlugin = require('@typescript-eslint/eslint-plugin');
const reactPlugin = require('eslint-plugin-react');
const reactHooksPlugin = require('eslint-plugin-react-hooks');

// Custom rule to detect window.location.href usage
const noWindowLocationHref = {
  meta: {
    type: 'problem',
    docs: {
      description: 'Disallow direct usage of window.location.href in Electron apps',
      recommended: true,
    },
  },
  create(context) {
    return {
      AssignmentExpression(node) {
        if (
          node.left.type === 'MemberExpression' &&
          node.left.object.type === 'MemberExpression' &&
          node.left.object.object.type === 'Identifier' &&
          node.left.object.object.name === 'window' &&
          node.left.object.property.name === 'location' &&
          node.left.property.name === 'href'
        ) {
          context.report({
            node,
            message: 'Do not use window.location.href directly in Electron apps. Use setView from the project instead'
          });
        }
      }
    };
  }
};

// Custom rule to detect strokeWidth passed to an icon from the app-icons registry.
// app-icons' light() wrapper pins stroke 1.5 *after* the prop spread, so the prop is
// silently discarded. Only identifiers imported from that module are flagged --
// inline <svg> and non-registry components use strokeWidth legitimately.
const APP_ICONS_MODULE = /(^|\/)app-icons(\.[jt]sx?)?$/;

const noStrokeWidthOnAppIcons = {
  meta: {
    type: 'problem',
    docs: {
      description: 'Disallow strokeWidth on icons imported from the app-icons registry',
      recommended: true,
    },
  },
  create(context) {
    const registryIcons = new Set();
    const registryNamespaces = new Set();

    return {
      Program(node) {
        for (const statement of node.body) {
          if (
            statement.type !== 'ImportDeclaration' ||
            typeof statement.source.value !== 'string' ||
            !APP_ICONS_MODULE.test(statement.source.value)
          ) {
            continue;
          }
          for (const specifier of statement.specifiers) {
            if (specifier.type === 'ImportNamespaceSpecifier') {
              registryNamespaces.add(specifier.local.name);
            } else {
              registryIcons.add(specifier.local.name);
            }
          }
        }
      },
      JSXOpeningElement(node) {
        const name = node.name;
        let isRegistryIcon = false;
        if (name.type === 'JSXIdentifier') {
          isRegistryIcon = registryIcons.has(name.name);
        } else if (
          name.type === 'JSXMemberExpression' &&
          name.object.type === 'JSXIdentifier'
        ) {
          isRegistryIcon = registryNamespaces.has(name.object.name);
        }
        if (!isRegistryIcon) {
          return;
        }
        for (const attribute of node.attributes) {
          if (attribute.type === 'JSXAttribute' && attribute.name.name === 'strokeWidth') {
            context.report({
              node: attribute,
              message: 'Do not pass strokeWidth to an app-icons icon. light() pins stroke 1.5 after the spread, so the prop is dead code -- remove it'
            });
          }
        }
      }
    };
  }
};

module.exports = [
  js.configs.recommended,
  {
    ignores: ['src/api/**'],
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        ecmaFeatures: {
          jsx: true,
        },
      },
      globals: {
        // This renderer/main code runs in the browser (Electron renderer) and in
        // Node (Electron main / build scripts). Declare both standard
        // environments from the canonical `globals` package instead of a
        // hand-maintained list — the old list silently omitted browser APIs
        // (PointerEvent, btoa/atob, structuredClone, BroadcastChannel, crypto,
        // TextDecoder, canvas/image types, …), which is why `no-undef` was red on
        // dozens of real, correct call sites. Mirrors the `env: { browser, node }`
        // already declared in the legacy `.eslintrc.json`.
        ...globals.browser,
        ...globals.node,
        ...globals.es2021,
        // App-/framework-specific identifiers not part of a standard env:
        Electron: 'readonly', // Electron type namespace (type positions; TS validates the rest)
        React: 'readonly',
        handleAction: 'readonly',
        // DOM `fetch` TypeScript-only *type* identifiers. These are erased at
        // runtime, so `globals.browser` (runtime values only) does not list them
        // and `no-undef` cannot see them as types — declare them explicitly so
        // type-position uses (e.g. `init?: RequestInit`) don't trip the rule.
        RequestInit: 'readonly',
        HeadersInit: 'readonly',
        RequestCredentials: 'readonly',
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      'react': reactPlugin,
      'react-hooks': reactHooksPlugin,
      'custom': {
        rules: {
          'no-window-location-href': noWindowLocationHref,
          'no-strokewidth-on-app-icons': noStrokeWidthOnAppIcons
        }
      }
    },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      ...reactPlugin.configs.recommended.rules,
      ...reactHooksPlugin.configs.recommended.rules,
      // Customize rules
      'react/react-in-jsx-scope': 'off',
      'react/prop-types': 'off', // We use TypeScript for prop validation
      'react/no-unknown-property': ['error', {
        ignore: ['dark:fill'] // Allow Tailwind dark mode syntax
      }],
      'react/no-unescaped-entities': 'off', // Allow quotes in JSX
      '@typescript-eslint/explicit-module-boundary-types': 'off',
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/no-unused-vars': ['warn', {
        argsIgnorePattern: '^_',
        varsIgnorePattern: '^_'
      }],
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
      '@typescript-eslint/no-var-requires': 'warn', // Downgrade to warning for Electron main process
      'no-undef': 'error',
      'no-useless-catch': 'warn',
      'custom/no-window-location-href': 'error',
      'custom/no-strokewidth-on-app-icons': 'error'
    },
    settings: {
      react: {
        version: 'detect',
      },
    },
  },
];
