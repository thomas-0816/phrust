<?php
require "lib/page.php";

$limit = 6;
if (isset($_GET["n"])) {
    $limit = (int) $_GET["n"];
}
if ($limit < 1) {
    $limit = 1;
}
if ($limit > 20) {
    $limit = 20;
}

$numbers = [];
for ($i = 1; $i <= $limit; $i = $i + 1) {
    $numbers[] = $i;
}

$sum = 0;
foreach ($numbers as $number) {
    $sum = $sum + $number;
}

demo_title("Loops and arrays");

echo "<p>Rendering numbers from 1 to ", demo_value($limit), " with a PHP <code>for</code> loop and summing them with <code>foreach</code>.</p>\n";
echo "<ol>\n";
foreach ($numbers as $number) {
    echo "<li>", demo_value($number), "</li>\n";
}
echo "</ol>\n";
echo "<p>Sum: <strong>", demo_value($sum), "</strong></p>\n";

demo_footer();
