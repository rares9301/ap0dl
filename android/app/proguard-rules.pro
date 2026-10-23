# Keep the JNI entry points and the Kotlin singleton that declares them.
-keep class com.ap0dl.anyomi.MobileEngine { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}
# kotlinx.serialization generated serializers are referenced reflectively.
-keepattributes *Annotation*, InnerClasses
-keep,includedescriptorclasses class com.ap0dl.anyomi.data.**$$serializer { *; }
-keepclassmembers class com.ap0dl.anyomi.data.** {
    *** Companion;
}
-keepclasseswithmembers class com.ap0dl.anyomi.data.** {
    kotlinx.serialization.KSerializer serializer(...);
}