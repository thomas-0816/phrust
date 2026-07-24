<?php

class NativeCatchOwnershipDestructor
{
    public function __destruct()
    {
        global $current;
        echo "drop:", get_debug_type($current), "\n";
    }
}

$current = new NativeCatchOwnershipDestructor();
try {
    throw new Exception("boom");
} catch (Exception $current) {
}

echo get_debug_type($current), ":", $current->getMessage(), ":";
echo $current instanceof Throwable ? "throwable\n" : "not-throwable\n";

$held = new NativeCatchOwnershipDestructor();
$alias = $held;
try {
    throw new Exception("held");
} catch (Exception $held) {
    echo "alias-catch\n";
}
echo "alias-held\n";
unset($alias);
echo "alias-released\n";
