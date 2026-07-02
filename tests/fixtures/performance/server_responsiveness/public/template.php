<?php
$items = [];
for ($i = 0; $i < 40; $i++) {
    $items[] = "<span>" . htmlspecialchars((string) $i, ENT_QUOTES) . "</span>";
}
echo implode(",", $items), "\n";
