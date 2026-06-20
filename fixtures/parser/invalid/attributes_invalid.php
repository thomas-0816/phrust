<?php
// invalid: attribute argument list is not closed

#[Broken(name: "x")
function broken(): void {
    echo "x";
}
