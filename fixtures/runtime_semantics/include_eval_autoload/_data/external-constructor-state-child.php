<?php
class ExternalConstructorState
{
    protected $user;
    protected $password;
    protected $database;
    protected $host;
    protected $active = false;

    public function __construct(
        $user,
        $password,
        $database,
        $host,
    )
    {
        $this->user = $user;
        $this->password = $password;
        $this->database = $database;
        $this->host = $host;
        $this->connect();
    }

    private function connect(): void
    {
        echo $this->user, ':', $this->database, '@', $this->host, "\n";
    }

    public function show(bool $allow = true): void
    {
        $this->active = true;
        echo $this->user, ':', $this->database, '@', $this->host, "\n";
    }
}
