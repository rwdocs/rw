# Diagram Style

Two copies of the same over-wide diagram: one wrapped in `<rw-diagram>` (as the
server emits it), one hand-authored bare. The wrapped one is styled only by the
sheet its shadow root adopts; the bare one only by `content.css`. Both must come
out sized and fonted the same.

The SVG is deliberately far wider than the article column so `max-width: 100%`
has something to do — at its intrinsic 1200px it would overflow the page.

<figure class="diagram" data-diagram-id="styled-wrapped"><rw-diagram><svg xmlns="http://www.w3.org/2000/svg" width="1200" height="200" viewBox="0 0 1200 200"><rect x="0" y="0" width="1200" height="200" fill="teal"/><text data-testid="wrapped-text" x="20" y="100" font-size="24">Wrapped label</text><a data-testid="wrapped-link" href="/getting-started"><text x="20" y="160" font-size="24">Wrapped link</text></a></svg></rw-diagram></figure>

<figure class="diagram" data-diagram-id="styled-bare"><svg xmlns="http://www.w3.org/2000/svg" width="1200" height="200" viewBox="0 0 1200 200"><rect x="0" y="0" width="1200" height="200" fill="olive"/><text data-testid="bare-text" x="20" y="100" font-size="24">Bare label</text></svg></figure>
