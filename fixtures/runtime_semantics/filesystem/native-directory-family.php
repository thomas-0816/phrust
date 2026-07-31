<?php
// runtime-semantics: category=filesystem expect=pass
// Exact directory-resource traversal and direct scandir array publication.

function nativeDirectoryFamily(): array
{
    $directory = opendir(__DIR__);
    $first = [readdir($directory), readdir($directory)];
    sort($first);
    $rewound = rewinddir($directory);
    $again = [readdir($directory), readdir($directory)];
    sort($again);
    $closed = closedir($directory);

    return [
        $first,
        $rewound,
        $again,
        $closed,
        array_slice(scandir(__DIR__), 0, 4),
        array_slice(scandir(__DIR__, 1), 0, 4),
    ];
}

var_dump(nativeDirectoryFamily());
