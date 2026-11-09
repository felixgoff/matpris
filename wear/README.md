# Matpris for Galaxy Watch 5 (Wear OS)

A native Wear OS app (Kotlin, Compose for Wear OS) for Wear OS 3 and later, so the
Galaxy Watch 4, 5, 6, 7 and other Wear OS watches. Say or type a product and the watch
shows what's cheapest near you, with the next cheapest below it. It ranks results like
the web page does by default (comparison price, multi-buy deals counted, only products
that are what you searched for: `app/src/main/java/se/matpris/wear/Ranking.kt`).

- **Search**: the Search button opens the watch's own input (voice or keyboard). The last
  six searches are kept as buttons, starting with mjölk, kaffe, bananer, ägg and smör.
- **Location**: the watch's GPS, or the phone's location through it. Without permission or
  a fix within 15 s it searches around Lund and says so.
- **Data**: the watch asks the Matpris app on the paired **Android phone** over the Wear OS
  Data Layer (Bluetooth or Wi-Fi). The phone runs the search in its Rust backend and sends
  each chain's results back as they arrive, so the first chains show while the others
  answer. The phone app doesn't have to be open (a `WearableListenerService` wakes it). An
  iPhone can't do this: Wear OS watches pair with Android phones.

### The protocol

Both sides must stay in step: `app/src/main/java/se/matpris/wear/Api.kt` here, and
`WearSearchService.kt` / `WatchBridge.kt` in `src-tauri/gen/android` with
`src-tauri/src/watch.rs` on the phone.

| Step | Path | Payload |
| --- | --- | --- |
| The phone app declares the capability | `matpris_search` | `res/values/wear.xml` |
| Watch → phone | `/matpris/search` | `{"id","q","lat","lng"}` (UTF-8 JSON) |
| Phone → watch, once per event | `/matpris/event/<id>` | one search event as JSON, trimmed to the fields the watch reads, ending with `{"type":"done"}` or `{"type":"error","message":…}` |

The Data Layer only connects apps with the **same application id** (`se.matpris.matpris`)
and the **same signing key**: build both with the same keystore (the Release workflow does,
from the `ANDROID_KEYSTORE` secrets). When the phone app isn't running, ICA has no session
cookie in that process and shows as busy on the watch.

## Build

From this folder (`wear/`), with JDK 21 and the Android SDK:

```bash
./gradlew assembleDebug           # → app/build/outputs/apk/debug/app-debug.apk
./gradlew testDebugUnitTest      # ranking and parsing tests
./gradlew recordRoborazziDebug   # screenshots of the screens → app/build/outputs/roborazzi
```

The Release workflow builds it as `Matpris-<version>-wear-os.apk`, signed with the same key as
the Tauri phone app (secrets `ANDROID_KEYSTORE` (base64), `ANDROID_KEYSTORE_PASSWORD`,
`ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`): the Data Layer only connects the watch to a
phone app with the same id and signature. To sign a local release build the same way, set
`ANDROID_KEYSTORE_FILE`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and
`ANDROID_KEY_PASSWORD`, or put `storeFile`, `storePassword`, `keyAlias` and `keyPassword` in
`wear/keystore.properties` (gitignored); without them `assembleRelease` uses the debug key.

## Install on the watch

The Galaxy Watch installs apps that aren't from the Play Store over Wi-Fi debugging:

1. On the watch: Settings → About watch → Software information → tap **Software version**
   five times. Developer options appear in Settings.
2. Settings → Developer options: turn on **ADB debugging** and **Debug over Wi-Fi**. The watch
   and the computer must be on the same Wi-Fi network.
3. Developer options → Wireless debugging → **Pair new device**, then on the computer:
   ```bash
   adb pair <ip>:<pairing port>      # enter the code the watch shows
   adb connect <ip>:<port>           # the port under "Wireless debugging"
   adb install -r Matpris-<version>-wear-os.apk
   ```
4. Open Matpris from the watch's app list and allow location.
