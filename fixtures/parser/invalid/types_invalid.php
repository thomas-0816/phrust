<?php
// invalid: type declarations are missing type atoms or delimiters

function missing_nullable(? $value): void {
    echo $value;
}

function missing_union(Foo| $value): void {
    echo $value;
}

function missing_dnf((A&B $value): void {
    echo $value;
}
