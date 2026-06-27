<?php
for ($i = 0; $i < 10; $i++) {
    echo "item-", $i, "=", true, "\n";
}

$prefix = "concat";
$suffix = "-echo";
$joined = $prefix . $suffix;
echo $joined, "\n";

echo "scalar:", 0, ":", 42, ":", true, ":", false, ":", null, ":end\n";

ob_start();
echo "outer-";
ob_start();
echo "inner";
ob_end_flush();
echo "-tail";
ob_end_flush();
echo "\n";
