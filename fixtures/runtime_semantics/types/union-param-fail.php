<?php
// runtime-semantics: category=types expect=pass
function label(int|string $value): string {
    return "ok";
}

echo label([]), "\n";
