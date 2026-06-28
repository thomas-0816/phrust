<?php
require "lib/page.php";

function greeting($name)
{
    if ($name == "") {
        return "Hello from phrust";
    }
    return "Hello, " . $name . "!";
}

$name = "";
if (isset($_POST["name"])) {
    $name = $_POST["name"];
}

demo_title("POST form");

echo "<form method=\"post\" action=\"/hello.php\">\n";
echo "<label>Name <input name=\"name\" value=\"", demo_value($name), "\"></label>\n";
echo "<button type=\"submit\">Send</button>\n";
echo "</form>\n";

echo "<p>", demo_value(greeting($name)), "</p>\n";
echo "<p>Request method: ", demo_value($_SERVER["REQUEST_METHOD"]), "</p>\n";

demo_footer();
