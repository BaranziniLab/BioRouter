/**
 * How the daemon will read a provider config key back.
 *
 * The provider settings form is a grid of `<input type="text">`, so every value
 * it produces is a JS string, and `/config/upsert` writes whatever JSON it is
 * handed straight into `config.yaml`. A string therefore lands quoted:
 *
 *     LLAMACPP_PORT: '11543'
 *
 * The backend reads these with a *typed* `Config::get_param::<T>()`, which
 * ends in `serde_yaml::from_value::<T>(value)` (`crates/biorouter/src/config/base.rs`).
 * serde_yaml will not coerce a YAML string into a `usize`/`u16`/`bool`, so the
 * read returns `Err` and every call site swallows it with `.ok()` /
 * `.unwrap_or(DEFAULT)`. The setting saves, reloads into the form, and does
 * nothing. Writing a JSON number/boolean instead produces an unquoted YAML
 * scalar, which deserializes.
 *
 * Note the env-var path in the same Rust file does not have this problem: it
 * runs `parse_env_value`, which re-parses the string into a typed JSON value
 * before deserializing. The config-file path has no equivalent step, so the
 * coercion has to happen here, at the write.
 */
export type ConfigKeyValueType = 'number' | 'boolean' | 'string';

/**
 * The subset of `ConfigKey` this module needs. Declared structurally so it also
 * accepts the looser inline shape `providerConfigSubmitHandler` works with.
 */
export interface ConfigKeyDeclaration {
  default?: unknown;
  secret?: boolean;
}

/** A JSON-compatible number literal. Deliberately rejects '' (`Number('')` is 0). */
const NUMBER_LITERAL = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;
const BOOLEAN_LITERAL = /^(true|false)$/i;

/**
 * Infer a key's value type from its *declared default*.
 *
 * `ConfigKey` carries no type field — the Rust struct
 * (`crates/biorouter/src/providers/base.rs`) is `{ name, required, secret,
 * default, oauth_flow }` and the generated TS mirrors it. The declared default
 * is the only type-bearing metadata that crosses the wire, and across the whole
 * provider catalog it is a faithful signal: every key whose default parses as a
 * number is read back with a numeric `get_param` (the four `*_TIMEOUT`s,
 * `LLAMACPP_PORT`, `LLAMACPP_CONTEXT_SIZE`, the four `GCP_*` retry knobs), the
 * one key defaulting to `false` is `LLAMACPP_ENABLE_THINKING`, and every other
 * default is an unambiguous string (hosts, base paths, regions, API versions,
 * `""`, `"default"`).
 *
 * Inference is off the *declaration*, never off what the user typed: a string
 * key must stay a string even when its value happens to look like a number.
 *
 * Consequences worth knowing:
 *  - a key with no declared default is treated as a string, so a future numeric
 *    key that ships without a default would regress to this same bug. Give
 *    numeric keys a default.
 *  - secrets are never coerced. They go to the OS credential store as opaque
 *    strings, and an all-digit API key must not become a number.
 */
export function inferConfigKeyValueType(parameter: ConfigKeyDeclaration): ConfigKeyValueType {
  if (parameter.secret === true) {
    return 'string';
  }

  const declared = parameter.default;
  if (typeof declared !== 'string') {
    return 'string';
  }

  const trimmed = declared.trim();
  if (BOOLEAN_LITERAL.test(trimmed)) {
    return 'boolean';
  }
  if (NUMBER_LITERAL.test(trimmed)) {
    return 'number';
  }
  return 'string';
}

/**
 * Convert one form value into the JSON type the daemon will be able to read
 * back for this key.
 *
 * Conservative by construction — the value is returned untouched unless the key
 * declares a numeric or boolean default *and* the value actually parses as one:
 *  - an empty value keeps its existing "unset" meaning; '' never becomes 0.
 *  - unparseable text is written as-is rather than as `NaN` (which JSON-encodes
 *    to `null`), so a typo still lands on today's fall-back-to-default path.
 *  - a value that is already a number/boolean passes straight through.
 */
export function coerceConfigKeyValue(parameter: ConfigKeyDeclaration, value: unknown): unknown {
  if (typeof value !== 'string') {
    return value;
  }

  const type = inferConfigKeyValueType(parameter);
  if (type === 'string') {
    return value;
  }

  const trimmed = value.trim();
  if (trimmed === '') {
    return value;
  }

  if (type === 'boolean') {
    if (!BOOLEAN_LITERAL.test(trimmed)) {
      return value;
    }
    return trimmed.toLowerCase() === 'true';
  }

  if (!NUMBER_LITERAL.test(trimmed)) {
    return value;
  }
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) ? parsed : value;
}
