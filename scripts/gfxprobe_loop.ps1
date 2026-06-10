# gfxprobe_loop.ps1 -- measure the incidence rate of issue #40 (random
# dead-surface launch). Launches the given executable N times with
# RCCE_GFXPROBE=exit: each run performs the boot render-sanity probe
# (Modules\Graphics\RenderSanity.bb), writes PASS / RECOVERED n / FAIL to
# gfxprobe_result.txt and exits immediately. The tally turns "random,
# hard to replicate" into a number.
#
# Usage (from the repo root):
#   powershell -ExecutionPolicy Bypass -File scripts\gfxprobe_loop.ps1 -Exe bin\Client.exe -Runs 100
#
# To test the ReShade hypothesis on issue #40, run once as-is and once
# with bin\dxgi.dll temporarily renamed (which disables ReShade), then
# compare the FAIL/RECOVERED counts.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [int]$Runs = 100,
    [int]$TimeoutSec = 60
)

$exePath = Resolve-Path $Exe -ErrorAction Stop
$workDir = Split-Path $exePath
# The probe writes its result anchored to the EXECUTABLE's directory
# (SystemProperty("AppDir")). Also check the parent in case an older binary
# (pre-fix: CWD-relative write + ChangeDir to the project root) is being
# measured -- a silent location mismatch would tally everything as
# NO-RESULT and masquerade as a 100% crash rate.
$resultCandidates = @(
    (Join-Path $workDir "gfxprobe_result.txt"),
    (Join-Path (Split-Path $workDir) "gfxprobe_result.txt")
)

$tally = @{ "PASS" = 0; "RECOVERED" = 0; "FAIL" = 0; "NO-RESULT" = 0 }

for ($i = 1; $i -le $Runs; $i++) {
    foreach ($rf in $resultCandidates) {
        if (Test-Path $rf) { Remove-Item $rf -Force }
    }
    $env:RCCE_GFXPROBE = "exit"
    $p = Start-Process -FilePath $exePath -WorkingDirectory $workDir -WindowStyle Minimized -PassThru
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        $p.Kill()
        Write-Output ("run {0}: TIMEOUT (killed)" -f $i)
        $tally["NO-RESULT"]++
        continue
    }
    $resultFile = $resultCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($resultFile) {
        $line = (Get-Content $resultFile -TotalCount 1).Trim()
        $key = ($line -split ' ')[0]
        if (-not $tally.ContainsKey($key)) { $key = "NO-RESULT" }
        $tally[$key]++
        if ($key -ne "PASS") { Write-Output ("run {0}: {1}" -f $i, $line) }
    } else {
        $tally["NO-RESULT"]++
        Write-Output ("run {0}: exited without writing a result" -f $i)
    }
}
Remove-Item Env:RCCE_GFXPROBE -ErrorAction SilentlyContinue

Write-Output ""
Write-Output ("=== {0} runs of {1} ===" -f $Runs, (Split-Path $exePath -Leaf))
Write-Output ("PASS      : {0}" -f $tally["PASS"])
Write-Output ("RECOVERED : {0}  (probe failed, re-init cured it)" -f $tally["RECOVERED"])
Write-Output ("FAIL      : {0}  (dead surfaces survived re-init)" -f $tally["FAIL"])
Write-Output ("NO-RESULT : {0}  (crash/timeout before the probe)" -f $tally["NO-RESULT"])
