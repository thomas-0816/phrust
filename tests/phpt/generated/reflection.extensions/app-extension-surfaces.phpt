--TEST--
reflection.extensions: app extension classes and methods expose owners
--DESCRIPTION--
Generated reflection coverage for extension-owned classes and arginfo surfaces
added by the app extension prompt pack.
--EXTENSIONS--
reflection
pdo
curl
zip
fileinfo
openssl
dom
intl
--FILE--
<?php
$classes = [
    ["PDO", "pdo"],
    ["PDOStatement", "pdo"],
    ["CurlHandle", "curl"],
    ["ZipArchive", "zip"],
    ["finfo", "fileinfo"],
    ["OpenSSLAsymmetricKey", "openssl"],
    ["OpenSSLCertificate", "openssl"],
    ["DOMDocument", "dom"],
    ["DOMElement", "dom"],
    ["Normalizer", "intl"],
    ["Locale", "intl"],
    ["NumberFormatter", "intl"],
];

foreach ($classes as [$class, $extension]) {
    $reflection = new ReflectionClass($class);
    echo $reflection->getName(), ":", strtolower($reflection->getExtensionName()), ":",
        $reflection->isInternal() ? "internal" : "user", "\n";
}

$functions = [
    ["pdo_drivers", "pdo"],
    ["curl_init", "curl"],
    ["zip_open", "zip"],
    ["finfo_open", "fileinfo"],
    ["openssl_pkey_new", "openssl"],
    ["normalizer_normalize", "intl"],
];

foreach ($functions as [$function, $extension]) {
    $reflection = new ReflectionFunction($function);
    echo $reflection->getName(), ":", strtolower($reflection->getExtensionName()), "\n";
}

$methods = [
    ["PDO", "prepare", "pdo", "query"],
    ["ZipArchive", "open", "zip", "filename"],
    ["finfo", "file", "fileinfo", "filename"],
    ["DOMDocument", "loadXML", "dom", "source"],
    ["Normalizer", "normalize", "intl", "string"],
];

foreach ($methods as [$class, $method, $extension, $firstParameter]) {
    $reflection = new ReflectionMethod($class, $method);
    $parameters = $reflection->getParameters();
    echo $class, "::", $reflection->getName(), ":",
        strtolower($reflection->getExtensionName()), ":",
        $parameters[0]->getName(), ":",
        ($parameters[0]->getPosition() === 0 ? "pos0" : "badpos"), "\n";
}
?>
--EXPECT--
PDO:pdo:internal
PDOStatement:pdo:internal
CurlHandle:curl:internal
ZipArchive:zip:internal
finfo:fileinfo:internal
OpenSSLAsymmetricKey:openssl:internal
OpenSSLCertificate:openssl:internal
DOMDocument:dom:internal
DOMElement:dom:internal
Normalizer:intl:internal
Locale:intl:internal
NumberFormatter:intl:internal
pdo_drivers:pdo
curl_init:curl
zip_open:zip
finfo_open:fileinfo
openssl_pkey_new:openssl
normalizer_normalize:intl
PDO::prepare:pdo:query:pos0
ZipArchive::open:zip:filename:pos0
finfo::file:fileinfo:filename:pos0
DOMDocument::loadXML:dom:source:pos0
Normalizer::normalize:intl:string:pos0
