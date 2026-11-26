#!/usr/bin/env bash
# Releases Switchyard X.Y.Z in one step. Write the version's CHANGELOG.md
# section first (it becomes the release notes), then:
#   scripts/release.sh 0.21.0            # open the release PR, auto-merge on
#   scripts/release.sh 0.21.0 --dry-run  # make the commit, show it, push nothing
# It builds the release commit from origin/main in a temporary worktree, so
# this checkout is never switched: the changelog section from here, and every
# version (yardd, the app, tauri.conf.json, Cargo.lock) set to X.Y.Z. main is
# protected, so the commit goes up as a PR that merges itself (squash) once the
# required checks pass. That merge starts .github/workflows/release.yml, which
# tests the commit, builds, signs and publishes the release and tags it vX.Y.Z.
set -euo pipefail
cd "$(dirname "$0")/.."
# Cargo is often only in a login shell's PATH (rustup on Homebrew, ~/.cargo).
PATH="$PATH:$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin"

usage() { echo "usage: scripts/release.sh X.Y.Z [--dry-run]" >&2; exit 1; }
fail() { echo "$*" >&2; exit 1; }

version="${1:-}"
mode="${2:-}"
[ "$#" -le 2 ] || usage
[ -z "$mode" ] || [ "$mode" = --dry-run ] || usage
printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || usage

# The section, heading included, up to the next version's heading.
section() { awk -v v="$version" '$0 ~ "^## " v "( |$)" { on = 1; print; next } /^## / { on = 0 } on' "$1"; }
notes="$(section CHANGELOG.md)"
[ -n "$notes" ] || fail "CHANGELOG.md has no '## $version' section yet"
[ -n "$(printf '%s\n' "$notes" | sed 1d | tr -d '[:space:]')" ] || fail "CHANGELOG.md's '## $version' section is empty"

repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner)" || fail "gh cannot see this repository; run gh auth login"
git fetch --quiet origin main
git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null && fail "v$version already exists"
gh release view "v$version" -R "$repo" >/dev/null 2>&1 && fail "Switchyard $version is already released"
branch="release/$version"
git ls-remote --exit-code --heads origin "$branch" >/dev/null && fail "$branch already exists on origin; its PR may be open already"

current="$(git show origin/main:crates/yardd/Cargo.toml | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)"
[ "$current" != "$version" ] || fail "main is already at $version; release.yml publishes it (rerun it from Actions if it failed)"
[ "$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -1)" = "$version" ] || fail "$version is older than main's $current"

work="$(mktemp -d)"
cleanup() { git worktree remove --force "$work/tree" 2>/dev/null || true; rm -rf "$work"; }
trap cleanup EXIT
git worktree add --quiet --detach "$work/tree" origin/main
printf '%s\n' "$notes" > "$work/section.md"

(
  cd "$work/tree"
  # main's changelog with this version's section from here, newest first:
  # it goes before the first version heading, replacing any older draft of it.
  awk -v v="$version" -v f="$work/section.md" '
    $0 ~ "^## " v "( |$)" { skip = 1; next }
    /^## / { skip = 0; if (!done) { while ((getline line < f) > 0) print line; print ""; done = 1 } }
    !skip { print }
    END { if (!done) { print ""; while ((getline line < f) > 0) print line } }
  ' CHANGELOG.md > "$work/CHANGELOG.md"
  mv "$work/CHANGELOG.md" CHANGELOG.md

  # The first "version" line in each file is the package's own.
  set_version() { # file, prefix before the quoted version
    awk -v p="$2" -v v="$version" '!done && index($0, p "\"") == 1 { print p "\"" v "\"" (index($0, "\",") ? "," : ""); done = 1; next } { print }' "$1" > "$1.tmp"
    mv "$1.tmp" "$1"
  }
  set_version crates/yardd/Cargo.toml 'version = '
  set_version app/src-tauri/Cargo.toml 'version = '
  set_version app/src-tauri/tauri.conf.json '  "version": '
  cargo metadata --format-version 1 >/dev/null # refreshes Cargo.lock

  # release.yml checks the same, so a mistake here stops before the PR.
  lock() { awk -v n="$1" '$0 == "name = \"" n "\"" { getline; sub(/^version = "/, ""); sub(/"$/, ""); print; exit }' Cargo.lock; }
  [ "$(lock yardd)" = "$version" ] && [ "$(lock switchyard)" = "$version" ] || fail "Cargo.lock did not pick up $version"
  [ "$(sed -n 's/^  "version": "\(.*\)",$/\1/p' app/src-tauri/tauri.conf.json)" = "$version" ] || fail "tauri.conf.json did not pick up $version"

  git switch --quiet -c "$branch"
  git add CHANGELOG.md crates/yardd/Cargo.toml app/src-tauri/Cargo.toml app/src-tauri/tauri.conf.json Cargo.lock
  git commit --quiet -m "release: $version"
  git show --stat --format='%h %s' HEAD
)

if [ "$mode" = --dry-run ]; then
  git -C "$work/tree" diff HEAD~1 HEAD -- CHANGELOG.md crates/yardd/Cargo.toml app/src-tauri/Cargo.toml app/src-tauri/tauri.conf.json Cargo.lock
  git -C "$work/tree" switch --quiet --detach
  git branch -D "$branch" >/dev/null
  echo "Dry run: nothing pushed."
  exit 0
fi

git -C "$work/tree" push --quiet origin "$branch"
git -C "$work/tree" switch --quiet --detach
git branch -D "$branch" >/dev/null
body="$(printf '%s\n\n%s\n' "$notes" "Merging this publishes Switchyard $version: .github/workflows/release.yml runs on the merge, tests it, builds and signs the app and the Linux packages, and makes the release and the tag v$version. Auto-merge (squash) is on.")"
url="$(gh pr create -R "$repo" --base main --head "$branch" --title "release: $version" --body "$body")"
gh pr merge -R "$repo" --auto --squash "$url"
echo "Opened $url; it merges by itself once the required checks pass."
echo "Then follow the release at https://github.com/$repo/actions/workflows/release.yml"
if [ -n "$(git status --porcelain -- CHANGELOG.md)" ]; then
  echo "Your CHANGELOG.md edit is in the PR; drop the local copy (git restore CHANGELOG.md) before you pull."
fi
