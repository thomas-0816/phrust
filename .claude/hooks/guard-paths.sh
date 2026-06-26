#!/usr/bin/env bash
# PreToolUse guard: block hand-edits to read-only or generated phrust paths.
#
# Wired in .claude/settings.json for Edit|Write|NotebookEdit. Receives the tool
# call as JSON on stdin; exit 2 blocks the call and surfaces stderr to Claude.
set -euo pipefail

input="$(cat)"
file="$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')"
[[ -z "$file" ]] && exit 0

block() {
  printf 'phrust guard: %s\n' "$1" >&2
  exit 2
}

case "$file" in
  */third_party/php-src/*)
    block "third_party/php-src is a read-only reference (pinned php-src source + original PHPTs). Do not edit upstream files. Add a minimized fixture under tests/phpt/generated/<module>/ instead (see the new-phpt-fixture skill)." ;;
  */tests/phpt/manifests/full-* \
  | */tests/phpt/manifests/known-gap-catalog.jsonl \
  | */tests/phpt/manifests/phpt-corpus.jsonl \
  | */tests/phpt/manifests/module-priority.json \
  | */tests/phpt/manifests/modules/*)
    block "$file is committed PHPT baseline source-of-truth. Regenerate it via 'just phpt-triage' or 'PHPT_RUN_FULL=1 just phpt-full-regression' (with an explicit, justified PHPT_ACCEPT_BASELINE=1 only when accepting new fingerprints) — do not hand-edit." ;;
  */docs/phpt/modules/* \
  | */docs/phpt/reports/*)
    block "$file is a rendered PHPT summary, not the source of truth. Regenerate via 'just phpt-triage' / 'just phpt-full-regression' instead of hand-editing counts." ;;
esac

exit 0
