[CmdletBinding()]
param(
    [Alias("HeadRef")]
    [string]$ComparisonRef = "HEAD",
    [Nullable[int]]$RecentReplayCount = $null,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("sc2coop-cache-compare-" + [guid]::NewGuid().ToString("N"))
$comparisonWorktree = Join-Path $tempRoot "comparison-worktree"
$currentOutput = Join-Path $tempRoot "current-cache_overall_stats.json"
$comparisonOutput = Join-Path $tempRoot "comparison-cache_overall_stats.json"
$currentPrettyOutput = Join-Path $tempRoot "current-cache_overall_stats_pretty.json"
$comparisonPrettyOutput = Join-Path $tempRoot "comparison-cache_overall_stats_pretty.json"
$shouldKeepArtifacts = $KeepArtifacts.IsPresent

function Import-EnvFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    foreach ($line in Get-Content -LiteralPath $Path) {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith("#")) {
            continue
        }

        $parts = $trimmed -split "=", 2
        if ($parts.Length -ne 2) {
            continue
        }

        $name = $parts[0].Trim()
        $value = $parts[1].Trim().Trim('"').Trim("'")
        Set-Item -Path ("Env:" + $name) -Value $value
    }
}

function Resolve-AccountDir {
    foreach ($key in @("SC2_ACCOUNT_PATH", "SC2_ACCOUNT_PATH_WINDOWS", "SC2_ACCOUNT_PATH_LINUX")) {
        $value = [Environment]::GetEnvironmentVariable($key)
        if ([string]::IsNullOrWhiteSpace($value)) {
            continue
        }

        $candidate = $value.Trim().Trim('"').Trim("'")
        if (Test-Path -LiteralPath $candidate -PathType Container) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    throw "No valid SC2 account directory found in .env or current environment."
}

function Invoke-Checked {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$WorkingDirectory
    )

    Push-Location -LiteralPath $WorkingDirectory
    try {
        & $FilePath @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($Arguments -join ' ')"
        }
    }
    finally {
        Pop-Location
    }
}

