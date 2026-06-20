<?php
function values($items) {
    yield 1;
    yield "key" => 2;
    yield from $items;
}
