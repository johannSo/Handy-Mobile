# Handy Mobile

The phone half of [Handy Mobile](../README.md#handy-mobile): pair it with Handy on your computer, tap, talk, and the computer transcribes.

It is a separate Tauri 2 app. The Rust core (`src-tauri/src/lib.rs`) keeps the link to the desktop, using the `handy-link` crate shared with the desktop app. The WebView (`src/`) captures the microphone as 16 kHz mono PCM, hands each frame to the core, and draws the screens.

## The keyboard

On Android the app is also an input method, for dictating into other apps. It is native Kotlin, because Android runs a keyboard as a service with no WebView, often in a process where the app itself never started:

- `gen/android/.../HandyKeyboardService.kt` draws the keyboard, records with `AudioRecord`, and types the transcript into the focused field.
- `src-tauri/src/keyboard.rs` is its link to the desktop, with a runtime of its own, called over JNI through `KeyboardLink.kt`. It reads the pairing the app stored (`desktop.json`) and connects on the `keyboard` channel, so its connection and the app's don't replace each other. It is connected only while the keyboard is on screen.
- `src/screens/KeyboardSetup.tsx` is the setup walkthrough. `SetupBridge` in `MainActivity.kt` tells it which permissions are granted and whether the keyboard is turned on and selected, and opens the system screens for each.

A keyboard can't ask for permissions, so when the microphone isn't allowed or no computer is paired, tapping its microphone opens the app on the setup screen.

To try it on the emulator: `adb shell ime enable com.pais.handy.mobile/.HandyKeyboardService`, then `adb shell ime set com.pais.handy.mobile/.HandyKeyboardService`.

## Build (Android)

Prerequisites: [Rust](https://rustup.rs/), [Bun](https://bun.sh/), a JDK (17 or newer), and the Android SDK with an NDK.

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

export ANDROID_HOME="$HOME/Android/Sdk"
export NDK_HOME="$ANDROID_HOME/ndk/<version>"
export JAVA_HOME=/path/to/jdk

cd mobile
bun install

# Run on a connected device or emulator, with hot reload
bun run tauri android dev

# A debug APK for the emulator
bun run tauri android build --debug --target x86_64 --apk

# A release APK for phones
bun run tauri android build --target aarch64 --target armv7 --apk
```

APKs land in `src-tauri/gen/android/app/build/outputs/apk/`.

### Signing a release

Android only installs signed APKs. Create a keystore once and keep it: an update has to be signed with the same key.

```bash
keytool -genkeypair -v -keystore ~/handy-mobile-release.jks \
  -keyalg RSA -keysize 4096 -validity 10000 -alias handy-mobile
```

Then create `src-tauri/gen/android/keystore.properties` (git-ignored):

```properties
storeFile=/home/you/handy-mobile-release.jks
password=<the keystore password>
keyAlias=handy-mobile
```

Without that file a release build comes out unsigned.

## Testing without a phone

`handy-link` ships a phone in a terminal. Turn on Handy Mobile on the desktop, choose **Pair a Phone**, copy the code, then:

```bash
cd crates/handy-link
cargo run --features client --example link-cli -- pair 'handy://pair?...'
cargo run --features client --example link-cli -- send speech.wav phone
```

`speech.wav` must be 16 kHz mono 16-bit PCM.

## Notes

- On Android 17 and later, apps can't reach the local network until the user allows it, so the app asks for "Nearby devices" (`ACCESS_LOCAL_NETWORK`) at launch (`MainActivity.kt`). Without it every connection to the computer times out, even though the phone can ping it. The emulator doesn't show this unless the computer is addressed as `10.0.2.2`.
- The QR scanner uses ML Kit with the barcode model bundled in the app (`com.google.mlkit:barcode-scanning` in `gen/android/app/build.gradle.kts`), so it works offline and without Google Play services. Release builds are minified; the keep rules ML Kit needs are in `gen/android/app/proguard-rules.pro`. Without them a release build can't start the scanner while a debug build can.

- Recording only runs while the app is on screen: Android stops a WebView's microphone in the background, so leaving the app finishes the dictation.
- The pairing secret is kept in the app's private storage.
- Unpairing on the phone forgets the computer. The computer keeps listing the phone until it is unpaired there too.