function Invoke-GenerateCache {
    param(
        [string]$ExePath,
        [string]$AccountDir,
        [string]$OutputFile
    )

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $arguments = @("generate-cache", "--account-dir", $AccountDir, "--output", $OutputFile)
    $output = & $ExePath @arguments 2>&1
    $exitCode = $LASTEXITCODE
    $stopwatch.Stop()

    if ($exitCode -ne 0) {
        $joined = ($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
        throw "Cache generation failed with exit code $exitCode.`n$joined"
    }

    $entryCount = ((Get-Content -LiteralPath $OutputFile -Raw | ConvertFrom-Json) | Measure-Object).Count

    [PSCustomObject]@{
        ElapsedSeconds = $stopwatch.Elapsed.TotalSeconds
        EntryCount = $entryCount
        Output = ($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
    }
}

function Get-FileDigest {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Expected file was not created: $Path"
    }

    $hash = Get-FileHash -LiteralPath $Path -Algorithm SHA256
    [PSCustomObject]@{
        Hash = $hash.Hash
        Size = (Get-Item -LiteralPath $Path).Length
    }
}

function New-RecentReplaySubset {
    param(
        [string]$SourceAccountDir,
        [string]$DestinationAccountDir,
        [int]$ReplayCount
    )

    if ($ReplayCount -le 0) {
        throw "RecentReplayCount must be greater than zero when supplied."
    }

    $replayFiles = Get-ChildItem -LiteralPath $SourceAccountDir -Recurse -File |
        Where-Object { $_.Extension -ieq ".SC2Replay" } |
        Sort-Object -Property @{ Expression = { $_.LastWriteTimeUtc }; Descending = $true }, @{ Expression = { $_.FullName.ToLowerInvariant() }; Descending = $false } |
        Select-Object -First $ReplayCount

    if ($replayFiles.Count -eq 0) {
        throw "No replay files found under account directory: $SourceAccountDir"
    }

    foreach ($replayFile in $replayFiles) {
        $relativePath = [System.IO.Path]::GetRelativePath($SourceAccountDir, $replayFile.FullName)
        $destinationPath = Join-Path $DestinationAccountDir $relativePath
        $destinationParent = Split-Path -Parent $destinationPath
        if (-not (Test-Path -LiteralPath $destinationParent)) {
            New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
        }
        Copy-Item -LiteralPath $replayFile.FullName -Destination $destinationPath -Force
    }

    return $replayFiles.Count
}

New-Item -ItemType Directory -Path $tempRoot | Out-Null

try {
    Import-EnvFile -Path (Join-Path $repoRoot ".env")
    $accountDir = Resolve-AccountDir
    $benchmarkAccountDir = $accountDir
    $selectedReplayCount = $null
    if ($null -ne $RecentReplayCount) {
        $subsetRoot = Join-Path $tempRoot "StarCraft II"
        $benchmarkAccountDir = Join-Path $subsetRoot "Accounts"
        $selectedReplayCount = New-RecentReplaySubset -SourceAccountDir $accountDir -DestinationAccountDir $benchmarkAccountDir -ReplayCount $RecentReplayCount
    }
    $comparisonCommit = (& git -C $repoRoot rev-parse $ComparisonRef).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($comparisonCommit)) {
        throw "Failed to resolve git ref '$ComparisonRef'."
    }

    Invoke-Checked -FilePath "cargo" -Arguments @(
        "build",
        "--release",
        "--manifest-path",
        "s2coop-analyzer/Cargo.toml",
        "--bin",
        "s2coop-analyzer-cli"
    ) -WorkingDirectory $repoRoot

    Invoke-Checked -FilePath "git" -Arguments @(
        "-C",
        $repoRoot,
        "worktree",
        "add",
        "--detach",
        $comparisonWorktree,
        $comparisonCommit
    ) -WorkingDirectory $repoRoot

    Invoke-Checked -FilePath "cargo" -Arguments @(
        "build",
        "--release",
        "--manifest-path",
        "s2coop-analyzer/Cargo.toml",
        "--bin",
        "s2coop-analyzer-cli"
    ) -WorkingDirectory $comparisonWorktree

    $currentExe = Join-Path $repoRoot "s2coop-analyzer\target\release\s2coop-analyzer-cli.exe"
    $comparisonExe = Join-Path $comparisonWorktree "s2coop-analyzer\target\release\s2coop-analyzer-cli.exe"

    $currentRun = Invoke-GenerateCache -ExePath $currentExe -AccountDir $benchmarkAccountDir -OutputFile $currentOutput
    $comparisonRun = Invoke-GenerateCache -ExePath $comparisonExe -AccountDir $benchmarkAccountDir -OutputFile $comparisonOutput

    $currentDigest = Get-FileDigest -Path $currentOutput
    $comparisonDigest = Get-FileDigest -Path $comparisonOutput
    $currentPrettyDigest = Get-FileDigest -Path $currentPrettyOutput
    $comparisonPrettyDigest = Get-FileDigest -Path $comparisonPrettyOutput

    $mainEqual = $currentDigest.Hash -eq $comparisonDigest.Hash -and $currentDigest.Size -eq $comparisonDigest.Size
    $prettyEqual = $currentPrettyDigest.Hash -eq $comparisonPrettyDigest.Hash -and $currentPrettyDigest.Size -eq $comparisonPrettyDigest.Size
    $deltaSeconds = $currentRun.ElapsedSeconds - $comparisonRun.ElapsedSeconds
    $ratio = if ($comparisonRun.ElapsedSeconds -le 0) { 0.0 } else { $currentRun.ElapsedSeconds / $comparisonRun.ElapsedSeconds }

    Write-Host "Comparison ref: $ComparisonRef"
    Write-Host "Comparison commit: $comparisonCommit"
    Write-Host "Account dir: $accountDir"
    if ($null -ne $RecentReplayCount) {
        Write-Host "Replay scope: recent $selectedReplayCount files"
        Write-Host "Benchmark account dir: $benchmarkAccountDir"
    } else {
        Write-Host "Replay scope: all replay files"
    }
    Write-Host "Current entry count: $($currentRun.EntryCount)"
    Write-Host "Comparison entry count: $($comparisonRun.EntryCount)"
    Write-Host "Main cache byte-identical: $mainEqual"
    Write-Host "Pretty cache byte-identical: $prettyEqual"
    Write-Host ("Current elapsed seconds: {0:N3}" -f $currentRun.ElapsedSeconds)
    Write-Host ("Comparison elapsed seconds: {0:N3}" -f $comparisonRun.ElapsedSeconds)
    Write-Host ("Delta seconds (current - comparison): {0:N3}" -f $deltaSeconds)
    Write-Host ("Runtime ratio (current / comparison): {0:N4}x" -f $ratio)
    Write-Host "Current output: $currentOutput"
    Write-Host "Comparison output: $comparisonOutput"
    if (-not $mainEqual -or -not $prettyEqual) {
        $shouldKeepArtifacts = $true
        Write-Host "Artifacts kept for inspection: $tempRoot"
    } elseif ($KeepArtifacts) {
        $shouldKeepArtifacts = $true
        Write-Host "Artifacts kept by request: $tempRoot"
    }
}
finally {
    if (Test-Path -LiteralPath $comparisonWorktree) {
        & git -C $repoRoot worktree remove --force $comparisonWorktree | Out-Null
    }

    if (-not $shouldKeepArtifacts -and (Test-Path -LiteralPath $tempRoot)) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
