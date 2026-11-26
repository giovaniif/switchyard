# Mermaid diagrams

Completed Mermaid fences in chat automatically show a diagram. Source remains
available, ordinary code stays code, and incomplete or invalid diagrams keep
their source readable. Diagrams stay inside the thread on phones.

## Setup

```sh
yv iso up
yv iso seed
```

## Cleanup

```sh
yv iso down
```

## Recipes

### automatic-diagrams
Viewports: both
Two diagram types render automatically, the source toggle works, and regular
code and surrounding prose survive.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-diagrams\n\nBefore the diagrams.\n\u0060\u0060\u0060mermaid\nflowchart LR\nA[Start] --> B[Finish]\n\u0060\u0060\u0060\nBetween diagrams.\n~~~mermaid\nsequenceDiagram\nAlice->>Bob: Hello\n~~~\n\u0060\u0060\u0060js\nconst ordinary = true;\n\u0060\u0060\u0060\nAfter the diagrams."}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"css:.mermaid img","count":2},{"eval":"Promise.all([...document.querySelectorAll(\".mermaid img\")].map(i => i.decode())).then(() => true)","equals":true},{"expect":{"css":".prose p","hasText":"Before the diagrams."}},{"expect":{"css":".prose p","hasText":"After the diagrams."}},{"expect":"css:pre.code","text":"const ordinary = true;"},{"shot":"automatic"},{"click":"role:button:Show source"},{"expect":"css:.mermaid pre","text":"A[Start] --> B[Finish]"},{"shot":"source"},{"click":"role:button:Show diagram"},{"expect":"css:.mermaid img","count":2},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"reload":true},{"expect":"css:.mermaid img","count":2}]'
```

### diagram-fallbacks
Viewports: both
Malformed, empty and still-open fences show source and a useful status without
breaking the answer.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_claude}}","text":"TEST-fallbacks\n\n\u0060\u0060\u0060mermaid\nflowchart LR\nA --> )\n\u0060\u0060\u0060\n\u0060\u0060\u0060mermaid\n\u0060\u0060\u0060\n\u0060\u0060\u0060mermaid\nflowchart LR\nA -->"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_claude}}' --steps '[{"expect":{"css":".mermaid","hasText":"Could not render diagram. Source is shown below."}},{"expect":"css:.mermaid pre","text":"A --> )"},{"expect":{"css":".mermaid","hasText":"Empty Mermaid diagram."}},{"expect":{"css":".mermaid","hasText":"Waiting for the diagram to finish…"}},{"expect":"css:.mermaid img","count":0},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"fallbacks"}]'
```

### safe-long-diagram-and-loading
Viewports: both
A new diagram shows its loading status, long labels fit the thread, and
embedded HTML and configuration cannot turn the preview into active content.

```sh
yv drive 'session:{{sid_think}}' --steps '[{"eval":"document.querySelectorAll(\".mermaid\").forEach(e => e.setAttribute(\"data-existing\", \"\")); window.__diagramStatuses = []; window.__diagramObserver = new MutationObserver(() => { document.querySelectorAll(\".mermaid [role=status]\").forEach(e => window.__diagramStatuses.push(e.textContent)); }); window.__diagramObserver.observe(document.querySelector(\".thread\"), {subtree:true, childList:true}); true","equals":true},{"fill":"label:Message","value":"TEST-safe-long-diagram\n\n\u0060\u0060\u0060mermaid\n%%{init: {\"securityLevel\":\"loose\",\"htmlLabels\":true,\"themeCSS\":\"body { display:none }\"}}%%\nflowchart TD\nA[\"Review the complete request and keep the diagram readable inside a narrow conversation\"] --> B[\"<img src=https://example.invalid/diagram onerror=alert(1)>\"]\n\u0060\u0060\u0060"},{"click":"role:button:Send"},{"expect":"css:.mermaid:not([data-existing]) img","count":1},{"eval":"(async () => { const i = document.querySelector(\".mermaid:not([data-existing]) img\"); await i.decode(); const svg = await fetch(i.src).then(r => r.text()); const d = new DOMParser().parseFromString(svg, \"image/svg+xml\"); return i.getBoundingClientRect().width <= Number(d.documentElement.getAttribute(\"viewBox\").split(/\\s+/)[2]) + 1 && !d.querySelector(\"script, foreignObject, [onclick], [onerror]\") && d.documentElement.textContent.replace(/\\s/g, \"\").includes(\"Reviewthecompleterequest\") && !svg.includes(\"body { display:none }\"); })()","equals":true},{"eval":"window.__diagramObserver.disconnect(); window.__diagramStatuses.includes(\"Drawing diagram…\")","equals":true},{"eval":"document.querySelectorAll(\".prose svg, .prose iframe, .prose script\").length","equals":0},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"safe-long-diagram"}]'
```

### opens-at-latest-reply
Viewports: both
Opening a session whose latest answer holds a diagram lands at the bottom of
the thread once the diagram has drawn. The image is inserted before it has a
height and grows a moment later; the browser's scroll adjustment for that
growth must not count as the user scrolling up.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-latest-diagram\n\nLine 1 of the notes above the diagram.\n\nLine 2 of the notes above the diagram.\n\nLine 3 of the notes above the diagram.\n\nLine 4 of the notes above the diagram.\n\nLine 5 of the notes above the diagram.\n\nLine 6 of the notes above the diagram.\n\nLine 7 of the notes above the diagram.\n\nLine 8 of the notes above the diagram.\n\nLine 9 of the notes above the diagram.\n\nLine 10 of the notes above the diagram.\n\nLine 11 of the notes above the diagram.\n\nLine 12 of the notes above the diagram.\n\nLine 13 of the notes above the diagram.\n\nLine 14 of the notes above the diagram.\n\nLine 15 of the notes above the diagram.\n\nLine 16 of the notes above the diagram.\n\nLine 17 of the notes above the diagram.\n\nLine 18 of the notes above the diagram.\n\nLine 19 of the notes above the diagram.\n\nLine 20 of the notes above the diagram.\n\nLine 21 of the notes above the diagram.\n\nLine 22 of the notes above the diagram.\n\nLine 23 of the notes above the diagram.\n\nLine 24 of the notes above the diagram.\n\nLine 25 of the notes above the diagram.\n\nLine 26 of the notes above the diagram.\n\nLine 27 of the notes above the diagram.\n\nLine 28 of the notes above the diagram.\n\nLine 29 of the notes above the diagram.\n\nLine 30 of the notes above the diagram.\n\n\u0060\u0060\u0060mermaid\nflowchart TD\nA[Read the configuration file] --> B{Is the cache warm?}\nB -->|Yes| C[Serve from memory]\nB -->|No| D[Rebuild the index]\n\u0060\u0060\u0060\nTEST-latest-line\n\n- one\n- two\n- three"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":{"css":".prose p","hasText":"TEST-latest-line"}},{"expect":"css:.mermaid img"},{"eval":"Promise.all([...document.querySelectorAll(\".mermaid img\")].map(i => i.decode())).then(() => new Promise(r => setTimeout(r, 500))).then(() => true)","equals":true},{"eval":"(() => { const el = document.querySelector(\".scroll\"); return el.scrollHeight - el.scrollTop - el.clientHeight < 2; })()","equals":true},{"shot":"latest-reply"}]'
```
