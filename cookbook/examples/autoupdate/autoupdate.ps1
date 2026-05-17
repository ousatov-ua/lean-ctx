# lean-ctx auto-updater — Windows (PowerShell)
# Checks GitHub API first; only calls `lean-ctx update` when a newer version exists.
#
# Install (run once as Admin):
#   $s = "$env:USERPROFILE\.lean-ctx\autoupdate.ps1"
#   $a = New-ScheduledTaskAction -Execute "pwsh" -Argument "-NonInteractive -WindowStyle Hidden -File `"$s`""
#   $t = New-ScheduledTaskTrigger -RepetitionInterval (New-TimeSpan -Hours 6) -Once -At (Get-Date)
#   Register-ScheduledTask -TaskName "lean-ctx autoupdate" -Action $a -Trigger $t -RunLevel Highest -Force

$lc = (Get-Command lean-ctx -ErrorAction SilentlyContinue)?.Source
if (-not $lc) { Write-Error "lean-ctx not in PATH"; exit 1 }

$log = "$env:USERPROFILE\.lean-ctx\autoupdate.log"
function Log($msg) { "$(Get-Date -f 'yyyy-MM-dd HH:mm:ss') $msg" | Add-Content $log }
function Notify($msg) {
    Add-Type -AssemblyName System.Windows.Forms
    $n = [System.Windows.Forms.NotifyIcon]::new()
    $n.Icon = [System.Drawing.SystemIcons]::Information
    $n.Visible = $true
    $n.ShowBalloonTip(5000, "lean-ctx", $msg, [System.Windows.Forms.ToolTipIcon]::Info)
    Start-Sleep 3; $n.Dispose()
}
function GetVersion { (& $lc status --json | ConvertFrom-Json).version }

$current = GetVersion
$latest  = (Invoke-RestMethod "https://api.github.com/repos/yvgude/lean-ctx/releases/latest").tag_name.TrimStart('v')

if (-not $current -or -not $latest) { Log "WARN: version check failed"; exit 0 }
Log "current=v$current latest=v$latest"
if ($current -eq $latest) { exit 0 }

Log "Updating v$current → v$latest"
& $lc update 2>&1 | Add-Content $log

$new = GetVersion
Notify "v$current → v$new · Restart IDE to reconnect MCP"
Log "Done: v$new"
