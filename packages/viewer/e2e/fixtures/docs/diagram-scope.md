# Diagram Popup Scope

A wrapped diagram whose SVG carries an id-scoped `<style>` rule and a
`url(#…)` clip reference. The zoom popup clones it into the modal's own shadow
root without rewriting ids, so both must still resolve inside that root.

Deliberately a separate fixture from `diagram-live.md`: that one is rewritten in
place by the live-reload spec, and the suite runs fully parallel, so a shared
fixture would race.

<figure class="diagram" data-diagram-id="scope"><rw-diagram><svg xmlns="http://www.w3.org/2000/svg" id="scope-diagram" width="200" height="100" viewBox="0 0 200 100"><style>#scope-diagram .styled{fill:#eef;}</style><defs><clipPath id="scope-clip"><rect x="100" y="0" width="50" height="100"/></clipPath></defs><rect data-testid="scope-rect" x="0" y="0" width="100" height="100" fill="teal"/><rect class="styled" data-testid="scope-styled" x="100" y="0" width="100" height="100" clip-path="url(#scope-clip)"/></svg></rw-diagram></figure>
