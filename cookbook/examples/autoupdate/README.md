# autoupdate

Background auto-updater for lean-ctx. Checks GitHub for a new release every 6 hours and runs `lean-ctx update` only when one is found — avoiding unnecessary daemon restarts.

| Platform | Script | Scheduler |
|---|---|---|
| macOS | `autoupdate.sh` | LaunchAgent (every 6 h) |
| Linux | `autoupdate.sh` | cron `0 */6 * * *` |
| Windows | `autoupdate.ps1` | Task Scheduler (every 6 h) |

## macOS install

```bash
cp autoupdate.sh ~/.lean-ctx/autoupdate.sh
chmod +x ~/.lean-ctx/autoupdate.sh

# generate plist with your actual paths and register it
SCRIPT="$HOME/.lean-ctx/autoupdate.sh"
PLIST="$HOME/Library/LaunchAgents/com.leactx.autoupdate.plist"
cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.leactx.autoupdate</string>
  <key>ProgramArguments</key>
  <array><string>/bin/bash</string><string>$SCRIPT</string></array>
  <key>StartInterval</key><integer>21600</integer>
  <key>RunAtLoad</key><false/>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin</string>
    <key>HOME</key><string>$HOME</string>
  </dict>
  <key>StandardOutPath</key><string>$HOME/.lean-ctx/autoupdate-stdout.log</string>
  <key>StandardErrorPath</key><string>$HOME/.lean-ctx/autoupdate-stderr.log</string>
</dict></plist>
EOF
launchctl load "$PLIST"
```

## Linux install

```bash
cp autoupdate.sh ~/.lean-ctx/autoupdate.sh
chmod +x ~/.lean-ctx/autoupdate.sh
(crontab -l 2>/dev/null; echo "0 */6 * * * bash $HOME/.lean-ctx/autoupdate.sh") | crontab -
```

## Windows install

Copy `autoupdate.ps1` to `$env:USERPROFILE\.lean-ctx\autoupdate.ps1`, then run the install block at the top of the file in an elevated PowerShell session.

## Logs

`~/.lean-ctx/autoupdate.log` (auto-rotated at 500 lines)
