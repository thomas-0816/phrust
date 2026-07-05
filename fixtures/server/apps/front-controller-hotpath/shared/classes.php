<?php
class HotpathPost {
    public $id;
    public $title;
    public $meta;

    public function __construct($id, $title, $meta) {
        $this->id = $id;
        $this->title = $title;
        $this->meta = $meta;
    }
}

class HotpathView {
    public $route;
    public $posts;

    public function __construct($route, $posts) {
        $this->route = $route;
        $this->posts = $posts;
    }
}
