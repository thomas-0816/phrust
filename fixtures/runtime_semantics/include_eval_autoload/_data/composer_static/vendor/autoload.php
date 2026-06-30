<?php
$composerStaticClassMap = [
    "ComposerMapped\\Thing" => __DIR__ . "/../lib/Thing.php",
];
$composerStaticPsr4 = [
    "ComposerStatic\\" => __DIR__ . "/../src/",
];

spl_autoload_register(function ($class) use ($composerStaticClassMap, $composerStaticPsr4) {
    if (isset($composerStaticClassMap[$class])) {
        require $composerStaticClassMap[$class];
        return;
    }

    foreach ($composerStaticPsr4 as $prefix => $baseDir) {
        if (strncmp($class, $prefix, strlen($prefix)) === 0) {
            $relative = substr($class, strlen($prefix));
            require $baseDir . str_replace("\\", "/", $relative) . ".php";
            return;
        }
    }
});
