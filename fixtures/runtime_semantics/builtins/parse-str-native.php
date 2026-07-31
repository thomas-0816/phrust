<?php
// runtime-semantics: category=builtins expect=pass

function run_native_parse_str(): array
{
    parse_str(
        'plain=value&list[]=a&list[]=b&12=numeric&nested[x]=old&nested[x]=new&flip=scalar&flip[child]=nested&collapse[child]=nested&collapse=scalar&extra1=1&extra2=2',
        $parsed,
    );
    parse_str('plain=replaced&next=owner', $replacement);

    return [$parsed, $replacement];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_parse_str();
}
var_dump($result);
