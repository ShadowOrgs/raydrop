import os
import sys

manifest_path = "src-tauri/gen/android/app/src/main/AndroidManifest.xml"
if not os.path.exists(manifest_path):
    print("AndroidManifest.xml not found, skipping patch.")
    sys.exit(0)

with open(manifest_path, "r", encoding="utf-8") as f:
    content = f.read()

permissions = """
    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
    <uses-permission android:name="android.permission.ACCESS_WIFI_STATE" />
    <uses-permission android:name="android.permission.CHANGE_WIFI_MULTICAST_STATE" />
    <uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE" android:maxSdkVersion="32" />
    <uses-permission android:name="android.permission.WRITE_EXTERNAL_STORAGE" android:maxSdkVersion="29" />
    <uses-permission android:name="android.permission.READ_MEDIA_AUDIO" />
    <uses-permission android:name="android.permission.READ_MEDIA_VIDEO" />
    <uses-permission android:name="android.permission.READ_MEDIA_IMAGES" />
"""

if "CHANGE_WIFI_MULTICAST_STATE" not in content:
    content = content.replace("<application", permissions + "\n    <application")
if 'android:testOnly="true"' in content:
    content = content.replace('android:testOnly="true"', 'android:testOnly="false"')
with open(manifest_path, "w", encoding="utf-8") as f:
    f.write(content)
print("AndroidManifest.xml successfully patched!")
