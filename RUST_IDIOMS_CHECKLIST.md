# Rust Idioms Review Checklist

A code-review checklist derived from the
[Rust Design Patterns — Idioms](https://rust-unofficial.github.io/patterns/idioms/)
chapter. Each idiom below has a set of checkboxes phrased as review questions:
tick one when the code satisfies it. Use it to audit a crate, a module, or a
single PR.

> Idioms are "commonly accepted community standards" — following them makes code
> readable and unsurprising to other Rust programmers. None are hard rules;
> each has trade-offs noted where relevant.

---

## 1. Use borrowed types for arguments

Prefer borrowed slices/trait objects over borrowed owned types, so callers with
either form can pass in.

- [ ] Functions take `&str` instead of `&String`.
- [ ] Functions take `&[T]` instead of `&Vec<T>`.
- [ ] Functions take `&T` instead of `&Box<T>` (or `&Rc<T>`/`&Arc<T>`).
- [ ] Functions take `&Path` instead of `&PathBuf`, `&OsStr` instead of `&OsString`.
- [ ] Generic bounds use `AsRef<str>` / `impl AsRef<Path>` where flexibility helps.
- [ ] No argument forces an allocation the callee doesn't need (e.g. taking
      `String` when it only reads it).

## 2. Concatenating strings with `format!`

- [ ] Building a string from mixed literals + values uses `format!` rather than a
      chain of `push_str`/`+` that hurts readability.
- [ ] Hot loops that append repeatedly still use `push_str`/`write!` on a reused
      buffer instead of allocating a new `format!` string each iteration.
- [ ] `write!`/`writeln!` into an existing `String`/`fmt::Formatter` is used when
      appending to a buffer, avoiding an intermediate allocation.

## 3. Constructors

- [ ] Types expose a `new()` (or clearly-named `with_*`/`from_*`) associated
      function rather than requiring struct-literal construction across module
      boundaries.
- [ ] Fallible construction returns `Result`/`Option` (e.g. `try_new`) instead of
      panicking.
- [ ] Multiple constructors are named by intent (`from_bytes`, `with_capacity`)
      rather than overloaded positional variants.
- [ ] `new()` without arguments is consistent with (or delegates to) `Default`.

## 4. The `Default` Trait

- [ ] Types with a sensible "empty/zero" value implement `Default`.
- [ ] `#[derive(Default)]` is used where all fields are themselves `Default`.
- [ ] A no-arg `new()` and `Default` don't diverge in behavior.
- [ ] Struct-update syntax (`Foo { a, ..Default::default() }`) is available for
      structs with many optional fields.
- [ ] `Default` is implemented for public types that appear in others' generic
      `Default` derives.

## 5. Collections are smart pointers (`Deref`)

- [ ] A wrapper newtype around a collection implements `Deref`/`DerefMut` to the
      inner type so it inherits its read methods — rather than re-exporting each
      method by hand.
- [ ] `Deref` is used only for genuine smart-pointer/wrapper relationships, not to
      fake inheritance between unrelated types.
- [ ] `Deref` target is an owned collection/inner value, and mutation invariants
      still hold when `DerefMut` is exposed.

## 6. Finalisation in destructors

- [ ] Cleanup that must always run (flush, unlock, temp-file removal, restore
      state) is placed in a `Drop` impl, not only on the happy path.
- [ ] Cleanup survives early returns, `?`, and panics via `Drop` rather than
      duplicated at each exit.
- [ ] `Drop` impls don't panic (or are documented/guarded if they might).
- [ ] Where explicit early finalization is needed, a consuming method exists
      alongside `Drop` (and avoids double-cleanup).

## 7. `mem::take` / `mem::replace` to keep owned values in changed enums

- [ ] Code that swaps an enum variant while reusing owned inner data uses
      `mem::replace`/`mem::take` instead of `clone()`-ing to satisfy the borrow
      checker.
- [ ] `mem::take` is used to move a field out behind `&mut self` (leaving
      `Default`) instead of `Option::take` gymnastics or clones.
- [ ] No `.clone()` exists solely to work around "cannot move out of borrowed
      content" where a `take`/`replace` would do.

