fn main() {
    // Pulls the Kotlin library in `android/` into the app's Gradle build. Only
    // the Android build has one.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        tauri_plugin::Builder::new(&[])
            .android_path("android")
            .build();
    }
}
