[CmdletBinding()]
param(
    [Alias("HeadRef")]
    [string]$ComparisonRef = "HEAD",
    [Nullable[int]]$RecentReplayCount = $null,
    [int]$Runs = 1,
    [Nullable[int]]$Workers = $null,
    [int]$WarmupRunsPerVariant = 1,
    [switch]$AnalyzerTimings,
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
$cargoJobs = [Math]::Max(1, [int][Math]::Floor([Environment]::ProcessorCount / 2))
$cliExecutableName = if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    "s2coop-analyzer-cli.exe"
} else {
    "s2coop-analyzer-cli"
}

if ($Runs -le 0) {
    throw "Runs must be greater than zero."
}

if ($null -ne $Workers -and $Workers -le 0) {
    throw "Workers must be greater than zero when supplied."
}

if ($WarmupRunsPerVariant -ne 1) {
    throw "WarmupRunsPerVariant must be exactly 1."
}

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
        if ($trimmed.StartsWith("export ")) {
            $trimmed = $trimmed.Substring(7).Trim()
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
        [string]$OutputFile,
        [Nullable[int]]$WorkerCount,
        [bool]$EnableAnalyzerTimings
    )

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $arguments = @("generate-cache", "--account-dir", $AccountDir, "--output", $OutputFile)
    if ($null -ne $WorkerCount) {
        $arguments += @("--workers", $WorkerCount.ToString())
    }

    $previousTimingValue = [Environment]::GetEnvironmentVariable("S2COOP_ANALYZER_TIMINGS")
    if ($EnableAnalyzerTimings) {
        Set-Item -Path "Env:S2COOP_ANALYZER_TIMINGS" -Value "1"
    }
    $output = & $ExePath @arguments 2>&1
    $exitCode = $LASTEXITCODE
    if ($EnableAnalyzerTimings) {
        if ($null -eq $previousTimingValue) {
            Remove-Item -Path "Env:S2COOP_ANALYZER_TIMINGS" -ErrorAction SilentlyContinue
        } else {
            Set-Item -Path "Env:S2COOP_ANALYZER_TIMINGS" -Value $previousTimingValue
        }
    }
    $stopwatch.Stop()

    if ($exitCode -ne 0) {
        $joined = ($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
        throw "Cache generation failed with exit code $exitCode.`n$joined"
    }

    $entryCount = ((Get-Content -LiteralPath $OutputFile -Raw | ConvertFrom-Json) | Measure-Object).Count
    $outputText = ($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
    $analyzerTotalSeconds = $null
    $decodeOrderedSeconds = $null
    $detailedReportSeconds = $null
    if ($outputText -match "total=([0-9.]+)s") {
        $analyzerTotalSeconds = [double]$Matches[1]
    }
    if ($outputText -match "decode_ordered=([0-9.]+)s") {
        $decodeOrderedSeconds = [double]$Matches[1]
    }
    if ($outputText -match "detailed_report=([0-9.]+)s") {
        $detailedReportSeconds = [double]$Matches[1]
    }

    [PSCustomObject]@{
        ElapsedSeconds = $stopwatch.Elapsed.TotalSeconds
        EntryCount = $entryCount
        AnalyzerTotalSeconds = $analyzerTotalSeconds
        DecodeOrderedSeconds = $decodeOrderedSeconds
        DetailedReportSeconds = $detailedReportSeconds
        Output = $outputText
    }
}

function Remove-GeneratedCacheOutputs {
    param([string]$OutputFile)

    $directory = Split-Path -Parent $OutputFile
    $name = [System.IO.Path]::GetFileNameWithoutExtension($OutputFile)
    $extension = [System.IO.Path]::GetExtension($OutputFile)
    $prettyOutputFile = Join-Path $directory ($name + "_pretty" + $extension)

    foreach ($path in @($OutputFile, $prettyOutputFile)) {
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            Remove-Item -LiteralPath $path -Force
        }
    }
}

