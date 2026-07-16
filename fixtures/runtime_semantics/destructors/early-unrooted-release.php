<?php
// runtime-semantics: category=destructors expect=pass
class EarlyReleaseProbe {
    public function __destruct() {
        echo "destruct\n";
    }
}

function release_probe(): void {
    $probe = new EarlyReleaseProbe();
    echo "before\n";
    unset($probe);
    echo "after\n";
}

release_probe();
