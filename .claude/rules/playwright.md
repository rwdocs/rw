---
description: Playwright E2E test conventions
globs: "**/*.spec.ts"
---

# Playwright E2E Test Conventions

## Selector Priority Order

Use selectors in this order, from most to least preferred:

1. **Role locators** — `page.getByRole('button', { name: 'Submit' })`, `page.getByRole('navigation', { name: 'Breadcrumb' })`. This includes ARIA roles and names.
2. **Text/label locators** — `page.getByText()`, `page.getByLabel()`, `page.getByPlaceholder()`
3. **Test IDs** — `page.getByTestId('viewer-root')` for elements without semantic roles
4. **CSS/XPath** — last resort, only for structural queries where no semantic alternative exists (e.g., `pre code[class*="language-python"]`)

## Rules

- Never use CSS class names as selectors (e.g., `.layout-sidebar`, `.flex-1`). Classes are styling concerns and break when refactoring.
- Prefer adding ARIA landmarks (`role`, `aria-label`) to components over adding `data-testid`. This improves both tests and accessibility.
- Use `data-testid` only for elements that have no meaningful role or text (e.g., layout wrappers, scroll containers).
- Prefer `locator.evaluate()` over `page.evaluate()` with `querySelector()`. When `page.evaluate()` is unavoidable (e.g., querying multiple elements), use semantic attributes (`aria-label`, `data-testid`) — never CSS classes.
