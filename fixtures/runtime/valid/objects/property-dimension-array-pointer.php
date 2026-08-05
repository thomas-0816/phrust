<?php
class CursorBox {
    public array $iterations = [[10, 20, 30]];

    public function advance(int $level): mixed {
        return next($this->iterations[$level]);
    }
}

$box = new CursorBox();
var_dump($box->advance(0));
var_dump($box->advance(0));
var_dump($box->advance(0));
var_dump(current($box->iterations[0]));
