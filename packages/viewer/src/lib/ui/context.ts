import { getContext, setContext } from "svelte";

/**
 * Typed wrapper around Svelte's context API.
 *
 * Svelte's `getContext`/`setContext` take an opaque key (any value) and return
 * `unknown`. Callers then cast at every use site, which drifts as the value's
 * shape evolves. `createContextKey` binds a type to a fresh Symbol key once,
 * so every producer/consumer shares the same type without casting.
 *
 *   const themeCtx = createContextKey<Theme>("theme");
 *   themeCtx.set({ mode: "dark" });           // in parent
 *   const theme = themeCtx.get();             // in child, typed as Theme
 *
 * Keys are Symbols so two modules that happen to pick the same context name
 * stay isolated.
 */
export function createContextKey<T>(name: string): {
  readonly set: (value: T) => T;
  readonly get: () => T | undefined;
} {
  const key = Symbol(name);
  return {
    set: (value: T) => setContext(key, value),
    get: () => getContext<T | undefined>(key),
  };
}
