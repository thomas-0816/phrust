# reflection.extensions PHPT coverage

## Verified scope

- `ReflectionExtension` canonical names.
- Registered extension functions, classes, class names, and extension-owner
  metadata selected by the manifest.
- App extension arginfo-backed owners for PDO, CurlHandle, ZipArchive, finfo,
  OpenSSL key/cert objects, DOM, and intl classes.

## Known gaps

- Full extension dependency, INI, constants, version, and author metadata parity
  is not claimed unless explicitly selected elsewhere.
