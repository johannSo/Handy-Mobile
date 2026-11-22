# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
# The pairing scanner (tauri-plugin-barcode-scanner) uses ML Kit, which finds
# its components by class name and builds them by reflection. R8 can't see
# that and strips their constructors, after which no scan ever starts
# ("NoSuchMethodException: ...BarcodeRegistrar.<init>"). Debug builds aren't
# minified, so only a release shows it.
-keep class * implements com.google.firebase.components.ComponentRegistrar { *; }
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_barcode.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_common.** { *; }
-keep class com.google.android.gms.internal.mlkit_common.** { *; }
# The plugin's own classes are reached from Rust by name.
-keep class app.tauri.barcodescanner.** { *; }
# ML Kit's bundled barcode module is also found by name ("Local module
# descriptor class for com.google.mlkit.dynamite.barcode not found"), and the
# native decoder calls back into the barhopper classes by name.
-keep class com.google.android.gms.dynamite.descriptors.** { *; }
-keep class com.google.android.gms.dynamite.** { *; }
-keep class com.google.android.libraries.barhopper.** { *; }
-keep class com.google.barhopper.** { *; }
-keep class com.google.photos.vision.barhopper.** { *; }
-keepclasseswithmembernames class * { native <methods>; }
# The keyboard setup screen calls MainActivity's bridge from JavaScript, by
# name.
-keepclassmembers class * { @android.webkit.JavascriptInterface <methods>; }
-keepattributes *Annotation*
