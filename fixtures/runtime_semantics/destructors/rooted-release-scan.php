<?php
// runtime-semantics: category=destructors expect=pass
class RootedReleaseProbe {
    public int $value = 1;

    public function __destruct() {
        global $script_finished;
        if (!$script_finished) {
            echo "premature destruct\n";
        }
    }
}

$request_root = null;
$script_finished = false;

function release_while_rooted(): void {
    global $request_root;
    $probe = new RootedReleaseProbe();
    $request_root = $probe;
    unset($probe);
    echo "rooted\n";
}

release_while_rooted();
echo "after\n";
$script_finished = true;
