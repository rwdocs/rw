/**
 * Read a response body as `T`.
 *
 * `Response.json()` resolves to `any`, which satisfies any declared return type
 * without TypeScript objecting: `return response.json()` from a function typed
 * `Promise<PageResponse>` type-checks while asserting a shape nothing verified.
 * Routing every read through here keeps that assertion in one place.
 *
 * The interfaces in `../types` mirror the Rust server's serialization by hand;
 * no codegen links the two. Runtime validation, if it is ever added, belongs
 * here rather than at each call site.
 */
export async function readJson<T>(response: Response): Promise<T> {
  return (await response.json()) as T;
}