## 8. On-Stack Dynamic Dispatch

- [ ] Where a value is one of a few concrete types chosen at runtime, a `&dyn`
      reference bound to a stack local is used instead of `Box<dyn>` heap
      allocation.
- [ ] `let x: &dyn Trait = if cond { &a } else { &b };`-style patterns are used to
      avoid boxing when the objects outlive the branch.
- [ ] Boxing (`Box<dyn Trait>`) is reserved for cases that genuinely need
      ownership/heap storage or escape the stack frame.

## 9. FFI Idioms

### Error handling in FFI

- [ ] Rust errors crossing the FFI boundary are converted to integer codes /
      out-params / nullable pointers — no `panic!` unwinds across `extern "C"`.
- [ ] `catch_unwind` wraps Rust callbacks invoked from C where a panic could
      otherwise cross the boundary.
- [ ] Error codes are documented and consistent (e.g. 0 = success).

### Accepting strings

- [ ] Incoming C strings are handled via `CStr::from_ptr` with an explicit null
      check, not assumed valid UTF-8 blindly.
- [ ] Borrowed C strings are consumed as `&CStr`/`&str` without taking ownership
      of memory Rust didn't allocate.

### Passing strings

- [ ] Strings handed to C are `CString`, with ownership/lifetime clearly defined
      (who frees, and a matching free function is exported when Rust allocates).
- [ ] Interior-null and lifetime issues are handled rather than passing a
      dangling `as_ptr()`.

> If the crate has no `unsafe`/`extern` FFI surface, mark this section N/A.

## 10. Iterating over an `Option`

- [ ] `Option` is chained into iterator pipelines via `.iter()`/`.into_iter()` or
      `.extend()`/`.chain()` instead of a `match`/`if let` that pushes one item.
- [ ] `opt.into_iter().flatten()`, `.filter_map`, `?`-in-iterator, or
      `Option::iter` is used where it reads more clearly than manual branching.

## 11. Pass variables to closure

- [ ] Closures capture only what they need — variables are `clone`d/`ref`d into a
      binding *before* the closure rather than moving a whole struct.
- [ ] `move` closures use a preceding `let x = data.clone();` (or a reference
      rebinding) to capture a single field instead of the entire environment.
- [ ] No accidental capture of `self` (or a large owner) where one field would do.

## 12. `#[non_exhaustive]` and private fields for extensibility

- [ ] Public enums/structs that may gain variants/fields later are marked
      `#[non_exhaustive]` so downstream match/constructors don't break on
      addition.
- [ ] Public structs that must stay constructible only via constructors keep at
      least one private field (or `#[non_exhaustive]`) to forbid external literal
      construction.
- [ ] Exhaustive matching is deliberately allowed (no `#[non_exhaustive]`) only
      where variant stability is guaranteed.

## 13. Easy doc initialization

- [ ] Doc examples that need a constructed value use a helper/`Default` so the
      example stays focused on the method under demonstration.
- [ ] Repeated setup across doc-tests is factored into a shared `# fn make() {}`
      hidden helper rather than copy-pasted boilerplate.

## 14. Temporary mutability

- [ ] A value that is built up mutably then used read-only is rebound to an
      immutable binding (`let data = { let mut d = ...; d.sort(); d };` or
      `let data = data;`) after construction.
- [ ] `let mut` scopes are as small as possible; nothing stays `mut` after it
      stops being mutated.

## 15. Return consumed argument on error

- [ ] APIs that take a value by ownership and can fail return the value back in
      the error variant (`Result<T, (E, Input)>` or an error carrying the input)
      so the caller can retry without reconstructing it.
- [ ] By-value fallible operations don't force the caller to `clone` defensively
      just in case they need the input again on failure.

---

## How to use this checklist

1. Pick one idiom (or a few) to focus a review — a whole-crate sweep of all 15 at
   once is rarely productive.
2. `grep`/`rg` for the mechanical signals (e.g. `&String`, `&Vec<`, `.clone()`,
   `Box<dyn`) to find candidate sites.
3. For each hit, tick the box or file it as a finding with `file:line`.
4. Remember the trade-offs — none of these are lints to apply blindly.
