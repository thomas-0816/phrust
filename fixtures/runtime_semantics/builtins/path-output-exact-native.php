<?php
// runtime-semantics: category=builtins expect=pass

function run_native_path_outputs(): array
{
    return [
        basename('/srv/www/index.php'),
        basename('/srv/www/index.php', '.php'),
        basename('/srv/www/'),
        basename('/'),
        dirname('/srv/www/wp-content/plugins', 2),
        dirname('index.php'),
        dirname('///'),
        pathinfo('/srv/www/index.php'),
        pathinfo('/srv/www/index.php', PATHINFO_DIRNAME),
        pathinfo('/srv/www/index.php', PATHINFO_BASENAME),
        pathinfo('/srv/www/index.php', PATHINFO_EXTENSION),
        pathinfo('/srv/www/index.php', PATHINFO_FILENAME),
        bin2hex(basename(hex2bin('726f6f742fff6e616d652e617263686976652e746172'))),
        bin2hex(dirname(hex2bin('726f6f742fff6e616d652e617263686976652e746172'))),
        bin2hex(pathinfo(hex2bin('726f6f742fff6e616d652e617263686976652e746172'), PATHINFO_DIRNAME)),
        bin2hex(pathinfo(hex2bin('726f6f742fff6e616d652e617263686976652e746172'), PATHINFO_BASENAME)),
        bin2hex(pathinfo(hex2bin('726f6f742fff6e616d652e617263686976652e746172'), PATHINFO_EXTENSION)),
        bin2hex(pathinfo(hex2bin('726f6f742fff6e616d652e617263686976652e746172'), PATHINFO_FILENAME)),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_path_outputs();
}

var_dump($result);
