# The Rust core calls NativeCore.fromCore through JNI, and Tauri creates
# BluetoothPlugin by name.
-keep class dev.matthewbpt.headphonebridge.bluetooth.NativeCore { *; }
-keep class dev.matthewbpt.headphonebridge.bluetooth.BluetoothPlugin { *; }
