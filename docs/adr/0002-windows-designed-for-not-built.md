# Windows is designed for, not built, in v1

Windows is a target platform, but there is no machine to test on, and an RFCOMM adapter that has never run would be sending writes to earbud firmware blind. In v1 the platform-independent crates (vendor protocols, device model) build and run their tests on Windows in CI, which keeps platform-specific code out of them, and no Windows Bluetooth adapter is written. The adapter (WinRT `RfcommDeviceService` via the `windows` crate) gets written once someone can test it on real hardware.
