<?php
// runtime-semantics: category=regressions expect=pass regression_category=objects-arrays reference_behavior=stdout:string-x16 regression_case=native-property-array-assignment-borrowed-result

class NativePropertyPipeline
{
    private array $output;

    public function __construct(array $input)
    {
        $this->output = $input;
    }

    public function filterPublic(): array
    {
        $filtered = array();
        foreach ($this->output as $key => $value) {
            if ($value->public) {
                $filtered[$key] = $value;
            }
        }
        $this->output = $filtered;
        return $this->output;
    }

    public function pluckName(): array
    {
        $names = array();
        foreach ($this->output as $key => $value) {
            $names[$key] = $value->name;
        }
        $this->output = $names;
        return $this->output;
    }

    public function output(): array
    {
        return $this->output;
    }
}

for ($iteration = 0; $iteration < 16; $iteration++) {
    $public = new stdClass();
    $public->public = true;
    $public->name = 'publish';

    $private = new stdClass();
    $private->public = false;
    $private->name = 'draft';

    $pipeline = new NativePropertyPipeline(array($public, $private));
    $pipeline->filterPublic();
    $pipeline->pluckName();

    foreach ($pipeline->output() as $name) {
        echo get_debug_type($name), "\n";
    }
}
