#!/bin/sh
# Checks that an APK is fit to attach to a release: signed with a release
# key (not the Android debug key), with the given version name and a version
# code. The release workflow runs it on the APK it built.
#
# Usage: check-apk.sh <apk> <version>
# Needs apksigner and aapt2 from the Android build-tools: on PATH, or found
# in the newest $ANDROID_HOME/build-tools.
set -eu

apk=$1
version=$2

tool() {
  if command -v "$1" >/dev/null 2>&1; then
    command -v "$1"
  else
    ls -d "$ANDROID_HOME"/build-tools/*/"$1" | sort -V | tail -n 1
  fi
}
apksigner=$(tool apksigner)
aapt2=$(tool aapt2)

fail() {
  echo "check-apk: $*" >&2
  exit 1
}

certs=$("$apksigner" verify --print-certs "$apk" 2>&1) ||
  fail "$apk is not signed: $certs"
echo "$certs" | grep -q 'CN=Android Debug' &&
  fail "$apk is signed with the Android debug key"

badging=$("$aapt2" dump badging "$apk")
version_name=$(echo "$badging" | sed -n "s/.*versionName='\([^']*\)'.*/\1/p")
version_code=$(echo "$badging" | sed -n "s/.*versionCode='\([^']*\)'.*/\1/p")
[ "$version_name" = "$version" ] ||
  fail "$apk has version name '$version_name', expected '$version'"
[ -n "$version_code" ] || fail "$apk has no version code"

echo "$apk: release-signed, version $version_name (code $version_code)"
