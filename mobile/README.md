# Matpris for Android and iPhone

A [Capacitor](https://capacitorjs.com) app that opens the Matpris server in a native
window: the same pages as the web and desktop apps (search, map, "Cheapest near you",
share), with the phone's location. Phones can't run the server (it searches the chains
and keeps ICA's session with headless Chrome), so the apps need one on the internet,
over HTTPS: see "Hosting the server" in the repo README.

Everything below runs from this folder (`mobile/`).

```bash
bun install
MATPRIS_SERVER_URL=https://matpris.example.se bun run sync   # writes the address into both apps
```

Without `MATPRIS_SERVER_URL` the app opens a page saying it has no server. If the server
can't be reached, the app shows `www/offline.html` with a Try again button.

## Android

```bash
MATPRIS_SERVER_URL=https://… bun run android   # → android/app/build/outputs/apk/debug/app-debug.apk
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

Needs JDK 21 and the Android SDK (`ANDROID_HOME`, or `sdk.dir` in `android/local.properties`).
The Release workflow builds it as `Matpris-<version>-android.apk`. To install a release over an
earlier one, every release has to be signed with the same key: generate it once and save it as
the repo secret `ANDROID_DEBUG_KEYSTORE`:

```bash
keytool -genkeypair -keystore matpris.keystore -storepass android -alias androiddebugkey \
  -keypass android -dname "CN=Matpris" -keyalg RSA -validity 10000
base64 -i matpris.keystore | pbcopy   # paste as the secret's value; keep the file safe
```

For a dev server on your computer, use the emulator's address for it:
`MATPRIS_SERVER_URL=http://10.0.2.2:5173 bun run sync` (plain http is allowed only then).

## iPhone

Needs a Mac with Xcode 26 and an Apple developer team (the MapKit team works).

```bash
MATPRIS_SERVER_URL=https://… bun run ios   # syncs and opens ios/App in Xcode
```

In Xcode pick the **App** target → Signing & Capabilities → your team, plug in the iPhone and
press Run. Without Xcode, open the server's page in Safari and choose
Share → **Add to Home Screen**: the site has a web app manifest and icon, so it opens full
screen like an app.

## Notes

- Product links open in the system browser; only the server's pages load in the app.
- `appId` is `se.matpris.app`, shared with the Wear OS app (`../wear`).
- The version is the repo's `package.json` (Android reads it in `android/app/build.gradle`).
- Icons and launch screens are drawn from `desktop/icons/matpris*.svg`.
