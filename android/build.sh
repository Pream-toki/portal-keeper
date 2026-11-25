#!/usr/bin/env bash
# Builds PortalKeeper.apk with aapt2 + javac + d8 + apksigner (no Gradle, no AndroidX).
set -euo pipefail
cd "$(dirname "$0")"

SDK="${ANDROID_SDK:-/c/android/sdk}"
BT="$SDK/build-tools/36.0.0"
PLATFORM="$SDK/platforms/android-35/android.jar"
JDK="${JDK_HOME:-/c/bot/tools/jdk-21.0.13+9}"
export PATH="$JDK/bin:$SDK/platform-tools:$PATH"

rm -rf build && mkdir -p build/classes build/dex build/assets
[ -f seed.properties ] && cp seed.properties build/assets/seed.properties

echo "== link resources =="
"$BT/aapt2.exe" link -o build/base.apk -I "$PLATFORM" --manifest AndroidManifest.xml \
    -A build/assets --min-sdk-version 24 --target-sdk-version 33

echo "== compile java =="
javac --release 11 -Xlint:-options -classpath "$PLATFORM" -d build/classes \
    $(find java -name '*.java')

echo "== dex =="
java -cp "$BT/lib/d8.jar" com.android.tools.r8.D8 --release --min-api 24 \
    --lib "$PLATFORM" --output build/dex $(find build/classes -name '*.class')

echo "== package =="
python - <<'PY'
import shutil, zipfile
shutil.copyfile('build/base.apk', 'build/unsigned.apk')
with zipfile.ZipFile('build/unsigned.apk', 'a', zipfile.ZIP_DEFLATED) as z:
    z.write('build/dex/classes.dex', 'classes.dex')
PY

"$BT/zipalign.exe" -f 4 build/unsigned.apk build/aligned.apk

if [ ! -f debug.keystore ]; then
    echo "== make key =="
    keytool -genkeypair -keystore debug.keystore -alias pk -storepass android \
        -keypass android -dname "CN=Portal Keeper" -keyalg RSA -keysize 2048 \
        -validity 10000 >/dev/null 2>&1
fi

echo "== sign =="
java -jar "$BT/lib/apksigner.jar" sign --ks debug.keystore --ks-pass pass:android \
    --key-pass pass:android --out PortalKeeper.apk build/aligned.apk
java -jar "$BT/lib/apksigner.jar" verify PortalKeeper.apk >/dev/null

ls -l PortalKeeper.apk | awk '{print "APK: "$5" bytes"}'
