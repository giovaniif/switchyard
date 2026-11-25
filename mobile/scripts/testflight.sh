#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
sdk_version=$(xcrun --sdk iphoneos --show-sdk-version)
if [ "${sdk_version%%.*}" -lt 27 ]; then
  echo "App Store Connect requires the iOS 27 SDK or newer from April 2027 (ITMS-90725); select Xcode 27+ with xcode-select." >&2
  exit 1
fi
: "${IOS_CERTIFICATE:?Set IOS_CERTIFICATE to a base64 Apple Distribution p12}"
: "${IOS_CERTIFICATE_PASSWORD:?Set IOS_CERTIFICATE_PASSWORD}"
: "${IOS_PROVISION_PROFILE:?Set IOS_PROVISION_PROFILE to a base64 App Store provisioning profile}"
: "${MACOS_TEAM_ID:?Set MACOS_TEAM_ID to the Apple Developer team}"
: "${APP_STORE_CONNECT_KEY_ID:?Set APP_STORE_CONNECT_KEY_ID}"
: "${APP_STORE_CONNECT_ISSUER_ID:?Set APP_STORE_CONNECT_ISSUER_ID}"
: "${APP_STORE_CONNECT_API_KEY:?Set APP_STORE_CONNECT_API_KEY to a base64 p8 key}"
build_number="${BUILD_NUMBER:-${GITHUB_RUN_NUMBER:-1}}"
mkdir -p build
signing_dir=$(mktemp -d)
keychain="$signing_dir/signing.keychain-db"
profile_path=""
cleanup() {
  security delete-keychain "$keychain" >/dev/null 2>&1 || true
  if [ -n "$profile_path" ]; then rm -f "$profile_path"; fi
  rm -rf "$signing_dir"
}
trap cleanup EXIT
printf '%s' "$IOS_CERTIFICATE" | base64 --decode > "$signing_dir/certificate.p12"
printf '%s' "$IOS_PROVISION_PROFILE" | base64 --decode > "$signing_dir/profile.mobileprovision"
printf '%s' "$APP_STORE_CONNECT_API_KEY" | base64 --decode > "$signing_dir/AuthKey.p8"
security cms -D -i "$signing_dir/profile.mobileprovision" > "$signing_dir/profile.plist"
profile_uuid=$(/usr/libexec/PlistBuddy -c 'Print UUID' "$signing_dir/profile.plist")
profile_name=$(/usr/libexec/PlistBuddy -c 'Print Name' "$signing_dir/profile.plist")
profile_path="$HOME/Library/MobileDevice/Provisioning Profiles/$profile_uuid.mobileprovision"
mkdir -p "$(dirname "$profile_path")"
cp "$signing_dir/profile.mobileprovision" "$profile_path"
keychain_password=$(openssl rand -hex 24)
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$signing_dir/certificate.p12" -P "$IOS_CERTIFICATE_PASSWORD" -A -t cert -f pkcs12 -k "$keychain"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" "$HOME/Library/Keychains/login.keychain-db"
# EAS may hold an older iPhone Distribution identity; both certificate types
# can sign App Store builds. Match the imported key, rather than its label.
signing_identity=$(security find-identity -v -p codesigning "$keychain" | python3 -c 'import re,sys; ids=re.findall(r"\b[0-9A-Fa-f]{40}\b",sys.stdin.read()); assert len(ids)==1, "Expected one valid iOS signing identity"; print(ids[0])')
bash scripts/prepare.sh
xcodebuild archive -project Switchyard.xcodeproj -scheme Switchyard -configuration Release \
  -destination 'generic/platform=iOS' -archivePath build/Switchyard.xcarchive -jobs 2 \
  CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY="$signing_identity" DEVELOPMENT_TEAM="$MACOS_TEAM_ID" \
  PROVISIONING_PROFILE_SPECIFIER="$profile_name" CURRENT_PROJECT_VERSION="$build_number"
export MACOS_TEAM_ID profile_name
python3 - <<'PY'
import os, plistlib
with open('build/ExportOptions.plist', 'wb') as f:
    plistlib.dump({
        'method': 'app-store-connect', 'destination': 'upload',
        'teamID': os.environ['MACOS_TEAM_ID'], 'signingStyle': 'manual',
        'provisioningProfiles': {'sh.giovani.switchyard.mobile': os.environ['profile_name']},
        'manageAppVersionAndBuildNumber': False, 'uploadSymbols': True,
    }, f)
PY
xcodebuild -exportArchive -archivePath build/Switchyard.xcarchive \
  -exportOptionsPlist build/ExportOptions.plist -exportPath build/export \
  -authenticationKeyPath "$signing_dir/AuthKey.p8" \
  -authenticationKeyID "$APP_STORE_CONNECT_KEY_ID" -authenticationKeyIssuerID "$APP_STORE_CONNECT_ISSUER_ID"
