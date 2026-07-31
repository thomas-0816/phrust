<?php
// runtime-semantics: category=builtins expect=pass

function run_native_tokenizer(string $source): array
{
    return token_get_all($source);
}

$source = "<?php\n// native tokenizer\nfunction add(\$left, \$right) {\n    return \$left + \$right;\n}\necho add(1, 2);\n";
$tokens = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $tokens = run_native_tokenizer($source);
}

var_dump($tokens);
