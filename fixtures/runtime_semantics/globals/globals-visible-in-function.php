<?php
$seed = "before";

function update_globals_from_function(): void {
    echo $GLOBALS["seed"], "\n";
    $GLOBALS["created"] = "inside";
}

update_globals_from_function();
echo $created, "\n";
