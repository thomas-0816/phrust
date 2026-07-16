<?php
// runtime-semantics: category=destructors expect=pass
class RootedReleaseProbe {
    public int $value = 1;
}

$request_root = null;

function release_while_rooted(): void {
    global $request_root;
    $probe = new RootedReleaseProbe();
    $request_root = $probe;
    unset($probe);
    echo "rooted\n";
}

release_while_rooted();
echo "after\n";
