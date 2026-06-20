<?php
$a = $b = 1;
$a += 2;
$b ??= $fallback;
echo $a ? $b : $fallback;
echo $a ?: $fallback;
echo $maybe ?? "default";
