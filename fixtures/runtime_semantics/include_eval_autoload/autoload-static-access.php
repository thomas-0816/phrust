<?php
spl_autoload_register(function ($class) {
    if ($class === "AutoloadStaticAccessFixture") {
        include (__DIR__ . "/_data/AutoloadStaticAccessFixture.php");
    }
});

echo AutoloadStaticAccessFixture::VALUE, "|";
echo AutoloadStaticAccessFixture::$prop, "|";
echo AutoloadStaticAccessFixture::method(), "|";
echo isset(AutoloadStaticAccessFixture::$prop) ? "isset" : "missing";
echo "|";
echo empty(AutoloadStaticAccessFixture::$missing) ? "empty" : "not-empty";
echo "\n";
