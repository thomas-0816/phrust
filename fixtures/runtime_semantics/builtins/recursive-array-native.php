<?php
function native_recursive_array_family(): array {
    $shared = 41;
    $left = [
        8 => "left-int",
        "scalar" => "left",
        "both" => ["x" => 1, 3 => "left-three"],
        "array-scalar" => ["a" => 1],
        "scalar-array" => "seed",
        "ref" => &$shared,
    ];
    $right = [
        2 => "right-int",
        "scalar" => "right",
        "both" => ["x" => 2, 5 => "right-five"],
        "array-scalar" => "tail",
        "scalar-array" => ["z" => 3],
    ];
    $third = [
        "scalar" => ["deep" => 4],
        "both" => ["y" => 5],
    ];
    $leftBefore = $left;
    $rightBefore = $right;
    $thirdBefore = $third;

    $merged = array_merge_recursive($left, $right, $third, [], [], [], []);
    $replaced = array_replace_recursive($left, $right, $third, [], [], [], []);
    $shared = 42;
    $merged["ref"] = 43;
    $referencePreserved = $shared === 43 && $replaced["ref"] === 43;
    unset($merged["ref"], $replaced["ref"]);

    return [
        "merged" => $merged,
        "replaced" => $replaced,
        "reference-preserved" => $referencePreserved,
        "inputs-unchanged" => [
            $left === $leftBefore,
            $right === $rightBefore,
            $third === $thirdBefore,
        ],
    ];
}

var_dump(native_recursive_array_family());
