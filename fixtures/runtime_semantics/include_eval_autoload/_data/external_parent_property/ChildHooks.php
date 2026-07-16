<?php
class ChildHooks extends Vendor\Requests\ParentHooks
{
    protected $url;
    protected $request = [];

    public function __construct($url, $request)
    {
        $this->url = $url;
        $this->request = $request;
    }

    public function dispatch(string $name, array $parameters = []): bool
    {
        $result = parent::dispatch($name, $parameters);
        fixture_action_ref_array("fixture-{$name}", []);
        return $result;
    }
}
