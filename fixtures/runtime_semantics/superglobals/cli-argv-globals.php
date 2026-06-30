<?php
// runtime-semantics: category=superglobals expect=pass args=alpha,beta
echo $argc, "\n";
echo $argv[0] === __FILE__ ? "script" : "bad", "\n";
echo $argv[1], ":", $argv[2], "\n";
echo $_SERVER["argc"], "\n";
echo $_SERVER["argv"][2], "\n";
$GLOBALS["from_globals"] = "live";
echo $from_globals, "\n";
echo empty($_GET) && empty($_POST) && empty($_COOKIE) && empty($_FILES) && empty($_REQUEST) ? "request-empty" : "request-set", "\n";
