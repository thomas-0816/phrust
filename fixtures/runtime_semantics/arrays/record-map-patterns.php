<?php
// runtime-semantics: category=arrays expect=pass php_ref_required=1
// Record-shaped string-key maps across common application patterns:
// nested config, translation fallback, payload validation, JSON
// responses, route params, dynamic keys, numeric-string keys, unset.
$config = [
    "app" => ["name" => "svc", "debug" => false],
    "db" => ["host" => "localhost", "port" => 5432],
];
echo $config["app"]["name"], "|", $config["db"]["port"], "\n";
$config["db"]["port"] = 6432;
echo $config["db"]["port"], "|", $config["db"]["host"], "\n";

$messages = ["greet" => "hello %s", "bye" => "goodbye"];
function trans($messages, $key) {
    return $messages[$key] ?? "missing:$key";
}
echo sprintf(trans($messages, "greet"), "world"), "|", trans($messages, "nope"), "\n";

$payload = ["email" => "a@b.c", "age" => 44, "tags" => ["x", "y"]];
$errors = [];
foreach (["email", "age", "name"] as $field) {
    if (!isset($payload[$field])) {
        $errors[] = "$field required";
    }
}
echo count($errors), ":", implode(",", $errors), "\n";

$response = ["status" => "ok", "data" => ["items" => [1, 2, 3], "total" => 3]];
echo json_encode($response), "\n";

$route = ["controller" => "user", "action" => "show", "id" => "42"];
foreach ($route as $k => $v) {
    echo "$k=$v;";
}
echo "\n";

$dyn = ["fixed" => 1];
$key = "dy" . "namic";
$dyn[$key] = 2;
echo $dyn["dynamic"], "|", count($dyn), "\n";

$mixedkeys = ["name" => "n"];
$mixedkeys["7"] = "int-coerced";
$mixedkeys["007"] = "stays-string";
var_dump(array_keys($mixedkeys));

$rec = ["a" => 1, "b" => 2, "c" => 3];
unset($rec["b"]);
$rec["b"] = 9;
foreach ($rec as $k => $v) {
    echo "$k:$v,";
}
echo "\n";
$rec[] = "appended";
var_dump(array_keys($rec));
