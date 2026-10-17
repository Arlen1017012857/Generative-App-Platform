#!/usr/bin/env bash
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"
cargo build -p generative-app-desktop --locked
APP="$PROJECT_ROOT/dist/Generative App Platform.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/third-party"
cp target/debug/generative-app-desktop "$APP/Contents/MacOS/GenerativeAppPlatform"
cp third-party/*-LICENSE "$APP/Contents/Resources/third-party/"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.generative-app-platform.desktop</string>
<key>CFBundleName</key><string>Generative App Platform</string>
<key>CFBundleDisplayName</key><string>Generative App Platform</string>
<key>CFBundleExecutable</key><string>GenerativeAppPlatform</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
</dict></plist>
PLIST
echo "$APP"
