<?php
require "lib/page.php";

function number_param($name, $fallback)
{
    if (isset($_GET[$name])) {
        return (int) $_GET[$name];
    }
    return $fallback;
}

function operation_row($label, $value)
{
    echo "<tr><th>", demo_value($label), "</th><td>", demo_value($value), "</td></tr>\n";
}

$a = number_param("a", 8);
$b = number_param("b", 5);

demo_title("GET calculator");

echo "<p>Change the query string to recalculate, for example <code>?a=21&amp;b=7</code>.</p>\n";
echo "<table border=\"1\" cellpadding=\"8\" cellspacing=\"0\">\n";
operation_row("a", $a);
operation_row("b", $b);
operation_row("a + b", $a + $b);
operation_row("a - b", $a - $b);
operation_row("a * b", $a * $b);
if ($b != 0) {
    operation_row("a / b", $a / $b);
} else {
    operation_row("a / b", "division by zero skipped");
}
echo "</table>\n";

demo_footer();
