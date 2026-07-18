# Diagram Collision

Two diagrams whose internal ids collide. Each `url(#clip1)` must resolve inside
its own diagram — document-wide resolution would paint both rects red.

The first diagram carries a `<text>` label for the comment-exclusion spec, which
needs a text node inside a shadow root to select. It sits in the bottom-left
corner deliberately: the isolation spec hit-tests each rect's center, and a
label over that point would be hit instead.

<figure class="diagram" data-diagram-id="first"><rw-diagram><svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 200 100"><defs><clipPath id="clip1"><rect x="0" y="0" width="200" height="100"/></clipPath></defs><rect data-testid="first-rect" x="0" y="0" width="200" height="100" fill="red" clip-path="url(#clip1)"/><text x="4" y="96" font-size="8">Collision label</text></svg></rw-diagram></figure>

<figure class="diagram" data-diagram-id="second"><rw-diagram><svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 200 100"><defs><clipPath id="clip1"><rect x="0" y="0" width="0" height="0"/></clipPath></defs><rect data-testid="second-rect" x="0" y="0" width="200" height="100" fill="green" clip-path="url(#clip1)"/></svg></rw-diagram></figure>
