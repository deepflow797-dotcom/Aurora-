# The JNI symbols are resolved by name at run time, so they must survive R8.
-keepclasseswithmembernames class app.aurora.player.NativePlayer {
    native <methods>;
}
-keep class app.aurora.player.NativePlayer { *; }
