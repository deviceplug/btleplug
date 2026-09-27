#
# Run each integration test individually to avoid multiple simultaneous
# BLE connections to the same test peripheral.
#
# Each test_*.rs file under tests/ is its own binary with a single test,
# ensuring process isolation for BLE stack stability.
#
# Usage:
#   .\scripts\run-integration-tests.ps1                    # run all tests
#   .\scripts\run-integration-tests.ps1 test_read_*        # run tests matching a glob
#
# Environment:
#   BTLEPLUG_TEST_PERIPHERAL  - peripheral name (default: btleplug-test)
#   RUST_LOG                  - log level (e.g. debug, btleplug=trace)
#   DELAY                     - seconds to wait between tests (default: 2)
#   TIMEOUT                   - seconds before a test is killed (default: 40)
#
# This script only runs on Windows, so it also always runs the ignored
# winrtble::adapter radio tests from `src/winrtble/adapter.rs` (#476), which
# need a real Windows Bluetooth radio and are otherwise never exercised by
# this script.

[CmdletBinding()]
param(
    [Parameter(Position = 0, ValueFromRemainingArguments)]
    [string[]]$Filter
)

$ErrorActionPreference = 'Stop'
# Native command exit codes are checked explicitly; never let PS 7.3+ turn them into exceptions.
$PSNativeCommandUseErrorActionPreference = $false

$Delay   = if ($env:DELAY)   { [int]$env:DELAY }   else { 2 }
$Timeout = if ($env:TIMEOUT) { [int]$env:TIMEOUT } else { 40 }
$Passed  = 0
$Failed  = 0
$Failures = @()

# Discover test files.
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$TestsDir  = Join-Path (Split-Path -Parent $ScriptDir) 'tests'

$TestNames = Get-ChildItem -Path $TestsDir -Filter 'test_*.rs' -Name |
    ForEach-Object { $_ -replace '\.rs$', '' } |
    Sort-Object

# Sentinel test name for the Windows-only winrtble radio lib tests, run
# alongside the per-file integration tests below. This script only runs on
# Windows, so always add it, subject to the same -like filter as the
# file-based tests (e.g. a filter of `test_read_*` excludes it, and a filter
# of `winrtble*` selects only it).
$WinrtbleRadioTests = 'winrtble_radio_tests'
$TestNames = @($TestNames) + $WinrtbleRadioTests

# Apply filter if provided.
if ($Filter) {
    $TestNames = $TestNames | Where-Object {
        $name = $_
        ($Filter | Where-Object { $name -like $_ }).Count -gt 0
    }
}

if ($TestNames.Count -eq 0) {
    Write-Host "No tests matched."
    if ($Filter) { Write-Host "Filter: $($Filter -join ', ')" }
    exit 1
}

$Total = $TestNames.Count
Write-Host "=== btleplug integration tests ==="
Write-Host "Running $Total tests sequentially (${Delay}s delay, ${Timeout}s timeout per test)"
Write-Host ""

# Build all needed test binaries once, outside the timed loop, so compile time
# doesn't count against each test's TIMEOUT. Output streams to the console; the
# child scope's 'Continue' keeps hosts that capture native stderr (ISE,
# remoting, a redirected caller) from turning cargo warnings into errors.
Write-Host "Building test binaries..."
$PrebuildArgs = @('test', '--quiet', '--no-run')
$RunLibTests = $false
foreach ($testName in $TestNames) {
    if ($testName -eq $WinrtbleRadioTests) {
        $RunLibTests = $true
    } else {
        $PrebuildArgs += @('--test', $testName)
    }
}
if ($RunLibTests) { $PrebuildArgs += '--lib' }

& {
    $ErrorActionPreference = 'Continue'
    & cargo @PrebuildArgs
}
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$LogFile = Join-Path $env:TEMP 'btleplug-test-output.log'
$TestNum = 0

