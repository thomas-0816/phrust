<?php

function run_exact_path_builtins(): void
{
    echo basename('/srv/www/index.php'), "\n";
    echo basename('/srv/www/index.php', '.php'), "\n";
    echo dirname('/srv/www/wp-content/plugins', 2), "\n";
    var_dump(file_exists(__FILE__));
    echo basename(realpath(__FILE__)), "\n";
    var_dump(realpath(__DIR__ . '/definitely-missing-native-path'));
}

run_exact_path_builtins();
