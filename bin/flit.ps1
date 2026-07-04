#!/usr/bin/env pwsh
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [String[]]$Rest
)

$ErrorActionPreference = "Stop"
$FlitUrl = if ($env:FLIT_URL) { $env:FLIT_URL } else { "http://127.0.0.1:7777" }
$Headers = @{}
if ($env:FLIT_TOKEN) { $Headers["Authorization"] = "Bearer $($env:FLIT_TOKEN)" }

function Show-Usage {
    @"
flit - drop anything into your Flit inbox

Usage:
    flit <text...>      send text
    flit -f <file>      send a file
    echo hi | flit      send stdin as text
    flit -l             list recent items
"@ | Write-Host
    exit 1
}

if ([Console]::IsInputRedirected) {
    $body = [Console]::In.ReadToEnd()
    if (-not [String]::IsNullOrWhiteSpace($body)) {
        Invoke-RestMethod -Uri "$FlitUrl/api/text" -Method Post -Headers $Headers `
            -ContentType "text/plain; charset=utf-8" -Body $body | Out-Null
        Write-Host "sent (text, stdin)"
    }
    exit 0
}

if ($Rest.Count -eq 0) { Show-Usage }

switch ($Rest[0]) {
    { $_ -in "-l", "--list"} {
        Invoke-RestMethod -Uri "$FlitUrl/api/items" -Headers $Headers | ConvertTo-Json -Depth 5
        break
    }
    { $_ -in "-f", "--file"} {
        $files = $Rest | Select-Object -Skip 1
        if ($files.Count -eq 0) { Show-Usage }
        foreach ($f in $files) {
            if (-not (Test-Path -LiteralPath $f -PathType Leaf)) {
                Write-Error "not a file: $f"
                exit 1
            }
            Invoke-RestMethod -Uri "$FlitUrl/api/file" -Method Post Headers $Headers `
                -Form @{ file = Get-Item -LiteralPath $f} | Out-Null
            Write-Host "sent (file): $f"
        }
        break
    }
    { $_ -in "-h", "--help" } { Show-Usage }
    default {
        $body = $Rest -join " "
        Invoke-RestMethod -Uri "$FlitUrl/api/text" -Method Post Headers $Headers `
            -ContentType "text/plain; charset=utf-8" -Body $body | Out-Null
        Write-Host "sent (text)"
    }
}