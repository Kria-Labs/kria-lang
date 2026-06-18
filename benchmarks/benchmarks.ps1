# Kria Language Benchmark Suite (PowerShell)
# Multi-run timing with warmup (milliseconds).

param(
    [int]$Warmup = 3,
    [int]$Runs = 10
)

$ErrorActionPreference = "Stop"

# Paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$KriaBinary = Join-Path $ProjectRoot "target\release\kria.exe"
$ResultsFile = Join-Path $ScriptDir "benchmark_results.txt"

# Colors
$Red = [System.ConsoleColor]::Red
$Green = [System.ConsoleColor]::Green
$Yellow = [System.ConsoleColor]::Yellow
$Blue = [System.ConsoleColor]::Blue

# Build release binary if not exists
if (-not (Test-Path $KriaBinary)) {
    Write-Host "Building release binary..." -ForegroundColor $Yellow
    Push-Location $ProjectRoot
    cargo build --release
    Pop-Location
}

if (-not (Test-Path $KriaBinary)) {
    Write-Host "Error: $KriaBinary not found" -ForegroundColor $Red
    exit 1
}

# Run kria once and measure time in milliseconds
function Run-KriaOnceMs {
    param([string]$BenchFile)
    
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    & $KriaBinary $BenchFile | Out-Null
    $stopwatch.Stop()
    
    return [math]::Round($stopwatch.Elapsed.TotalMilliseconds, 2)
}

# Run kria and capture output
function Run-KriaCapture {
    param([string]$BenchFile)
    
    $output = & $KriaBinary $BenchFile 2>&1
    return @{
        Output = $output -join "`n"
        ExitCode = $LASTEXITCODE
    }
}

# Compute statistics
function Compute-Stats {
    param([double[]]$Values)
    
    if ($Values.Count -eq 0) {
        return @{
            Median = 0
            Min = 0
            Max = 0
            Mean = 0
        }
    }
    
    $sorted = $Values | Sort-Object
    $n = $sorted.Count
    $sum = ($sorted | Measure-Object -Sum).Sum
    
    $median = if ($n % 2 -eq 1) {
        $sorted[($n - 1) / 2]
    } else {
        ($sorted[$n / 2 - 1] + $sorted[$n / 2]) / 2
    }
    
    return @{
        Median = [math]::Round($median, 2)
        Min = [math]::Round($sorted[0], 2)
        Max = [math]::Round($sorted[-1], 2)
        Mean = [math]::Round($sum / $n, 2)
    }
}

# Format stats with ms suffix
function Format-StatsMs {
    param($Stats)
    return "median=$($Stats.Median)ms min=$($Stats.Min)ms max=$($Stats.Max)ms mean=$($Stats.Mean)ms"
}

# Run single benchmark with warmup and multiple runs
function Run-Benchmark {
    param([string]$BenchFile)
    
    # Warmup runs
    for ($w = 0; $w -lt $Warmup; $w++) {
        Run-KriaOnceMs $BenchFile | Out-Null
    }
    
    # Actual runs
    $samples = @()
    for ($r = 0; $r -lt $Runs; $r++) {
        $ms = Run-KriaOnceMs $BenchFile
        $samples += $ms
    }
    
    return Compute-Stats $samples
}

# Print results table
function Print-ResultsTable {
    param(
        [string[]]$Names,
        [object[]]$StatsArray,
        [string[]]$Outputs
    )
    
    $header1 = "{0,-28} {1,12} {2,12} {3,12} {4,12} {5}" -f "Test", "Median", "Min", "Max", "Mean", "Output"
    $header2 = "{0,-28} {1,12} {2,12} {3,12} {4,12} {5}" -f "------------------------------", "-----", "-----", "-----", "-----", "-----"
    
    Write-Host $header1
    Write-Host $header2
    
    $totalMedian = 0
    $validCount = 0
    
    for ($i = 0; $i -lt $Names.Count; $i++) {
        $name = $Names[$i]
        $stats = $StatsArray[$i]
        
        if ($stats -eq "ERROR") {
            Write-Host ("{0,-28} {1,12}" -f $name, "ERROR")
            continue
        }
        
        Write-Host ("{0,-28} {1,11}ms {2,11}ms {3,11}ms {4,11}ms {5}" -f $name, $stats.Median, $stats.Min, $stats.Max, $stats.Mean, $Outputs[$i])
        $totalMedian += $stats.Median
        $validCount++
    }
    
    Write-Host ""
    Write-Host ("{0,-28} {1,12}" -f "Tests run", $Names.Count)
    
    if ($validCount -gt 0) {
        $avgMedian = [math]::Round($totalMedian / $validCount, 2)
        Write-Host ("{0,-28} {1,11}ms" -f "Sum of medians", $totalMedian)
        Write-Host ("{0,-28} {1,11}ms" -f "Avg median per test", $avgMedian)
    }
}

