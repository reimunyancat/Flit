param([Parameter(ValueFromRemainingArguments=$true)] [string[]] $Items)
$Base = if ($env:FLIT_URL) { $env:FLIT_URL } else { "http://localhost:7777" }
$Headers = @{}
if ($env:FLIT_TOKEN) { $Headers["X-Flit-Token"] = $env:FLIT_TOKEN }
function Send-Text($t) { Invoke-RestMethod -Uri "$Base/api/text" -Method Post -Headers $Headers -ContentType "text/plain" -Body $t | Out-Null; Write-Host "sent text" }
function Send-File($p) { Invoke-RestMethod -Uri "$Base/api/file" -Method Post -Headers $Headers -Form @{ file = Get-Item -Path $p } | Out-Null; Write-Host "sent $p" }
if (-not $Items -or $Items.Count -eq 0) { $stdin=[Console]::In.ReadToEnd(); if ($stdin) { Send-Text $stdin }; return }
foreach ($a in $Items) { if (Test-Path -Path $a -PathType Leaf) { Send-File $a } else { Send-Text $a } }