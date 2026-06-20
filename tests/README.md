# Tests

Phase 0 defines the future test layout but does not add real PHP fixtures.

Later phases may add:

- Lexer differential fixtures compared against `token_get_all()`.
- Parser accept/reject fixtures compared against `php -l`.
- Runtime fixtures compared against the reference CLI.
- Imported or adapted `.phpt` tests from the pinned PHP `8.5.7` reference.

Expected results should be generated from the pinned reference PHP, not written
from memory.
