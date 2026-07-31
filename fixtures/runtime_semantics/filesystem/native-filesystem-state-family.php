<?php
// runtime-semantics: category=filesystem expect=pass
// Exact request-owned cwd/umask state and the no-op stat-cache boundary.

function nativeFilesystemStateFamily(): array
{
    $original = getcwd();
    $entered = chdir(__DIR__);
    $inside = basename(getcwd());

    $initial = umask();
    $previous = umask(0077);
    $changed = umask();
    umask($initial);

    $cleared = clearstatcache();
    $restored = chdir($original);

    return [
        $entered,
        $inside,
        decoct($initial),
        decoct($previous),
        decoct($changed),
        $cleared,
        $restored,
        getcwd() === $original,
    ];
}

var_dump(nativeFilesystemStateFamily());
