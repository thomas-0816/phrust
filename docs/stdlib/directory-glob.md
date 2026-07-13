# Standard library Directory and Glob MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library provides directory resources and deterministic local directory/glob
helpers on top of the Standard library filesystem capability model.

## Implemented Functions

- `opendir`
- `readdir`
- `rewinddir`
- `closedir`
- `scandir`
- `glob`
- `getcwd`
- `chdir`

## Directory Resources

`opendir` registers a request-local directory resource in `ResourceTable`.
Directory cursors are independent from stream byte cursors and support
`readdir`, `rewinddir`, and `closedir`.

Directory entries are normalized for deterministic tests: `.` and `..` are
returned first, followed by lexically sorted host entries. `scandir` uses the
same normalized list and supports descending order through sorting order `1`.

## Capability Model

Directory and glob paths resolve relative to the builtin context current
working directory and must stay inside explicit allowed roots. Without an
allowed root, directory opens, scans, globbing, and `chdir` return `false`.

`getcwd` reads the request-local builtin context CWD. `chdir` mutates that CWD
inside the active builtin context. Full VM-wide persistence across separate
builtin dispatches is tracked as a known gap until the VM owns mutable
request-local CWD state.

## Glob MVP

`glob` supports local single-directory `*` and `?` filename patterns and returns
sorted absolute paths for matching entries. Recursive patterns, brace expansion,
character classes, flags, and platform-specific shell glob details are not part
of this MVP.