function Invoke-WarmupGenerateCache {
    param(
        [string]$Variant,
        [string]$ExePath,
        [string]$AccountDir,
        [Nullable[int]]$WorkerCount,
        [bool]$EnableAnalyzerTimings
    )

    $outputFile = Join-Path $tempRoot ("warmup-{0}-cache_overall_stats.json" -f $Variant)
    Write-Host ("Warm-up {0}: starting" -f $Variant)
    try {
        $run = Invoke-GenerateCache `
            -ExePath $ExePath `
            -AccountDir $AccountDir `
            -OutputFile $outputFile `
            -WorkerCount $WorkerCount `
            -EnableAnalyzerTimings $EnableAnalyzerTimings
        Write-Host (
            "Warm-up {0}: elapsed={1:N3}s entries={2} discarded" -f `
                $Variant,
                $run.ElapsedSeconds,
                $run.EntryCount
        )
    }
    finally {
        Remove-GeneratedCacheOutputs -OutputFile $outputFile
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

function Get-OptionalFileDigest {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }

    return Get-FileDigest -Path $Path
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

    $replayFiles = @(Get-ChildItem -LiteralPath $SourceAccountDir -Recurse -File |
        Where-Object { $_.Extension -ieq ".SC2Replay" } |
        Sort-Object -Property @{ Expression = { $_.LastWriteTimeUtc }; Descending = $true }, @{ Expression = { $_.FullName.ToLowerInvariant() }; Descending = $false } |
        Select-Object -First $ReplayCount)

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

function Get-AlternatingVariant {
    param([int]$RunIndex)

    $phase = $RunIndex % 4
    if ($phase -eq 0 -or $phase -eq 3) {
        return "comparison"
    }

    return "current"
}

function Get-MeanSeconds {
    param(
        [object[]]$Rows,
        [string]$PropertyName
    )

    $values = @($Rows |
        ForEach-Object { $_.$PropertyName } |
        Where-Object { $null -ne $_ })
    if ($values.Count -eq 0) {
        return $null
    }

    return ($values | Measure-Object -Average).Average
}

function Format-OptionalSeconds {
    param([Nullable[double]]$Value)

    if ($null -eq $Value) {
        return "n/a"
    }

    return ("{0:N3}" -f $Value)
}

New-Item -ItemType Directory -Path $tempRoot | Out-Null

try {
    Import-EnvFile -Path (Join-Path $repoRoot ".env")
    Import-EnvFile -Path (Join-Path $repoRoot ".envrc")
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
        "--jobs",
        $cargoJobs.ToString(),
        "--target-dir",
        ([System.IO.Path]::Combine($repoRoot, "target")),
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
        "--jobs",
        $cargoJobs.ToString(),
        "--target-dir",
        ([System.IO.Path]::Combine($comparisonWorktree, "target")),
        "--manifest-path",
        "s2coop-analyzer/Cargo.toml",
        "--bin",
        "s2coop-analyzer-cli"
    ) -WorkingDirectory $comparisonWorktree

    $currentExe = [System.IO.Path]::Combine($repoRoot, "target", "release", $cliExecutableName)
    $comparisonExe = [System.IO.Path]::Combine($comparisonWorktree, "target", "release", $cliExecutableName)

    for ($warmupIndex = 0; $warmupIndex -lt $WarmupRunsPerVariant; $warmupIndex++) {
        Invoke-WarmupGenerateCache `
            -Variant "comparison" `
            -ExePath $comparisonExe `
            -AccountDir $benchmarkAccountDir `
            -WorkerCount $Workers `
            -EnableAnalyzerTimings $AnalyzerTimings.IsPresent
        Invoke-WarmupGenerateCache `
            -Variant "current" `
            -ExePath $currentExe `
            -AccountDir $benchmarkAccountDir `
            -WorkerCount $Workers `
            -EnableAnalyzerTimings $AnalyzerTimings.IsPresent
    }

    $runRows = New-Object System.Collections.Generic.List[object]
    for ($runIndex = 0; $runIndex -lt $Runs; $runIndex++) {
        $runNumber = $runIndex + 1
        $variant = if ($Runs -eq 1) { "comparison" } else { Get-AlternatingVariant -RunIndex $runIndex }
        $exePath = if ($variant -eq "current") { $currentExe } else { $comparisonExe }
        $outputPrefix = "{0:D2}-{1}" -f $runNumber, $variant
        $outputFile = Join-Path $tempRoot ($outputPrefix + "-cache_overall_stats.json")
        $prettyOutputFile = Join-Path $tempRoot ($outputPrefix + "-cache_overall_stats_pretty.json")
        $run = Invoke-GenerateCache `
            -ExePath $exePath `
            -AccountDir $benchmarkAccountDir `
            -OutputFile $outputFile `
            -WorkerCount $Workers `
            -EnableAnalyzerTimings $AnalyzerTimings.IsPresent
        $digest = Get-FileDigest -Path $outputFile
        $prettyDigest = Get-OptionalFileDigest -Path $prettyOutputFile
        $row = [PSCustomObject]@{
            Run = $runNumber
            Variant = $variant
            ElapsedSeconds = $run.ElapsedSeconds
            AnalyzerTotalSeconds = $run.AnalyzerTotalSeconds
            DecodeOrderedSeconds = $run.DecodeOrderedSeconds
            DetailedReportSeconds = $run.DetailedReportSeconds
            EntryCount = $run.EntryCount
            Hash = $digest.Hash
            Size = $digest.Size
            PrettyHash = if ($null -eq $prettyDigest) { $null } else { $prettyDigest.Hash }
            PrettySize = if ($null -eq $prettyDigest) { $null } else { $prettyDigest.Size }
            OutputFile = $outputFile
        }
        $runRows.Add($row)

        Write-Host (
            "Run {0:D2} {1}: elapsed={2:N3}s analyzer_total={3} decode_ordered={4} detailed_report={5} entries={6} sha={7}" -f `
                $runNumber,
                $variant,
                $row.ElapsedSeconds,
                (Format-OptionalSeconds -Value $row.AnalyzerTotalSeconds),
                (Format-OptionalSeconds -Value $row.DecodeOrderedSeconds),
                (Format-OptionalSeconds -Value $row.DetailedReportSeconds),
                $row.EntryCount,
                $row.Hash.Substring(0, 16)
        )
    }

    if ($Runs -eq 1) {
        $currentRun = Invoke-GenerateCache `
            -ExePath $currentExe `
            -AccountDir $benchmarkAccountDir `
            -OutputFile $currentOutput `
            -WorkerCount $Workers `
            -EnableAnalyzerTimings $AnalyzerTimings.IsPresent
        $currentDigest = Get-FileDigest -Path $currentOutput
        $currentPrettyDigest = Get-OptionalFileDigest -Path $currentPrettyOutput
        $runRows.Add([PSCustomObject]@{
            Run = 2
            Variant = "current"
            ElapsedSeconds = $currentRun.ElapsedSeconds
            AnalyzerTotalSeconds = $currentRun.AnalyzerTotalSeconds
            DecodeOrderedSeconds = $currentRun.DecodeOrderedSeconds
            DetailedReportSeconds = $currentRun.DetailedReportSeconds
            EntryCount = $currentRun.EntryCount
            Hash = $currentDigest.Hash
            Size = $currentDigest.Size
            PrettyHash = if ($null -eq $currentPrettyDigest) { $null } else { $currentPrettyDigest.Hash }
            PrettySize = if ($null -eq $currentPrettyDigest) { $null } else { $currentPrettyDigest.Size }
            OutputFile = $currentOutput
        })
    }

    $currentRows = @($runRows | Where-Object { $_.Variant -eq "current" })
    $comparisonRows = @($runRows | Where-Object { $_.Variant -eq "comparison" })
    $currentMean = Get-MeanSeconds -Rows $currentRows -PropertyName "ElapsedSeconds"
    $comparisonMean = Get-MeanSeconds -Rows $comparisonRows -PropertyName "ElapsedSeconds"
    $currentAnalyzerMean = Get-MeanSeconds -Rows $currentRows -PropertyName "AnalyzerTotalSeconds"
    $comparisonAnalyzerMean = Get-MeanSeconds -Rows $comparisonRows -PropertyName "AnalyzerTotalSeconds"
    $currentDecodeMean = Get-MeanSeconds -Rows $currentRows -PropertyName "DecodeOrderedSeconds"
    $comparisonDecodeMean = Get-MeanSeconds -Rows $comparisonRows -PropertyName "DecodeOrderedSeconds"
    $currentDetailedMean = Get-MeanSeconds -Rows $currentRows -PropertyName "DetailedReportSeconds"
    $comparisonDetailedMean = Get-MeanSeconds -Rows $comparisonRows -PropertyName "DetailedReportSeconds"

    $mainDigestKeys = @($runRows | ForEach-Object { "$($_.Hash):$($_.Size)" } | Sort-Object -Unique)
    $prettyDigestRows = @($runRows | Where-Object { $null -ne $_.PrettyHash })
    $prettyDigestKeys = @($prettyDigestRows | ForEach-Object { "$($_.PrettyHash):$($_.PrettySize)" } | Sort-Object -Unique)
    $mainEqual = $mainDigestKeys.Count -eq 1
    $prettyEqual = if ($prettyDigestRows.Count -eq 0) { $null } else { $prettyDigestKeys.Count -eq 1 }
    $entryCounts = @($runRows | Select-Object -ExpandProperty EntryCount -Unique)
    $deltaSeconds = if ($null -eq $currentMean -or $null -eq $comparisonMean) { $null } else { $currentMean - $comparisonMean }
    $ratio = if ($null -eq $currentMean -or $null -eq $comparisonMean -or $comparisonMean -le 0.0) { $null } else { $currentMean / $comparisonMean }
    $csvPath = Join-Path $tempRoot "cache-generation-comparison-runs.csv"
    $summaryPath = Join-Path $tempRoot "cache-generation-comparison-summary.json"
    $runRows | Export-Csv -NoTypeInformation -Path $csvPath
    [PSCustomObject]@{
        ComparisonRef = $ComparisonRef
        ComparisonCommit = $comparisonCommit
        Runs = $Runs
        WarmupRunsPerVariant = $WarmupRunsPerVariant
        Workers = $Workers
        AnalyzerTimings = $AnalyzerTimings.IsPresent
        CurrentMeanSeconds = $currentMean
        ComparisonMeanSeconds = $comparisonMean
        DeltaSeconds = $deltaSeconds
        RuntimeRatio = $ratio
        CurrentAnalyzerMeanSeconds = $currentAnalyzerMean
        ComparisonAnalyzerMeanSeconds = $comparisonAnalyzerMean
        CurrentDecodeOrderedMeanSeconds = $currentDecodeMean
        ComparisonDecodeOrderedMeanSeconds = $comparisonDecodeMean
        CurrentDetailedReportMeanSeconds = $currentDetailedMean
        ComparisonDetailedReportMeanSeconds = $comparisonDetailedMean
        MainCacheByteIdentical = $mainEqual
        PrettyCacheByteIdentical = $prettyEqual
        EntryCounts = $entryCounts
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath

    Write-Host "Comparison ref: $ComparisonRef"
    Write-Host "Comparison commit: $comparisonCommit"
    Write-Host "Account dir: $accountDir"
    if ($null -ne $RecentReplayCount) {
        Write-Host "Replay scope: recent $selectedReplayCount files"
        Write-Host "Benchmark account dir: $benchmarkAccountDir"
    } else {
        Write-Host "Replay scope: all replay files"
    }
    Write-Host "Runs: $Runs"
    Write-Host "Warm-up runs per variant: $WarmupRunsPerVariant"
    if ($null -ne $Workers) {
        Write-Host "Workers: $Workers"
    }
    Write-Host "Analyzer timings: $($AnalyzerTimings.IsPresent)"
    Write-Host "Current runs: $($currentRows.Count)"
    Write-Host "Comparison runs: $($comparisonRows.Count)"
    Write-Host "Entry counts: $($entryCounts -join ', ')"
    Write-Host "Main cache byte-identical: $mainEqual"
    if ($null -eq $prettyEqual) {
        Write-Host "Pretty cache byte-identical: not compared"
        Write-Host "Pretty cache generated runs: $($prettyDigestRows.Count)"
    } else {
        Write-Host "Pretty cache byte-identical: $prettyEqual"
    }
    Write-Host ("Current elapsed mean seconds: {0}" -f (Format-OptionalSeconds -Value $currentMean))
    Write-Host ("Comparison elapsed mean seconds: {0}" -f (Format-OptionalSeconds -Value $comparisonMean))
    Write-Host ("Delta mean seconds (current - comparison): {0}" -f (Format-OptionalSeconds -Value $deltaSeconds))
    if ($null -eq $ratio) {
        Write-Host "Runtime ratio (current / comparison): n/a"
    } else {
        Write-Host ("Runtime ratio (current / comparison): {0:N4}x" -f $ratio)
    }
    if ($AnalyzerTimings.IsPresent) {
        Write-Host ("Current analyzer total mean seconds: {0}" -f (Format-OptionalSeconds -Value $currentAnalyzerMean))
        Write-Host ("Comparison analyzer total mean seconds: {0}" -f (Format-OptionalSeconds -Value $comparisonAnalyzerMean))
        Write-Host ("Current decode_ordered mean seconds: {0}" -f (Format-OptionalSeconds -Value $currentDecodeMean))
        Write-Host ("Comparison decode_ordered mean seconds: {0}" -f (Format-OptionalSeconds -Value $comparisonDecodeMean))
        Write-Host ("Current detailed_report mean seconds: {0}" -f (Format-OptionalSeconds -Value $currentDetailedMean))
        Write-Host ("Comparison detailed_report mean seconds: {0}" -f (Format-OptionalSeconds -Value $comparisonDetailedMean))
    }
    if (-not $mainEqual -or ($null -ne $prettyEqual -and -not $prettyEqual)) {
        $shouldKeepArtifacts = $true
        Write-Host "Artifacts kept for inspection: $tempRoot"
    } elseif ($KeepArtifacts) {
        $shouldKeepArtifacts = $true
        Write-Host "Artifacts kept by request: $tempRoot"
    }
    if ($shouldKeepArtifacts) {
        Write-Host "Run CSV: $csvPath"
        Write-Host "Summary JSON: $summaryPath"
    } else {
        Write-Host "Run CSV and summary JSON are temporary; pass -KeepArtifacts to keep them."
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
