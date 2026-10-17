# A Samsung Device is gated by its OS name, then confirmed by its extended status

The spec wants the Model identified without sending any writes, but a Galaxy Buds Device only reveals its device id (`EXTENDED_STATUS_UPDATED`, bytes 14–17) after the app sends the `MANAGER_INFO` handshake: in no capture does it send that status unprompted. So identification has two stages, and each Samsung Model on the allowlist has a name pattern and a set of device ids for them:

| Model            | Name contains              | Device ids |
| ---------------- | -------------------------- | ---------- |
| Galaxy Buds4 Pro | "Buds4 Pro"                | 359–361    |
| Galaxy Buds2     | "Buds2", but not "Buds2 Pro" | 313–321    |

First, the core only Attaches to a Device whose OS-reported identity points at the allowlist: the Samsung vendor service UUID is present and the name matches a Model's pattern. A Samsung Device with any other name is listed as unsupported and is never Attached, so it's sent nothing. A pattern excludes a sibling Model whose name contains it, such as the Galaxy Buds2 Pro, rather than leaving the handshake to reject it. Second, a name-matched Device is sent `MANAGER_INFO`, the one write allowed before identification. Samsung's own app sends the same handshake, and it changes no settings. If the device id in the reply isn't one of a supported Model's, the core closes the link without any further write; the device id, not the name, decides the Model. The cost: a Device that the listener has renamed so the name no longer matches its Model's pattern isn't Attached.