foreach ($testName in $TestNames) {
    $TestNum++
    Write-Host -NoNewline ("[{0,2}/{1,2}] {2,-55} " -f $TestNum, $Total, $testName)

    if ($testName -eq $WinrtbleRadioTests) {
        # Filter to the winrtble::adapter::cleanup_tests module path so this
        # only picks up the #476 radio tests, not any other #[ignore]d lib
        # test that might exist. Serialised (--test-threads=1) since these
        # tests share a single physical Bluetooth radio.
        $CargoArgsStr = 'test --lib winrtble::adapter::cleanup_tests:: -- --ignored --test-threads=1'
    } else {
        $CargoArgsStr = "test --test $testName -- --ignored"
    }

    # Run cargo test with a timeout.
    # Use [System.Diagnostics.Process] directly: Start-Process -PassThru opens
    # the handle without PROCESS_QUERY_INFORMATION, making ExitCode unavailable.
    $psi = [System.Diagnostics.ProcessStartInfo]@{
        FileName               = 'cargo'
        Arguments              = $CargoArgsStr
        UseShellExecute        = $false
        RedirectStandardOutput = $true
        RedirectStandardError  = $true
        WorkingDirectory       = (Get-Location).Path
    }
    $proc = [System.Diagnostics.Process]::new()
    $proc.StartInfo = $psi
    $proc.Start() | Out-Null

    # Drain both pipes concurrently to prevent deadlock if buffers fill.
    $stdoutTask = $proc.StandardOutput.ReadToEndAsync()
    $stderrTask = $proc.StandardError.ReadToEndAsync()

    $finished = $proc.WaitForExit($Timeout * 1000)
    $killFailed = $false
    if (-not $finished) {
        # Process.Kill($true) is .NET Core 3.0+ only, so it throws on Windows
        # PowerShell 5.1. taskkill /T kills the whole tree on both editions,
        # including the test binary that holds the radio (a child of cargo).
        & {
            $ErrorActionPreference = 'Continue'
            & taskkill.exe /T /F /PID $proc.Id *> $null
        }
        $killFailed = -not $proc.WaitForExit(5000)
        # A reader can outlive the kill if an orphaned grandchild still holds
        # the pipe, so bound this wait too.
        [void][System.Threading.Tasks.Task]::WaitAll(@($stdoutTask, $stderrTask), 5000)
    } else {
        # No-arg WaitForExit ensures the process and its readers are fully done.
        $proc.WaitForExit()
        [System.Threading.Tasks.Task]::WaitAll($stdoutTask, $stderrTask)
    }

    # Only read a task's Result once it has actually completed -- the
    # getter blocks until completion otherwise, which would undo the
    # bounded waits above for a reader that never finished.
    $stdoutText = if ($stdoutTask.IsCompleted) { $stdoutTask.Result } else { '' }
    $stderrText = if ($stderrTask.IsCompleted) { $stderrTask.Result } else { '' }

    # Persist captured output for potential display below.
    [System.IO.File]::WriteAllText("$LogFile.stdout", $stdoutText)
    [System.IO.File]::WriteAllText($LogFile, $stderrText)

    $showOutput = $false
    if (-not $finished) {
        Write-Host "TIMEOUT (${Timeout}s)"
        if ($killFailed) {
            Write-Host "  WARNING: process $($proc.Id) still alive after taskkill" -ForegroundColor Yellow
        }
        $Failed++
        $Failures += $testName
        $showOutput = $true
    } elseif ($proc.ExitCode -ne 0) {
        Write-Host "FAIL"
        $Failed++
        $Failures += $testName
        $showOutput = $true
    } elseif ($testName -eq $WinrtbleRadioTests -and $stdoutText -match 'running 0 tests') {
        # Even on a zero exit code, cargo reports "running 0 tests" if the
        # module filter matched nothing -- don't let that pass silently.
        Write-Host "FAIL (0 tests matched)"
        $Failed++
        $Failures += $testName
        $showOutput = $true
    } else {
        Write-Host "PASS"
        $Passed++
    }

    if ($showOutput) {
        Write-Host "  --- output ---"
        if (Test-Path "$LogFile.stdout") {
            Get-Content "$LogFile.stdout" -Tail 20 | ForEach-Object { "  $_" }
        }
        if (Test-Path $LogFile) {
            Get-Content $LogFile -Tail 20 | ForEach-Object { "  $_" }
        }
        Write-Host "  --- end ---"
    }

    # Brief delay to let the BLE stack settle between tests.
    if ($TestNum -lt $Total) {
        Start-Sleep -Seconds $Delay
    }
}

Remove-Item -Path $LogFile, "$LogFile.stdout" -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "=== Results ==="
Write-Host "  Passed:  $Passed"
Write-Host "  Failed:  $Failed"
Write-Host "  Total:   $Total"

if ($Failures.Count -gt 0) {
    Write-Host ""
    Write-Host "Failed tests:"
    foreach ($f in $Failures) {
        Write-Host "  - $f"
    }
    exit 1
}

Write-Host ""
Write-Host "All tests passed."
