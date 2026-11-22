# Runs the tpt-benchmark vs Criterion comparison (benches vs_criterion and vs_tpt).
#
# Builds both benches first (compile time is excluded), then runs each bench
# back-to-back for -Runs rounds. Each `cargo bench` invocation is wall-timed with
# Measure-Command. After each run the 95% CI of every workload's mean is read
# from the output files, then medians and spreads are printed and written to
# <TargetDir>/compare-results.json.
#
# Usage: pwsh -File scripts/compare.ps1 -TargetDir <absolute path> [-Runs 3]

param(
    [Parameter(Mandatory = $true)] [string] $TargetDir,
    [int] $Runs = 3
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$env:CARGO_TARGET_DIR = $TargetDir

$criterionNames = @('sum_u64_1024', 'dot_f64_128x128', 'format_string_100', 'fast_mul_u64')

Push-Location $repoRoot
try {
    cargo bench -p tpt-benchmark --no-run --bench vs_criterion --bench vs_tpt
    if ($LASTEXITCODE -ne 0) { throw "build failed" }

    $runsOut = @()
    for ($run = 1; $run -le $Runs; $run++) {
        foreach ($bench in @('vs_criterion', 'vs_tpt')) {
            $wall = Measure-Command {
                cargo bench -p tpt-benchmark --bench $bench 2>&1 | Out-Null
            }
            if ($LASTEXITCODE -ne 0) { throw "cargo bench $bench failed" }
            $wallSec = [math]::Round($wall.TotalSeconds, 3)

            if ($bench -eq 'vs_criterion') {
                foreach ($name in $criterionNames) {
                    $path = Join-Path $TargetDir "criterion/$name/new/estimates.json"
                    $ci = (Get-Content $path -Raw | ConvertFrom-Json).mean.confidence_interval
                    $runsOut += [pscustomobject]@{
                        run = $run; side = 'criterion'; workload = $name
                        ci_low = [double]$ci.lower_bound; ci_high = [double]$ci.upper_bound
                        mean = [double](Get-Content $path -Raw | ConvertFrom-Json).mean.point_estimate
                        stop_reason = ''; wall_s = $wallSec
                    }
                }
            } else {
                $doc = Get-Content (Join-Path $TargetDir 'tpt-benchmark/tpt-benchmark.json') -Raw | ConvertFrom-Json
                foreach ($b in $doc.benchmarks) {
                    $wl = ($b.name -split '::')[-1]
                    $layer = $b.layers | Where-Object { $_.layer -eq 'WallClock' } | Select-Object -First 1
                    $runsOut += [pscustomobject]@{
                        run = $run; side = 'tpt'; workload = $wl
                        ci_low = [double]$layer.ci_low; ci_high = [double]$layer.ci_high
                        mean = [double]$layer.mean
                        stop_reason = [string]$layer.stop_reason; wall_s = $wallSec
                    }
                }
            }
            Write-Host "run $run  $bench  wall ${wallSec}s"
        }
    }

    $summary = foreach ($name in $criterionNames) {
        $c = @($runsOut | Where-Object { $_.side -eq 'criterion' -and $_.workload -eq $name })
        $t = @($runsOut | Where-Object { $_.side -eq 'tpt' -and $_.workload -eq $name })
        $med = { param($xs) $s = $xs | Sort-Object; $s[[math]::Floor(($s.Count - 1) / 2)] }
        $cLo = & $med ($c.ci_low); $cHi = & $med ($c.ci_high)
        $tLo = & $med ($t.ci_low); $tHi = & $med ($t.ci_high)
        [pscustomobject]@{
            workload = $name
            criterion_ci_low_ns = $cLo; criterion_ci_high_ns = $cHi
            tpt_ci_low_ns = $tLo; tpt_ci_high_ns = $tHi
            overlap = ($cLo -le $tHi -and $tLo -le $cHi)
            criterion_mean_ns = & $med ($c.mean); tpt_mean_ns = & $med ($t.mean)
            criterion_wall_s_median = & $med ($c.wall_s | Select-Object -Unique)
            tpt_wall_s_median = & $med ($t.wall_s | Select-Object -Unique)
            tpt_stop_reasons = (($t.stop_reason | Sort-Object -Unique) -join ',')
            criterion_wall_s_all = (($c.wall_s | Select-Object -Unique) -join ',')
            tpt_wall_s_all = (($t.wall_s | Select-Object -Unique) -join ',')
        }
    }

    $summary | Format-Table -AutoSize
    $out = Join-Path $TargetDir 'compare-results.json'
    [pscustomobject]@{ runs = $runsOut; summary = @($summary) } |
        ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $out
    Write-Host "wrote $out"
}
finally {
    Pop-Location
}
