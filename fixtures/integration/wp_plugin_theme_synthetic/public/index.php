<?php
$root = dirname(__DIR__);
require $root . "/app/hooks.php";
require $root . "/plugins/synthetic-plugin.php";
require $root . "/themes/synthetic-theme/template.php";

$state = [
    "root" => $root,
    "options" => [
        "site_name" => $_GET["name"] ?? "synthetic",
    ],
];

$state = synthetic_do_action("init", $state);

$options_path = $root . "/var/options.txt";
file_put_contents($options_path, serialize($state["options"]));
$state["options"] = unserialize(file_get_contents($options_path));
unlink($options_path);

$state = synthetic_do_action("package", $state);
$state = synthetic_do_action("upload", $state);

if (isset($_GET["redirect"])) {
    setcookie("synthetic_demo", "redirect", ["path" => "/", "samesite" => "Lax"]);
    header("Location: /activated", true, 302);
    echo "redirect\n";
    return;
}

setcookie("synthetic_demo", "enabled", ["path" => "/", "samesite" => "Lax"]);
header("X-Synthetic-Fixture: ok");

ob_start();
synthetic_render_template($state);
$body = ob_get_clean();

echo $body;
