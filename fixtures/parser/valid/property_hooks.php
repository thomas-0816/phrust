<?php

class HookExamples {
    public string $name {
        get => "name";
        set { echo $value; }
    }

    public int $count {
        get { echo "count"; }
    }
}
