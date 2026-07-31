<?php
function native_random_family(): array {
    $bytes = random_bytes(16);
    $random = random_int(-4, 4);
    $legacy = rand(10, 20);
    $mt = mt_rand(30, 40);

    $source = ["alpha" => 1, 7 => 2, "omega" => 3];
    $key = array_rand($source);
    $keys = array_rand($source, 2);

    $shuffled = ["left" => 1, 7 => 2, "right" => 3];
    $shuffleResult = shuffle($shuffled);
    $reindexed = array_keys($shuffled) === [0, 1, 2];
    sort($shuffled);

    return [
        strlen($bytes) === 16,
        $random >= -4 && $random <= 4,
        $legacy >= 10 && $legacy <= 20,
        $mt >= 30 && $mt <= 40,
        getrandmax() === 2147483647,
        mt_getrandmax() === 2147483647,
        array_key_exists($key, $source),
        count($keys) === 2,
        $keys[0] !== $keys[1],
        array_key_exists($keys[0], $source),
        array_key_exists($keys[1], $source),
        $shuffleResult,
        $reindexed,
        $shuffled === [1, 2, 3],
    ];
}

var_dump(native_random_family());
