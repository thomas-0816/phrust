<?php

#[FileAttr]
function marked(#[ParamAttr] string $name): void {
    echo $name;
}

$closure = #[ClosureAttr(name: "c", flags: [1, 2])] function (#[ParamAttr] int $x): void {
    echo $x;
};

$arrow = #[ArrowAttr(B::class)] fn(#[ParamAttr] int $x): int => $x;

#[ClassAttr]
class Annotated {
    #[ConstAttr]
    public const VALUE = 1;

    #[PropertyAttr]
    public string $name = "x";

    #[MethodAttr]
    public function run(): void {
        echo self::VALUE;
    }
}
