# Pull request links

The conversation header links to the GitHub pull request for its branch when
one exists. Workspace has no pull request view. PR links open in another tab
or, in the desktop app, in the default browser.

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

### pull-request-opens-externally
Viewports: both

```sh
yv drive 'session:{{sid_edit}}' --steps '[{"expect":"role:link:Pull request #4242"},{"eval":"(() => {const a=document.querySelector(\".thread .bar .session-pr-link\"); return [a.href,a.target,a.rel];})()","equals":["https://github.com/test-owner/test-demo-app/pull/4242","_blank","noopener noreferrer"]},{"shot":"external-pr"}]'
```

### no-pull-request-link-on-unmatched-branch
Viewports: desktop

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"expect":"css:.term-pane"},{"wait":350},{"expect":"css:.session-pr-link","count":0}]'
```
