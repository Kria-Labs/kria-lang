param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RunnerArguments
)

$ErrorActionPreference = "Stop"
$Runner = Join-Path $PSScriptRoot "run.mjs"

& node $Runner @RunnerArguments
exit $LASTEXITCODE