# Write header to results file
function Write-Header {
    $rustcVersion = try { rustc -V } catch { "n/a" }
    $cargoVersion = try { cargo -V } catch { "n/a" }
    
    $header = @"
Kria Benchmark Results
======================
date: $(Get-Date -Format "o")
rustc: $rustcVersion
cargo: $cargoVersion
system: $([System.Environment]::OSVersion.VersionString)
binary: $KriaBinary
warmup: $Warmup
runs: $Runs
timing_backend: .NET Stopwatch (wall clock, milliseconds)

Format: name | median=..ms min=..ms max=..ms mean=..ms | exit=.. | output=..
All timing values are in milliseconds (ms).

"@
    $header | Out-File -FilePath $ResultsFile -Encoding UTF8
}

# Main
Write-Host ""
Write-Host "========================================" -ForegroundColor $Blue
Write-Host "    Kria Language Benchmark Suite" -ForegroundColor $Blue
Write-Host "========================================" -ForegroundColor $Blue
Write-Host ""
Write-Host "Timing: .NET Stopwatch (warmup=$Warmup, runs=$Runs)" -ForegroundColor $Green
Write-Host ""

Write-Header

$benchmarks = @()
$benchNames = @()
$benchStats = @()
$benchOutputs = @()
$benchExits = @()

$benchFiles = @(Get-ChildItem -Path $ScriptDir -Name "bench_*.krx" -File | Sort-Object)

foreach ($benchFile in $benchFiles) {
    $fullPath = Join-Path $ScriptDir $benchFile
    $benchName = [System.IO.Path]::GetFileNameWithoutExtension($benchFile)
    
    Write-Host -NoNewline "Running $benchName... "
    
    # Capture output
    $result = Run-KriaCapture $fullPath
    $output = $result.Output
    $exitCode = $result.ExitCode
    
    if ($exitCode -ne 0) {
        Write-Host "FAILED" -ForegroundColor $Red -NoNewline
        Write-Host " (exit $exitCode)"
        
        $benchNames += $benchName
        $benchStats += "ERROR"
        $benchOutputs += $output
        $benchExits += $exitCode
        
        "$benchName | ERROR exit=$exitCode | $output" | Add-Content -Path $ResultsFile
        continue
    }
    
    # Run benchmark
    $stats = Run-Benchmark $fullPath
    $statsMs = Format-StatsMs $stats
    
    $benchNames += $benchName
    $benchStats += $stats
    $benchOutputs += $output
    $benchExits += 0
    
    Write-Host "OK" -ForegroundColor $Green -NoNewline
    Write-Host " $statsMs (output: $output)"
    "$benchName | $statsMs | exit=0 | output=$output" | Add-Content -Path $ResultsFile
}

# Write summary to file and console
Write-Host ""
Write-Host "========================================" -ForegroundColor $Blue
Write-Host "         Kria Benchmark Results (ms)" -ForegroundColor $Blue
Write-Host "========================================" -ForegroundColor $Blue
Write-Host ""

Print-ResultsTable $benchNames $benchStats $benchOutputs

# Append summary to file
@"

Summary (ms)
============
"@ | Add-Content -Path $ResultsFile

for ($i = 0; $i -lt $benchNames.Count; $i++) {
    if ($benchStats[$i] -eq "ERROR") {
        "$($benchNames[$i]),ERROR,ERROR,ERROR,ERROR," | Add-Content -Path $ResultsFile
    } else {
        "$($benchNames[$i]),$($benchStats[$i].Median),$($benchStats[$i].Min),$($benchStats[$i].Max),$($benchStats[$i].Mean),$($benchOutputs[$i])" | Add-Content -Path $ResultsFile
    }
}

Write-Host ""
Write-Host "========================================" -ForegroundColor $Blue
Write-Host ""
Write-Host "Results saved to: $ResultsFile" -ForegroundColor $Green
