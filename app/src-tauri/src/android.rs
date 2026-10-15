//! The JNI symbols for `NativeCore` in the platform-android crate's Kotlin
//! library. They live in this crate, the cdylib, so the linker keeps them.

use std::path::PathBuf;

use headphone_bridge_platform_android::android;
use jni::objects::{JClass, JString};
use jni::sys::jlong;
use jni::JNIEnv;

#[no_mangle]
pub extern "system" fn Java_dev_matthewbpt_headphonebridge_bluetooth_NativeCore_start(
    mut env: JNIEnv,
    class: JClass,
    frame_log: JString,
) {
    let frame_log = (!frame_log.is_null())
        .then(|| env.get_string(&frame_log).ok().map(String::from))
        .flatten()
        .map(PathBuf::from);
    android::start(&mut env, &class, frame_log.as_deref());
}

#[no_mangle]
pub extern "system" fn Java_dev_matthewbpt_headphonebridge_bluetooth_NativeCore_event(
    mut env: JNIEnv,
    _class: JClass,
    json: JString,
) {
    if let Ok(json) = env.get_string(&json) {
        android::event(&String::from(json));
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_matthewbpt_headphonebridge_bluetooth_NativeCore_command(
    mut env: JNIEnv,
    _class: JClass,
    request: jlong,
    json: JString,
) {
    if let Ok(json) = env.get_string(&json) {
        android::command(request, &String::from(json));
    }
}
